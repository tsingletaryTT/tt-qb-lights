//! Off-thread polling and the pure decision state machine that keeps
//! tt-qb-lights from amplifying a chip fault into a whole-box lockup.
//!
//! Background: on tt-quietbox the driver's hwmon telemetry path can block for
//! seconds/minutes under compute load, and once a chip's ARC-NOC path drops the
//! readings collapse to all-ones sentinels. A naive poller that stalls on such a
//! read — or piles up multiple blocked reads — was documented as an *amplifier*
//! of the resulting whole-box lockup (see ~/qb2-debug reports 2026-05-28,
//! 2026-07-12, 2026-07-14). This module makes that structurally impossible.

use crate::monitoring::{DeviceMetrics, HardwareMonitor};
use crate::rgb::RgbColor;
use std::sync::Arc;
use std::time::Duration;

/// Timing and thresholds for the controller, derived from `MonitoringConfig`.
#[derive(Debug, Clone)]
pub struct PollPolicy {
    pub base_interval: Duration,
    pub max_interval: Duration,
    pub read_timeout: Duration,
    pub backoff_multiplier: f32,
    pub sentinel_temp_c: f32,
    pub sentinel_power_w: f32,
    pub fault_dim_after: Duration,
    pub fault_brightness: f32,
}

/// What to render on the LEDs this iteration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Display {
    pub color: RgbColor,
    pub brightness: f32,
}

/// Classified outcome of one poll attempt. `Good` carries the already-mapped
/// display so the controller stays free of color-mapping concerns.
#[derive(Debug, Clone, PartialEq)]
pub enum PollResult {
    Good(Display),
    Sentinel,
    Error,
    Timeout,
}

/// Pure decision state machine: interval backoff, fault tracking, and the
/// hold-last-good→dim display policy. No clock, hardware, or async deps — the
/// caller passes monotonic `now` (time since program start), so it is directly
/// unit-testable.
pub struct PollController {
    policy: PollPolicy,
    interval: Duration,
    fault_since: Option<Duration>,
    last_good: Option<Display>,
}

impl PollController {
    pub fn new(policy: PollPolicy) -> Self {
        let interval = policy.base_interval;
        Self {
            policy,
            interval,
            fault_since: None,
            last_good: None,
        }
    }

    /// Current interval to wait before the next poll (grows under fault).
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Feed a poll result observed at monotonic time `now`; returns the display
    /// to render, or `None` if there is nothing to show yet (fault before any
    /// successful poll).
    pub fn record(&mut self, result: PollResult, now: Duration) -> Option<Display> {
        match result {
            PollResult::Good(display) => {
                // Healthy: reset backoff and fault state, remember last-good.
                self.interval = self.policy.base_interval;
                self.fault_since = None;
                self.last_good = Some(display);
                Some(display)
            }
            PollResult::Sentinel | PollResult::Error | PollResult::Timeout => {
                // Trouble: grow the interval geometrically, capped, so we stop
                // hammering a contended/wedged box (self-triggered auto-pause).
                let grown = self.interval.mul_f32(self.policy.backoff_multiplier);
                self.interval = grown.min(self.policy.max_interval);

                // Mark when the fault began so we can dim after it persists.
                let fault_start = *self.fault_since.get_or_insert(now);

                match self.last_good {
                    None => None, // nothing valid ever seen — show nothing
                    Some(good) => {
                        let fault_age = now.saturating_sub(fault_start);
                        if fault_age >= self.policy.fault_dim_after {
                            // Sustained fault: hold color, drop to the idle floor
                            // so a stuck-bright bug is visually obvious.
                            Some(Display {
                                color: good.color,
                                brightness: self.policy.fault_brightness,
                            })
                        } else {
                            // Brief fault: hold last-good unchanged.
                            Some(good)
                        }
                    }
                }
            }
        }
    }
}

/// Raw outcome of an off-thread poll attempt (before sentinel classification).
#[derive(Debug)]
pub enum PollOutcome {
    Good(Vec<DeviceMetrics>),
    Error,
    Timeout,
}

/// Drives `HardwareMonitor::poll_metrics` on a tokio blocking thread with a
/// per-call timeout, enforcing a single-outstanding-reader invariant.
///
/// A hwmon read stuck in uninterruptible `D` state cannot be killed. By reusing
/// one in-flight blocking task and never starting a second while one is
/// outstanding, at most ONE blocked read ever exists for this process — the
/// opposite of the pile-up that starved the workqueue in the 07-13 lockup.
pub struct AsyncPoller {
    monitor: Arc<dyn HardwareMonitor>,
    in_flight: Option<tokio::task::JoinHandle<anyhow::Result<Vec<DeviceMetrics>>>>,
}

impl AsyncPoller {
    pub fn new(monitor: Arc<dyn HardwareMonitor>) -> Self {
        Self {
            monitor,
            in_flight: None,
        }
    }

    /// Poll once, waiting at most `read_timeout`. If a prior poll is still
    /// blocked, re-awaits it instead of starting a new one.
    pub async fn poll_once(&mut self, read_timeout: Duration) -> PollOutcome {
        if self.in_flight.is_none() {
            let monitor = Arc::clone(&self.monitor);
            self.in_flight = Some(tokio::task::spawn_blocking(move || monitor.poll_metrics()));
        }

        // `&mut JoinHandle` is a Future (JoinHandle is Unpin); on timeout the
        // handle is left intact so the next call re-awaits the same read.
        let handle = self.in_flight.as_mut().unwrap();
        match tokio::time::timeout(read_timeout, handle).await {
            Ok(join_result) => {
                self.in_flight = None;
                match join_result {
                    Ok(Ok(metrics)) => PollOutcome::Good(metrics),
                    Ok(Err(_read_err)) => PollOutcome::Error,
                    Err(_join_err) => PollOutcome::Error, // task panicked/cancelled
                }
            }
            Err(_elapsed) => PollOutcome::Timeout,
        }
    }
}

#[cfg(test)]
mod controller_tests {
    use super::*;

    fn policy() -> PollPolicy {
        PollPolicy {
            base_interval: Duration::from_secs(3),
            max_interval: Duration::from_secs(60),
            read_timeout: Duration::from_millis(750),
            backoff_multiplier: 2.0,
            sentinel_temp_c: 150.0,
            sentinel_power_w: 1000.0,
            fault_dim_after: Duration::from_secs(30),
            fault_brightness: 0.1,
        }
    }

    fn disp(brightness: f32) -> Display {
        Display {
            color: RgbColor::new(10, 20, 30),
            brightness,
        }
    }

    #[test]
    fn good_poll_shows_mapped_display_and_keeps_base_interval() {
        let mut c = PollController::new(policy());
        let out = c.record(PollResult::Good(disp(0.8)), Duration::from_secs(0));
        assert_eq!(out, Some(disp(0.8)));
        assert_eq!(c.interval(), Duration::from_secs(3));
    }

    #[test]
    fn trouble_backs_off_interval_geometrically_and_caps() {
        let mut c = PollController::new(policy());
        c.record(PollResult::Timeout, Duration::from_secs(0));
        assert_eq!(c.interval(), Duration::from_secs(6));
        c.record(PollResult::Timeout, Duration::from_secs(1));
        assert_eq!(c.interval(), Duration::from_secs(12));
        // keep going until we would exceed 60s; assert it caps at 60
        for i in 2..10 {
            c.record(PollResult::Error, Duration::from_secs(i));
        }
        assert_eq!(c.interval(), Duration::from_secs(60));
    }

    #[test]
    fn one_good_poll_resets_backoff() {
        let mut c = PollController::new(policy());
        c.record(PollResult::Timeout, Duration::from_secs(0));
        c.record(PollResult::Timeout, Duration::from_secs(1));
        assert_eq!(c.interval(), Duration::from_secs(12));
        c.record(PollResult::Good(disp(0.5)), Duration::from_secs(2));
        assert_eq!(c.interval(), Duration::from_secs(3));
    }

    #[test]
    fn fault_holds_last_good_then_dims_after_threshold() {
        let mut c = PollController::new(policy());
        // establish last-good bright display at t=0
        c.record(PollResult::Good(disp(0.9)), Duration::from_secs(0));
        // fault begins at t=1: hold last-good (still 0.9)
        let held = c.record(PollResult::Sentinel, Duration::from_secs(1));
        assert_eq!(held, Some(disp(0.9)));
        // still within dim window at t=20 (fault age 19s < 30s): still held
        let still = c.record(PollResult::Timeout, Duration::from_secs(20));
        assert_eq!(still, Some(disp(0.9)));
        // fault age crosses 30s at t=31 (age 30s): dim to fault_brightness, same color
        let dimmed = c.record(PollResult::Timeout, Duration::from_secs(31)).unwrap();
        assert_eq!(dimmed.color, RgbColor::new(10, 20, 30));
        assert_eq!(dimmed.brightness, 0.1);
    }

    #[test]
    fn fault_before_any_good_shows_nothing() {
        let mut c = PollController::new(policy());
        assert_eq!(c.record(PollResult::Error, Duration::from_secs(0)), None);
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    use anyhow::Result;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock monitor whose poll behavior is configurable for tests.
    struct MockMonitor {
        calls: Arc<AtomicUsize>,
        mode: MockMode,
    }
    enum MockMode {
        FastGood,
        Slow(Duration), // blocks this long (simulates a D-state read)
        Erroring,
    }
    impl HardwareMonitor for MockMonitor {
        fn poll_metrics(&self) -> Result<Vec<DeviceMetrics>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match &self.mode {
                MockMode::FastGood => Ok(vec![sample_metrics()]),
                MockMode::Slow(d) => {
                    std::thread::sleep(*d);
                    Ok(vec![sample_metrics()])
                }
                MockMode::Erroring => anyhow::bail!("simulated read error"),
            }
        }
        fn source_name(&self) -> &str {
            "mock"
        }
    }

    fn sample_metrics() -> DeviceMetrics {
        DeviceMetrics {
            bus_id: "0000:01:00.0".into(),
            architecture: "blackhole".into(),
            board_type: "p300c".into(),
            asic_temp: 45.0,
            power_watts: 60.0,
            tdp_watts: 300.0,
            fan_rpm: 0,
            gddr_temps: vec![],
            max_temp: 45.0,
            power_utilization: 0.2,
        }
    }

    #[tokio::test]
    async fn fast_good_returns_metrics() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mon = Arc::new(MockMonitor {
            calls,
            mode: MockMode::FastGood,
        });
        let mut poller = AsyncPoller::new(mon);
        match poller.poll_once(Duration::from_millis(500)).await {
            PollOutcome::Good(m) => assert_eq!(m.len(), 1),
            other => panic!("expected Good, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn erroring_poll_maps_to_error() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mon = Arc::new(MockMonitor {
            calls,
            mode: MockMode::Erroring,
        });
        let mut poller = AsyncPoller::new(mon);
        assert!(matches!(
            poller.poll_once(Duration::from_millis(500)).await,
            PollOutcome::Error
        ));
    }

    #[tokio::test]
    async fn slow_poll_times_out_without_piling_up() {
        // Read blocks for 2s; timeout is 100ms. Repeated poll_once calls must
        // return Timeout quickly and NOT start additional reads (no pile-up).
        let calls = Arc::new(AtomicUsize::new(0));
        let mon = Arc::new(MockMonitor {
            calls: calls.clone(),
            mode: MockMode::Slow(Duration::from_secs(2)),
        });
        let mut poller = AsyncPoller::new(mon);
        for _ in 0..5 {
            assert!(matches!(
                poller.poll_once(Duration::from_millis(100)).await,
                PollOutcome::Timeout
            ));
        }
        // Only one underlying read was ever started despite 5 poll_once calls.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
