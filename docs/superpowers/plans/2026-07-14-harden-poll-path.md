# Harden Poll Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make tt-qb-lights structurally incapable of amplifying a chip fault into a whole-box lockup, regardless of telemetry source.

**Architecture:** Move hardware polling onto a single reusable off-thread task with a per-poll timeout and a single-outstanding-reader invariant (never a second poll while one is stuck in D-state). A pure `PollController` state machine owns interval backoff, sentinel/fault classification, and hold-last-good→dim display logic; `main.rs` becomes a thin async driver.

**Tech Stack:** Rust, tokio (`spawn_blocking`, `time::timeout`), serde/toml, anyhow, tracing.

## Global Constraints

- **No TT hardware access during development.** Do not run `tt-smi` (especially not `tt-smi -r`). Verify only with `cargo build` and `cargo test` against mock monitors.
- **Do not `git commit` or `git push` unless the operator asks.** The commit step in each task is the intended staging/grouping; when commits are not yet authorized, run `git add` and pause instead of committing.
- New config fields MUST use `#[serde(default = ...)]` so existing installed `config.toml` files keep parsing unchanged.
- Version bump required: `Cargo.toml` `0.1.0` → `0.2.0`.
- Follow existing code style: deeply commented, honest comments.

---

### Task 1: Sentinel / implausible-value detection on `DeviceMetrics`

**Files:**
- Modify: `src/monitoring/mod.rs` (add method on `DeviceMetrics`, add test module)

**Interfaces:**
- Produces: `DeviceMetrics::is_plausible(&self, sentinel_temp_c: f32, sentinel_power_w: f32) -> bool` — `true` when the reading is a real measurement, `false` when it matches a sentinel/implausible value (e.g. 65536 °C or 4294 W `0xFFFF`/`u32::MAX` sentinels).

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src/monitoring/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(max_temp: f32, power_watts: f32) -> DeviceMetrics {
        DeviceMetrics {
            bus_id: "0000:04:00.0".to_string(),
            architecture: "blackhole".to_string(),
            board_type: "p300c".to_string(),
            asic_temp: max_temp,
            power_watts,
            tdp_watts: 300.0,
            fan_rpm: 0,
            gddr_temps: vec![],
            max_temp,
            power_utilization: 0.0,
        }
    }

    #[test]
    fn plausible_normal_reading_passes() {
        assert!(metrics(47.0, 67.0).is_plausible(150.0, 1000.0));
    }

    #[test]
    fn implausible_temp_sentinel_fails() {
        // 65536 °C = 0xFFFF NOC sentinel observed in the 07-13 lockup
        assert!(!metrics(65536.0, 67.0).is_plausible(150.0, 1000.0));
    }

    #[test]
    fn implausible_power_sentinel_fails() {
        // 4294 W = u32::MAX microwatts sentinel
        assert!(!metrics(47.0, 4294.0).is_plausible(150.0, 1000.0));
    }

    #[test]
    fn at_threshold_is_implausible() {
        assert!(!metrics(150.0, 67.0).is_plausible(150.0, 1000.0));
        assert!(!metrics(47.0, 1000.0).is_plausible(150.0, 1000.0));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ~/code/tt-qb-lights && cargo test -p tt-qb-lights is_plausible 2>&1 | tail -20` (also runs via module name)
Expected: FAIL — `no method named is_plausible found for struct DeviceMetrics`.

- [ ] **Step 3: Write minimal implementation**

Add inside `impl DeviceMetrics` in `src/monitoring/mod.rs` (after `is_overheating`):

```rust
    /// Returns `true` when this reading looks like a genuine measurement, `false`
    /// when it matches a telemetry sentinel or is otherwise implausible.
    ///
    /// When a Blackhole chip's ARC-NOC path drops (see qb2-debug reports), hwmon
    /// telemetry collapses to all-ones sentinels — 65536 °C (0xFFFF) and
    /// ~4294 W (u32::MAX microwatts). Feeding those into color mapping would drive
    /// garbage output, so callers use this to hold last-good instead.
    pub fn is_plausible(&self, sentinel_temp_c: f32, sentinel_power_w: f32) -> bool {
        self.max_temp < sentinel_temp_c && self.power_watts < sentinel_power_w
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ~/code/tt-qb-lights && cargo test 2>&1 | tail -20`
Expected: PASS (all tests, including the 4 new ones).

- [ ] **Step 5: Commit** (only if commits authorized — otherwise `git add` and pause)

```bash
cd ~/code/tt-qb-lights
git add src/monitoring/mod.rs
git commit -m "feat(monitoring): add DeviceMetrics::is_plausible sentinel guard"
```

---

### Task 2: Config knobs + validation

**Files:**
- Modify: `src/config.rs` (add fields to `MonitoringConfig`, default fns, validation, fix `create_valid_config` test helper, add validation tests)

**Interfaces:**
- Produces on `MonitoringConfig`: `read_timeout_ms: u64`, `max_poll_interval_ms: u64`, `backoff_multiplier: f32`, `sentinel_temp_c: f32`, `sentinel_power_w: f32`, `fault_dim_after_ms: u64`, `fault_brightness: f32` (all with serde defaults).

- [ ] **Step 1: Write the failing test**

Add these tests to the `tests` module in `src/config.rs`:

```rust
    #[test]
    fn test_monitoring_defaults_apply_to_minimal_config() {
        // A config.toml written before these knobs existed must still parse,
        // filling defaults.
        let toml = r#"
[monitoring]
poll_interval_ms = 3000
source = "lm-sensors"

[openrgb]
server_host = "127.0.0.1"
server_port = 6742
device_name = "X"
zone_strategy = "unified"

[color_mapping]
scheme = "s"
[[color_mapping.schemes.s]]
temp = 20
color = "#00FF00"
[[color_mapping.schemes.s]]
temp = 70
color = "#FF0000"

[effects]
enable_power_brightness = true
enable_warning_pulse = true
pulse_speed_ms = 500
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.monitoring.read_timeout_ms, 750);
        assert_eq!(cfg.monitoring.max_poll_interval_ms, 60000);
        assert_eq!(cfg.monitoring.backoff_multiplier, 2.0);
        assert_eq!(cfg.monitoring.sentinel_temp_c, 150.0);
        assert_eq!(cfg.monitoring.sentinel_power_w, 1000.0);
        assert_eq!(cfg.monitoring.fault_dim_after_ms, 30000);
        assert_eq!(cfg.monitoring.fault_brightness, 0.1);
        cfg.validate().unwrap();
    }

    #[test]
    fn test_invalid_backoff_multiplier_rejected() {
        let mut config = create_valid_config();
        config.monitoring.backoff_multiplier = 0.5; // must be >= 1.0
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_invalid_fault_brightness_rejected() {
        let mut config = create_valid_config();
        config.monitoring.fault_brightness = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_max_interval_below_base_rejected() {
        let mut config = create_valid_config();
        config.monitoring.poll_interval_ms = 5000;
        config.monitoring.max_poll_interval_ms = 1000;
        assert!(config.validate().is_err());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ~/code/tt-qb-lights && cargo test 2>&1 | tail -25`
Expected: FAIL — `create_valid_config` literal missing new fields (compile error) and/or missing fields on `MonitoringConfig`.

- [ ] **Step 3: Write minimal implementation**

In `src/config.rs`, extend `MonitoringConfig`:

```rust
pub struct MonitoringConfig {
    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,

    /// Data source: "tt-smi" or "lm-sensors"
    pub source: MonitoringSource,

    /// How long the main loop waits for a single poll before treating it as
    /// slow/blocked and proceeding (holding last-good). Guards against the
    /// blocking-hwmon D-state stall (qb2-debug report-2026-05-28).
    #[serde(default = "default_read_timeout_ms")]
    pub read_timeout_ms: u64,

    /// Upper bound on the (backed-off) poll interval.
    #[serde(default = "default_max_poll_interval_ms")]
    pub max_poll_interval_ms: u64,

    /// Factor the poll interval grows by after each troubled poll.
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f32,

    /// ASIC temperature (°C) at or above which a reading is treated as a
    /// sentinel/fault rather than a real measurement.
    #[serde(default = "default_sentinel_temp_c")]
    pub sentinel_temp_c: f32,

    /// Power (W) at or above which a reading is treated as a sentinel/fault.
    #[serde(default = "default_sentinel_power_w")]
    pub sentinel_power_w: f32,

    /// How long a fault must persist before the lights dim from last-good.
    #[serde(default = "default_fault_dim_after_ms")]
    pub fault_dim_after_ms: u64,

    /// Brightness floor shown while a fault persists past `fault_dim_after_ms`.
    #[serde(default = "default_fault_brightness")]
    pub fault_brightness: f32,
}

fn default_read_timeout_ms() -> u64 { 750 }
fn default_max_poll_interval_ms() -> u64 { 60000 }
fn default_backoff_multiplier() -> f32 { 2.0 }
fn default_sentinel_temp_c() -> f32 { 150.0 }
fn default_sentinel_power_w() -> f32 { 1000.0 }
fn default_fault_dim_after_ms() -> u64 { 30000 }
fn default_fault_brightness() -> f32 { 0.1 }
```

Add to `validate()` (before the final `Ok(())`):

```rust
        // Validate poll-hardening knobs
        if self.monitoring.backoff_multiplier < 1.0 {
            anyhow::bail!("backoff_multiplier must be >= 1.0");
        }
        if self.monitoring.fault_brightness < 0.0 || self.monitoring.fault_brightness > 1.0 {
            anyhow::bail!("fault_brightness must be between 0.0 and 1.0");
        }
        if self.monitoring.max_poll_interval_ms < self.monitoring.poll_interval_ms {
            anyhow::bail!("max_poll_interval_ms must be >= poll_interval_ms");
        }
```

Update the `create_valid_config()` helper's `MonitoringConfig { .. }` literal to include the new fields:

```rust
            monitoring: MonitoringConfig {
                poll_interval_ms: 1000,
                source: MonitoringSource::LmSensors,
                read_timeout_ms: 750,
                max_poll_interval_ms: 60000,
                backoff_multiplier: 2.0,
                sentinel_temp_c: 150.0,
                sentinel_power_w: 1000.0,
                fault_dim_after_ms: 30000,
                fault_brightness: 0.1,
            },
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ~/code/tt-qb-lights && cargo test 2>&1 | tail -25`
Expected: PASS.

- [ ] **Step 5: Commit** (only if authorized)

```bash
cd ~/code/tt-qb-lights
git add src/config.rs
git commit -m "feat(config): add poll-hardening knobs with serde defaults"
```

---

### Task 3: `PollController` pure state machine

**Files:**
- Create: `src/monitoring/poller.rs`
- Modify: `src/monitoring/mod.rs` (add `pub mod poller;`)

**Interfaces:**
- Consumes: `DeviceMetrics` (Task 1), `crate::rgb::RgbColor`.
- Produces:
  - `pub struct PollPolicy { pub base_interval: Duration, pub max_interval: Duration, pub read_timeout: Duration, pub backoff_multiplier: f32, pub sentinel_temp_c: f32, pub sentinel_power_w: f32, pub fault_dim_after: Duration, pub fault_brightness: f32 }`
  - `pub struct Display { pub color: RgbColor, pub brightness: f32 }` (derive `Clone, Copy, Debug, PartialEq`)
  - `pub enum PollResult { Good(Display), Sentinel, Error, Timeout }`
  - `pub struct PollController` with `pub fn new(policy: PollPolicy) -> Self`, `pub fn interval(&self) -> Duration`, `pub fn record(&mut self, result: PollResult, now: Duration) -> Option<Display>`.
- `now` is monotonic time since program start (caller passes `start.elapsed()`), so the controller has **no** clock dependency and is directly unit-testable.

- [ ] **Step 1: Write the failing test**

Create `src/monitoring/poller.rs` with the test module first (implementation stub follows in Step 3):

```rust
//! Off-thread polling and the pure decision state machine that keeps
//! tt-qb-lights from amplifying a chip fault into a whole-box lockup.

use crate::monitoring::{DeviceMetrics, HardwareMonitor};
use crate::rgb::RgbColor;
use std::sync::Arc;
use std::time::Duration;

// ---- (implementation goes here in Step 3) ----

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
        Display { color: RgbColor::new(10, 20, 30), brightness }
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
```

- [ ] **Step 2: Run test to verify it fails**

Also add `pub mod poller;` to `src/monitoring/mod.rs` (after `pub mod tenstorrent;`).
Run: `cd ~/code/tt-qb-lights && cargo test 2>&1 | tail -25`
Expected: FAIL — types (`PollPolicy`, `Display`, `PollResult`, `PollController`) not defined.

- [ ] **Step 3: Write minimal implementation**

Insert this implementation into `src/monitoring/poller.rs` at the `// ---- (implementation goes here in Step 3) ----` marker:

```rust
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
/// caller passes monotonic `now` (time since program start).
pub struct PollController {
    policy: PollPolicy,
    interval: Duration,
    fault_since: Option<Duration>,
    last_good: Option<Display>,
}

impl PollController {
    pub fn new(policy: PollPolicy) -> Self {
        let interval = policy.base_interval;
        Self { policy, interval, fault_since: None, last_good: None }
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ~/code/tt-qb-lights && cargo test 2>&1 | tail -25`
Expected: PASS (5 new controller tests).

- [ ] **Step 5: Commit** (only if authorized)

```bash
cd ~/code/tt-qb-lights
git add src/monitoring/poller.rs src/monitoring/mod.rs
git commit -m "feat(monitoring): add PollController backoff/fault state machine"
```

---

### Task 4: `AsyncPoller` off-thread worker with single-outstanding-reader invariant

**Files:**
- Modify: `src/monitoring/poller.rs` (add `AsyncPoller` + `PollOutcome` + async tests with a mock monitor)

**Interfaces:**
- Consumes: `Arc<dyn HardwareMonitor>`.
- Produces:
  - `pub enum PollOutcome { Good(Vec<DeviceMetrics>), Error, Timeout }`
  - `pub struct AsyncPoller` with `pub fn new(monitor: Arc<dyn HardwareMonitor>) -> Self` and `pub async fn poll_once(&mut self, read_timeout: Duration) -> PollOutcome`.
- Invariant: `poll_once` issues a new `spawn_blocking` poll only when none is in flight; a stuck poll keeps its handle and is re-awaited (never a second concurrent read).

- [ ] **Step 1: Write the failing test**

Append to `src/monitoring/poller.rs`:

```rust
#[cfg(test)]
mod worker_tests {
    use super::*;
    use crate::monitoring::DeviceMetrics;
    use anyhow::Result;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock monitor whose poll behavior is configurable for tests.
    struct MockMonitor {
        calls: Arc<AtomicUsize>,
        mode: MockMode,
    }
    #[derive(Clone)]
    enum MockMode {
        FastGood,
        Slow(Duration), // blocks this long (simulates D-state read)
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
        fn source_name(&self) -> &str { "mock" }
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
        let mon = Arc::new(MockMonitor { calls: calls.clone(), mode: MockMode::FastGood });
        let mut poller = AsyncPoller::new(mon);
        match poller.poll_once(Duration::from_millis(500)).await {
            PollOutcome::Good(m) => assert_eq!(m.len(), 1),
            other => panic!("expected Good, got {:?}", std::mem::discriminant(&other) as *const _ as usize),
        }
    }

    #[tokio::test]
    async fn erroring_poll_maps_to_error() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mon = Arc::new(MockMonitor { calls, mode: MockMode::Erroring });
        let mut poller = AsyncPoller::new(mon);
        assert!(matches!(poller.poll_once(Duration::from_millis(500)).await, PollOutcome::Error));
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd ~/code/tt-qb-lights && cargo test 2>&1 | tail -25`
Expected: FAIL — `AsyncPoller` / `PollOutcome` not defined.

- [ ] **Step 3: Write minimal implementation**

Add to `src/monitoring/poller.rs` (after `PollController`):

```rust
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
        Self { monitor, in_flight: None }
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd ~/code/tt-qb-lights && cargo test 2>&1 | tail -25`
Expected: PASS. The `slow_poll_times_out_without_piling_up` test confirms `calls == 1` after 5 `poll_once` calls.

- [ ] **Step 5: Commit** (only if authorized)

```bash
cd ~/code/tt-qb-lights
git add src/monitoring/poller.rs
git commit -m "feat(monitoring): add AsyncPoller with single-outstanding-reader invariant"
```

---

### Task 5: Wire the hardened path into `main.rs`

**Files:**
- Modify: `src/main.rs` (convert monitor to `Arc`, replace the synchronous poll loop with `AsyncPoller` + `PollController`)

**Interfaces:**
- Consumes: `AsyncPoller`, `PollController`, `PollPolicy`, `Display`, `PollResult`, `PollOutcome` (Tasks 3–4); `DeviceMetrics::is_plausible` (Task 1); `MonitoringConfig` knobs (Task 2).

- [ ] **Step 1: Update imports and monitor construction**

In `src/main.rs`, update the monitoring import line and monitor type. Change:

```rust
use monitoring::{sensors::SensorsMonitor, tenstorrent::TtSmiMonitor, HardwareMonitor};
```
to:
```rust
use monitoring::{
    poller::{AsyncPoller, Display, PollController, PollOutcome, PollPolicy, PollResult},
    sensors::SensorsMonitor,
    tenstorrent::TtSmiMonitor,
    HardwareMonitor,
};
use std::sync::Arc;
```

Change the monitor binding from `Box<dyn HardwareMonitor>` to `Arc<dyn HardwareMonitor>`:

```rust
    let monitor: Arc<dyn HardwareMonitor> = match config.monitoring.source {
        config::MonitoringSource::TtSmi => {
            Arc::new(TtSmiMonitor::new().context("Failed to initialize tt-smi monitor")?)
        }
        config::MonitoringSource::LmSensors => {
            Arc::new(SensorsMonitor::new().context("Failed to initialize sensors monitor")?)
        }
    };
```

The initial test poll (`monitor.poll_metrics()?`) and single-shot path stay as-is (they call `poll_metrics` directly on the `Arc`, which derefs fine).

- [ ] **Step 2: Replace the main monitoring loop**

Replace the whole `loop { ... }` block (currently starting at the `loop {` after "Starting main monitoring loop") through its closing brace with:

```rust
    // Build the poll policy from config.
    let policy = PollPolicy {
        base_interval: Duration::from_millis(config.monitoring.poll_interval_ms),
        max_interval: Duration::from_millis(config.monitoring.max_poll_interval_ms),
        read_timeout: Duration::from_millis(config.monitoring.read_timeout_ms),
        backoff_multiplier: config.monitoring.backoff_multiplier,
        sentinel_temp_c: config.monitoring.sentinel_temp_c,
        sentinel_power_w: config.monitoring.sentinel_power_w,
        fault_dim_after: Duration::from_millis(config.monitoring.fault_dim_after_ms),
        fault_brightness: config.monitoring.fault_brightness,
    };
    let read_timeout = policy.read_timeout;
    let mut poller = AsyncPoller::new(Arc::clone(&monitor));
    let mut controller = PollController::new(policy);

    let start = Instant::now();
    let mut loop_count = 0u64;
    let mut last_log_time = Instant::now();
    let mut last_rgb_color: Option<(rgb::RgbColor, f32)> = None;
    let mut last_rgb_update = Instant::now();
    let min_rgb_update_interval = Duration::from_secs(5);
    let mut faulted = false; // for one-shot fault logging

    info!("Starting main monitoring loop (Ctrl+C to stop)");

    loop {
        let loop_start = Instant::now();

        // Poll off-thread with a timeout; never blocks the loop, never piles up.
        let outcome = poller.poll_once(read_timeout).await;
        let now = start.elapsed();

        // Classify the outcome into a PollResult for the controller.
        let result = match outcome {
            PollOutcome::Good(metrics) => {
                let all_plausible = metrics
                    .iter()
                    .all(|m| m.is_plausible(config.monitoring.sentinel_temp_c, config.monitoring.sentinel_power_w));
                if !all_plausible {
                    PollResult::Sentinel
                } else {
                    loop_count += 1;
                    let hottest = metrics
                        .iter()
                        .max_by(|a, b| a.max_temp.partial_cmp(&b.max_temp).unwrap())
                        .unwrap();
                    let color = color_mapper.map_temperature(hottest.max_temp);
                    let brightness = if config.effects.enable_power_brightness {
                        config.effects.min_brightness
                            + (config.effects.max_brightness - config.effects.min_brightness)
                                * hottest.power_utilization
                    } else {
                        config.effects.max_brightness
                    };
                    let final_brightness = if config.effects.enable_warning_pulse
                        && hottest.max_temp >= config.effects.warning_temp_threshold
                    {
                        let pulse_phase = (loop_count as f32 * 0.1).sin() * 0.5 + 0.5;
                        brightness * (0.5 + pulse_phase * 0.5)
                    } else {
                        brightness
                    };

                    if last_log_time.elapsed() >= Duration::from_secs(10) {
                        info!(
                            "Status: {:.1}°C (max) | {:.1}W | RGB: #{:02X}{:02X}{:02X} @ {:.0}%",
                            hottest.max_temp, hottest.power_watts,
                            color.r, color.g, color.b, final_brightness * 100.0
                        );
                        last_log_time = Instant::now();
                    }
                    PollResult::Good(Display { color, brightness: final_brightness })
                }
            }
            PollOutcome::Error => PollResult::Error,
            PollOutcome::Timeout => PollResult::Timeout,
        };

        // One-shot fault/recovery logging (rate-limited by state change).
        let is_trouble = !matches!(result, PollResult::Good(_));
        if is_trouble && !faulted {
            warn!("Telemetry degraded (slow/errored/sentinel) — backing off polling, holding last-good lights");
            faulted = true;
        } else if !is_trouble && faulted {
            info!("Telemetry recovered — resuming normal polling");
            faulted = false;
        }

        // Ask the controller what to display and push it to the LEDs, keeping
        // the existing anti-flicker throttle (color changed + min interval).
        if let Some(display) = controller.record(result, now) {
            let color = display.color;
            let final_brightness = display.brightness;
            let color_changed = if let Some((last_color, last_brightness)) = last_rgb_color {
                (color.r as i16 - last_color.r as i16).abs() > 5
                    || (color.g as i16 - last_color.g as i16).abs() > 5
                    || (color.b as i16 - last_color.b as i16).abs() > 5
                    || (final_brightness - last_brightness).abs() > 0.05
            } else {
                true
            };
            if color_changed && last_rgb_update.elapsed() >= min_rgb_update_interval {
                if let Some(controller_rgb) = rgb_controller.as_mut() {
                    // All zone strategies currently render unified (per-device /
                    // gradient remain TODO as before).
                    if !matches!(config.openrgb.zone_strategy, config::ZoneStrategy::Unified) {
                        warn_once!("Only unified zone strategy is implemented; using unified");
                    }
                    if let Err(e) = controller_rgb.set_all(color, final_brightness) {
                        error!("Failed to update RGB lights: {}", e);
                    } else {
                        last_rgb_color = Some((color, final_brightness));
                        last_rgb_update = Instant::now();
                        info!("Updated RGB to #{:02X}{:02X}{:02X} @ {:.0}%",
                            color.r, color.g, color.b, final_brightness * 100.0);
                    }
                }
            }
        }

        // Sleep the controller's (possibly backed-off) interval, minus time
        // already spent this iteration; wake early on Ctrl+C.
        let elapsed = loop_start.elapsed();
        let interval = controller.interval();
        let sleep_duration = interval.saturating_sub(elapsed).max(Duration::from_millis(10));

        tokio::select! {
            _ = tokio::time::sleep(sleep_duration) => {}
            _ = tokio::signal::ctrl_c() => {
                info!("Received shutdown signal, exiting gracefully");
                break;
            }
        }
    }
```

- [ ] **Step 3: Build and run the full test suite**

Run: `cd ~/code/tt-qb-lights && cargo build 2>&1 | tail -20 && cargo test 2>&1 | tail -25`
Expected: builds clean; all tests pass. Fix any unused-import warnings (e.g. drop `HardwareMonitor` from the import if the compiler flags it as unused — it is still needed for the `Arc<dyn HardwareMonitor>` annotation, so it should remain).

- [ ] **Step 4: Dry-run smoke test (no hardware writes, no tt-smi)**

The default `source = "lm-sensors"` reads sysfs only. Confirm the binary starts, polls, and honors `--dry-run` (no OpenRGB writes). Do NOT use `--single-shot` against real hardware if it would invoke tt-smi; with `lm-sensors` it only reads sysfs.

Run: `cd ~/code/tt-qb-lights && timeout 12 cargo run -- --dry-run --debug 2>&1 | tail -30`
Expected: logs "Starting main monitoring loop", at least one poll cycle, no panic. (If no TT devices are visible via hwmon in this environment it will log a poll error and back off — that is acceptable and itself exercises the backoff path.)

- [ ] **Step 5: Commit** (only if authorized)

```bash
cd ~/code/tt-qb-lights
git add src/main.rs
git commit -m "feat: drive lights through hardened AsyncPoller + PollController"
```

---

### Task 6: Docs, config template, version bump, project log

**Files:**
- Modify: `config.toml` (document new knobs + systemctl note)
- Modify: `README.md` (document safety behavior)
- Modify: `Cargo.toml` (version bump)
- Create or modify: `CLAUDE.md` (repo project log)

- [ ] **Step 1: Document the new knobs in `config.toml`**

In the `[monitoring]` section of `config.toml`, after the `source = "lm-sensors"` block, add:

```toml
# --- Poll-path safety (added v0.2.0) ---
# Background: on tt-quietbox, telemetry reads go through the driver's blocking
# hwmon MMIO path, which can stall for seconds/minutes under compute load
# (see ~/qb2-debug/report-2026-05-28 and report-2026-07-14). These knobs keep
# tt-qb-lights from amplifying such a stall into a system-wide problem.

# Max time (ms) the loop waits for one poll before treating it as slow and
# holding last-good lights. The loop never blocks longer than this.
read_timeout_ms = 750

# When polls go slow/error/sentinel, the interval backs off geometrically by
# this factor, up to the cap below — a self-triggered "pause during heavy runs".
backoff_multiplier = 2.0
max_poll_interval_ms = 60000

# A reading at/above either threshold is treated as a fault sentinel (the
# 65536 °C / 4294 W all-ones values seen when a chip's ARC-NOC path drops) and
# never drives the LED color.
sentinel_temp_c = 150.0
sentinel_power_w = 1000.0

# While a fault persists past this long, hold the last-good color but dim to
# fault_brightness so a stuck-bright state is visually obvious.
fault_dim_after_ms = 30000
fault_brightness = 0.1

# NOTE: These make tt-qb-lights resilient, but during heavy multi-chip runs the
# safest choice is still to pause it entirely: `systemctl stop tt-qb-lights`.
```

- [ ] **Step 2: Document behavior in `README.md`**

Add a section to `README.md` (near the monitoring/configuration docs):

```markdown
## Safe polling on fragile hardware

tt-qb-lights only ever *reads* telemetry and writes RGB over OpenRGB — it never
opens `/dev/tenstorrent` or resets a chip. But on some boxes the driver's hwmon
telemetry path can block for seconds under compute load, and a naive poller that
stalls or piles up reads can amplify a single-chip fault into a system-wide
lockup (see the qb2-debug reports).

To prevent that, polling is hardened as of v0.2.0:

- **Off-thread, time-bounded polling.** Each poll runs on a background thread and
  the main loop waits at most `read_timeout_ms`. A slow read never stalls the loop.
- **Single outstanding reader.** A new poll is never started while a previous one
  is still blocked, so at most one blocked read can ever exist for this process.
- **Sentinel rejection.** All-ones fault telemetry (e.g. 65536 °C / 4294 W) is
  detected and never drives the lights.
- **Adaptive backoff.** Slow/failed/sentinel polls grow the interval up to
  `max_poll_interval_ms`, automatically easing off a contended machine.
- **Fault display.** On trouble the last valid color is held, then dimmed to
  `fault_brightness` after `fault_dim_after_ms` so a degraded state is visible.

During heavy multi-chip workloads, pausing the service entirely
(`systemctl stop tt-qb-lights`) is still the most conservative option.
```

- [ ] **Step 3: Bump the version**

In `Cargo.toml`, change `version = "0.1.0"` to `version = "0.2.0"`.

- [ ] **Step 4: Log the change in `CLAUDE.md`**

If `~/code/tt-qb-lights/CLAUDE.md` does not exist, create it; otherwise append. Add:

```markdown
# tt-qb-lights — project log

## 2026-07-14 — Harden poll path (v0.2.0)
Implemented the tt-qb-lights recommendations from
`~/qb2-debug/report-2026-07-14-hardlock-chip3-recurrence.md` (Rec 3 + amplifier
analysis). The tool was ruled out as the *cause* of the recurring chip-3 ARC-NOC
hard-locks but flagged as an *amplifier*: its synchronous hwmon poll could stall
8–13 s/iter and pile up D-state reads, feeding the workqueue-starvation cascade.

Changes (source-agnostic — protects both lm-sensors and tt-smi backends):
- `AsyncPoller`: off-thread `spawn_blocking` poll with a `read_timeout_ms`
  deadline and a single-outstanding-reader invariant (never a second read while
  one is stuck) — the loop can no longer stall or pile up.
- `PollController`: pure state machine for geometric interval backoff (auto-pause
  under contention), fault tracking, and hold-last-good→dim display.
- `DeviceMetrics::is_plausible`: rejects 0xFFFF sentinels (65536 °C / 4294 W).
- New `[monitoring]` config knobs (serde defaults; existing configs unaffected).
- Docs + `systemctl stop tt-qb-lights` guidance for heavy runs.

Constraint during this work: no TT hardware access (no tt-smi / tt-smi -r);
verified with `cargo test` against mock monitors only.
```

- [ ] **Step 5: Final verification + commit** (commit only if authorized)

```bash
cd ~/code/tt-qb-lights
cargo build --release 2>&1 | tail -10
cargo test 2>&1 | tail -15
git add config.toml README.md Cargo.toml CLAUDE.md
git commit -m "docs: document poll-path hardening; bump to v0.2.0"
```

Expected: release build succeeds; all tests pass.

---

## Self-Review

**Spec coverage:** Off-thread bounded poll → Task 4. Single-outstanding-reader → Task 4 (`slow_poll...piling_up` test). Sentinel detection → Task 1. Adaptive backoff → Task 3. Hold-last-good→dim → Task 3. Config knobs w/ serde defaults → Task 2. PollController isolation/testability → Task 3. Docs/version/log → Task 6. All spec sections covered.

**Placeholder scan:** No TBD/TODO left in new code. The pre-existing per-device/gradient zone strategies remain unimplemented (unchanged behavior, now consolidated behind one `warn_once!`), which is out of scope for this plan.

**Type consistency:** `Display`, `PollResult`, `PollOutcome`, `PollController::record(result, now) -> Option<Display>`, `AsyncPoller::poll_once(read_timeout) -> PollOutcome`, and `is_plausible(sentinel_temp_c, sentinel_power_w)` are used identically across Tasks 1, 3, 4, and 5.
