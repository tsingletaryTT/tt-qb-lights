// Hardware monitoring module
// Collects temperature, power, and utilization metrics from Tenstorrent devices

pub mod poller;
pub mod sensors;
pub mod tenstorrent;

use anyhow::Result;

/// Represents a single Tenstorrent device's metrics
#[derive(Debug, Clone)]
pub struct DeviceMetrics {
    /// PCI bus ID (e.g., "0000:01:00.0")
    pub bus_id: String,

    /// Device architecture (grayskull, wormhole_b0, blackhole)
    pub architecture: String,

    /// Board type (e.g., "p300c", "p150")
    pub board_type: String,

    /// Main ASIC temperature in Celsius
    pub asic_temp: f32,

    /// Power consumption in watts
    pub power_watts: f32,

    /// Thermal Design Power (TDP) in watts
    pub tdp_watts: f32,

    /// Fan speed in RPM (0 if passive cooling)
    pub fan_rpm: u32,

    /// Individual GDDR memory temperatures (if available)
    pub gddr_temps: Vec<f32>,

    /// Maximum temperature across all sensors
    pub max_temp: f32,

    /// Power utilization as percentage of TDP (0.0 to 1.0)
    pub power_utilization: f32,
}

impl DeviceMetrics {
    /// Calculate the overall thermal load (0.0 = cool, 1.0 = very hot)
    /// This is used for color mapping
    pub fn thermal_load(&self, base_temp: f32, max_temp: f32) -> f32 {
        ((self.max_temp - base_temp) / (max_temp - base_temp))
            .max(0.0)
            .min(1.0)
    }

    /// Check if device is in warning state (overheating)
    pub fn is_overheating(&self, threshold: f32) -> bool {
        self.max_temp >= threshold
    }

    /// Returns `true` when this reading looks like a genuine measurement, `false`
    /// when it matches a telemetry sentinel or is otherwise implausible.
    ///
    /// When a Blackhole chip's ARC-NOC path drops (see the qb2-debug reports),
    /// hwmon telemetry collapses to all-ones sentinels — 65536 °C (0xFFFF) and
    /// ~4294 W (u32::MAX microwatts). Feeding those into color mapping would drive
    /// garbage output, so callers use this to hold last-good instead.
    pub fn is_plausible(&self, sentinel_temp_c: f32, sentinel_power_w: f32) -> bool {
        self.max_temp < sentinel_temp_c && self.power_watts < sentinel_power_w
    }
}

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

/// Trait for hardware monitoring implementations
pub trait HardwareMonitor: Send + Sync {
    /// Poll current metrics from all Tenstorrent devices
    fn poll_metrics(&self) -> Result<Vec<DeviceMetrics>>;

    /// Get the monitoring source name (for logging)
    fn source_name(&self) -> &str;
}
