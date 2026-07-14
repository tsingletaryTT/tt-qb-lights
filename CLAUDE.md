# tt-qb-lights — project log

RGB lighting controller for Tenstorrent hardware: reads per-chip temperature/power
and drives the motherboard RGB via OpenRGB. Runs as a systemd service on
tsingletaryTT-quietbox. See `README.md` for usage and `config.toml` for tunables.

## 2026-07-14 — Harden poll path (v0.2.0)

Implemented the tt-qb-lights recommendations from
`~/qb2-debug/report-2026-07-14-hardlock-chip3-recurrence.md` (Rec 3 + the
"Was tt-qb-lights causing harm?" analysis). The tool was ruled out as the *cause*
of the recurring chip-3 ARC-NOC hard-locks but flagged as an *amplifier*: its
synchronous hwmon poll (`main.rs`) could stall 8–13 s/iter and pile up D-state
reads, feeding the workqueue-starvation cascade that took the whole box down.

Changes (source-agnostic — protects both the lm-sensors and tt-smi backends):

- **`AsyncPoller`** (`src/monitoring/poller.rs`): off-thread `spawn_blocking` poll
  with a `read_timeout_ms` deadline and a **single-outstanding-reader invariant**
  (never a second read while one is stuck) — the loop can no longer stall or pile
  up. A stuck read holds exactly one blocking-pool thread; the loop keeps moving.
- **`PollController`** (same file): pure, clock-free state machine for geometric
  interval backoff (self-triggered auto-pause under contention), fault tracking,
  and hold-last-good→dim display. Unit-tested directly with monotonic `now`.
- **`DeviceMetrics::is_plausible`** (`src/monitoring/mod.rs`): rejects the 0xFFFF
  sentinels (65536 °C / 4294 W) so fault telemetry never drives the LEDs.
- **Config knobs** (`src/config.rs`, `[monitoring]`): `read_timeout_ms`,
  `max_poll_interval_ms`, `backoff_multiplier`, `sentinel_temp_c`,
  `sentinel_power_w`, `fault_dim_after_ms`, `fault_brightness` — all
  `#[serde(default)]`, so pre-existing installed configs keep parsing unchanged.
- Docs: README "Safe polling on fragile hardware" section; `config.toml` knob docs
  + the belt-and-suspenders `systemctl stop tt-qb-lights` guidance for heavy runs.

Design doc: `docs/design/2026-07-14-harden-poll-path.md`.

**Prompting note:** original ask was "the latest report has recommendations for
tt-qb-lights — implement them." Constraint given mid-task: **no TT hardware access**
(no `tt-smi`, no `tt-smi -r`) because card 924055 is fragile. Verified entirely with
`cargo test` against mock `HardwareMonitor`s — no live poll run. 66 lib tests pass.
