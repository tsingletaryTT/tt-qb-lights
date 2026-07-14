# tt-qb-lights: non-amplifying poll path

**Date:** 2026-07-14
**Status:** Approved (design)
**Origin:** `~/qb2-debug/report-2026-07-14-hardlock-chip3-recurrence.md`, Recommendation 3 and the
"Was tt-qb-lights causing harm?" analysis. Cross-refs: `report-2026-05-28-hwmon-sysfs-lockup.md`,
`report-2026-07-12-hardlock-chip3-idle-noc.md`.

## Problem

tt-qb-lights did not *cause* the chip-3 ARC-NOC lock-ups, but it was a documented **amplifier**:

1. Its telemetry reads go through the driver's **blocking hwmon MMIO path** (default
   `source = "lm-sensors"` → `/sys/class/hwmon/*/temp1_input`). Under compute load those reads
   block in uninterruptible `D` state (report-2026-05-28: 10 s → 97 s → 479 s per read). The
   leading edge was directly observed on 2026-07-13 at 20:47–20:49 when all four reads failed.
2. Once chip 3 wedged, the poll loop **stalled 8–13 s/iteration**, and reads had nothing stopping
   them from piling up — feeding the workqueue-starvation cascade that took the whole box down.

Root of the amplification in code: `src/main.rs` calls `monitor.poll_metrics()` **synchronously on
the main loop**. A single stuck read stalls everything, and each iteration is free to start another
read on top of the blocked one.

## Goal

Make tt-qb-lights structurally incapable of being this amplifier, **independent of the telemetry
source** (protects both `lm-sensors` and `tt-smi`; note report-2026-05-28 §Root Cause warns
`tt-smi` may hit the same driver path, so a source swap alone is not a fix).

Out of scope: an SMBus/I2C side-channel backend (deferred by the operator); driver/hardware
recommendations #1, #2, #4–6, which are not tt-qb-lights changes.

## Design

### 1. Off-thread bounded poll — `src/monitoring/poller.rs`

A single long-lived **worker thread** owns the `Box<dyn HardwareMonitor>` and services poll
requests over channels. The async main loop requests a poll and waits for the result with a
`read_timeout_ms` deadline.

**Single-outstanding-reader invariant:** the loop never issues a new poll while one is still in
flight. Because a `read()` stuck in `D` state cannot be killed, a *persistent* worker guarantees
**at most one** blocked read ever exists for this process. Spawning a fresh thread per poll would
*be* the pile-up bug. When a stuck read eventually returns, the worker becomes available again and
the loop resumes normal cadence.

Mechanism: request channel (capacity 1) + result channel. The loop tracks an `in_flight` flag; it
sends a request only when `!in_flight`, then `select!`s the result receiver against a timeout. On
timeout it leaves `in_flight = true` and proceeds without blocking; a later iteration drains a
late-arriving result and clears the flag.

### 2. Sentinel / implausible-value detection — `src/monitoring/mod.rs`

`DeviceMetrics::is_plausible(&self, sentinel_temp_c, sentinel_power_w) -> bool`. A reading is a
fault if `max_temp >= sentinel_temp_c` (default 150.0 — catches the 65536 °C `0xFFFF` sentinel) or
`power_watts >= sentinel_power_w` (default 1000.0 — catches the 4294 W `u32::MAX µW` sentinel). A
poll containing any implausible device is treated as a fault. Sentinel values never reach color
mapping.

### 3. Adaptive backoff = self-triggered auto-pause — `PollController`

Consecutive trouble polls (slow/timeout/error/sentinel) multiply the interval by
`backoff_multiplier` (default 2.0) up to `max_poll_interval_ms` (default 60000), starting from the
configured `poll_interval_ms` (3000). One clean, fast, plausible poll resets to base. This is the
"pause during heavy runs" behavior, self-triggered by observed contention.

### 4. Fault lights: hold last-good, then dim

- Normal: map telemetry → color as today.
- Fault (slow/timeout/error/sentinel): hold the last valid color.
- Sustained fault ≥ `fault_dim_after_ms` (default 30000): fade brightness to `fault_brightness`
  (default 0.1) so a stuck-bright bug is visually obvious without being alarming.
- Recovery: resume normal mapping.

### 5. Config knobs — `src/config.rs`, `[monitoring]`

All new fields use `#[serde(default = ...)]`, so existing installed configs keep working unchanged:

| Field | Default | Purpose |
|-------|---------|---------|
| `read_timeout_ms` | 750 | Loop's wait deadline for one poll |
| `max_poll_interval_ms` | 60000 | Backoff ceiling |
| `backoff_multiplier` | 2.0 | Interval growth per trouble poll |
| `sentinel_temp_c` | 150.0 | Implausible-temperature threshold |
| `sentinel_power_w` | 1000.0 | Implausible-power threshold |
| `fault_dim_after_ms` | 30000 | Hold-last-good duration before dimming |
| `fault_brightness` | 0.1 | Idle floor while faulted |

### 6. Boundaries & testability

Loop decision logic is extracted into a pure `PollController` state machine (current interval,
consecutive-trouble count, fault-start time, last-good color/brightness). It has **no** hardware,
tokio, or clock dependencies — time is passed in — so it is unit-testable directly. `src/main.rs`
becomes a thin driver: ask the controller what to do, drive the worker thread, apply the result.

## Testing

Pure-logic unit tests (no hardware — `tt-smi` must not run):

- `is_plausible`: normal reading passes; 65536 °C and 4294 W sentinels fail; boundary values.
- `PollController` backoff: base → ×2 → … → capped at max; resets after one clean poll.
- Fault state machine: good→fault holds last-good; sustained fault dims after threshold; recovery
  restores mapping.
- Off-thread poll behavior via a mock `HardwareMonitor` (fast-good / slow / sentinel / error) —
  verify the loop never blocks past `read_timeout_ms` and never issues overlapping polls.

Verification: `cargo build --release` and `cargo test`. No TT hardware access.

## Docs & versioning

- `config.toml`: document new knobs; add the belt-and-suspenders note recommending
  `systemctl stop tt-qb-lights` during heavy multi-chip runs (report Rec 3, operational half).
- `README.md`: document the watchdog / backoff / fault behavior and the safety rationale.
- Bump `Cargo.toml` version `0.1.0` → `0.2.0`.
- Log the change in the repo `CLAUDE.md`.
