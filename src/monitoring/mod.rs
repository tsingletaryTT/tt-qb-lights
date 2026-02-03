// Hardware monitoring module
// Collects temperature, power, and utilization metrics from Tenstorrent devices

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
}

/// Trait for hardware monitoring implementations
pub trait HardwareMonitor: Send + Sync {
    /// Poll current metrics from all Tenstorrent devices
    fn poll_metrics(&self) -> Result<Vec<DeviceMetrics>>;

    /// Get the monitoring source name (for logging)
    fn source_name(&self) -> &str;
}
