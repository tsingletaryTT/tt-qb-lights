// Passive hardware monitoring via lm-sensors
// Reads /sys/class/hwmon entries directly without invoking tt-smi
// This is a more passive approach that reads kernel-exposed sensor data

use super::{DeviceMetrics, HardwareMonitor};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// lm-sensors based hardware monitor
/// Reads from /sys/class/hwmon/* entries for Tenstorrent devices
pub struct SensorsMonitor {
    hwmon_devices: Vec<HwmonDevice>,
}

/// Represents a single hwmon device in /sys/class/hwmon/
#[derive(Debug, Clone)]
struct HwmonDevice {
    /// Path to hwmon device (e.g., /sys/class/hwmon/hwmon2)
    path: PathBuf,

    /// Device name (e.g., "blackhole-pci-0100")
    name: String,

    /// PCI bus ID extracted from name (e.g., "0100")
    bus_id: String,

    /// Device architecture (blackhole, wormhole_b0, grayskull)
    architecture: String,

    /// Available temperature sensor indices
    temp_indices: Vec<usize>,

    /// Available power sensor indices
    power_indices: Vec<usize>,
}

impl SensorsMonitor {
    pub fn new() -> Result<Self> {
        let hwmon_devices = discover_tenstorrent_devices()?;

        if hwmon_devices.is_empty() {
            anyhow::bail!(
                "No Tenstorrent devices found in /sys/class/hwmon. \
                 Ensure drivers are loaded and devices are visible."
            );
        }

        tracing::info!("Discovered {} Tenstorrent device(s) via hwmon", hwmon_devices.len());
        for device in &hwmon_devices {
            tracing::debug!(
                "Found device: {} (arch: {}, bus: {}, {} temps, {} power sensors)",
                device.name,
                device.architecture,
                device.bus_id,
                device.temp_indices.len(),
                device.power_indices.len()
            );
        }

        Ok(Self { hwmon_devices })
    }
}

impl HardwareMonitor for SensorsMonitor {
    fn poll_metrics(&self) -> Result<Vec<DeviceMetrics>> {
        let mut all_metrics = Vec::new();

        for device in &self.hwmon_devices {
            match read_device_metrics(device) {
                Ok(metrics) => all_metrics.push(metrics),
                Err(e) => {
                    tracing::warn!("Failed to read metrics from {}: {}", device.name, e);
                    // Continue with other devices
                }
            }
        }

        if all_metrics.is_empty() {
            anyhow::bail!("Failed to read metrics from any device");
        }

        Ok(all_metrics)
    }

    fn source_name(&self) -> &str {
        "lm-sensors"
    }
}

/// Discover all Tenstorrent devices in /sys/class/hwmon
fn discover_tenstorrent_devices() -> Result<Vec<HwmonDevice>> {
    let hwmon_base = Path::new("/sys/class/hwmon");

    if !hwmon_base.exists() {
        anyhow::bail!("/sys/class/hwmon does not exist. Is this a Linux system?");
    }

    let mut devices = Vec::new();

    // Iterate through hwmon0, hwmon1, etc.
    for entry in fs::read_dir(hwmon_base).context("Failed to read /sys/class/hwmon")? {
        let entry = entry?;
        let path = entry.path();

        // Read device name
        let name_path = path.join("name");
        if !name_path.exists() {
            continue;
        }

        let name = fs::read_to_string(&name_path)
            .context("Failed to read hwmon device name")?
            .trim()
            .to_string();

        // Check if this is a Tenstorrent device
        // Device names: "blackhole", "blackhole-pci-0100", "wormhole", etc.
        if let Some((architecture, mut bus_id)) = parse_tenstorrent_device_name(&name) {
            // If bus ID wasn't in the name, try to extract it from the sysfs path
            if bus_id == "unknown" {
                bus_id = extract_pci_bus_id_from_path(&path)
                    .unwrap_or_else(|| format!("unknown-{}", devices.len()));
            }

            // Discover available sensors
            let temp_indices = discover_sensor_indices(&path, "temp")?;
            let power_indices = discover_sensor_indices(&path, "power")?;

            devices.push(HwmonDevice {
                path,
                name,
                bus_id,
                architecture,
                temp_indices,
                power_indices,
            });
        }
    }

    Ok(devices)
}

/// Extract PCI bus ID from hwmon sysfs path
/// Example: /sys/class/hwmon/hwmon1 -> ../../devices/pci0000:00/0000:00:01.1/0000:01:00.0/hwmon/hwmon1
/// Extract "0000:01:00.0" from the path
fn extract_pci_bus_id_from_path(hwmon_path: &Path) -> Option<String> {
    // Read the symlink to get the real device path
    let real_path = fs::read_link(hwmon_path).ok()?;

    // Convert to string and look for PCI bus ID pattern (XXXX:XX:XX.X)
    let path_str = real_path.to_string_lossy();

    // Split by '/' and find components that look like PCI addresses
    // Format: 0000:01:00.0 (4 hex digits, colon, 2 hex, colon, 2 hex, dot, 1 digit)
    path_str
        .split('/')
        .filter_map(|component| {
            // Check if component matches PCI address format
            let parts: Vec<&str> = component.split(':').collect();
            if parts.len() == 3 {
                let domain = parts[0];
                let bus = parts[1];
                let dev_func = parts[2];

                // Validate format (simple check)
                if domain.len() == 4
                    && bus.len() == 2
                    && dev_func.contains('.')
                    && dev_func.len() == 4
                {
                    return Some(component.to_string());
                }
            }
            None
        })
        .last() // Take the last (deepest) PCI address
}

/// Parse Tenstorrent device name to extract architecture and bus ID
/// Examples:
///   "blackhole-pci-0100" -> ("blackhole", "0000:01:00.0")
///   "blackhole" -> ("blackhole", "unknown")
///   "wormhole-pci-0200" -> ("wormhole_b0", "0000:02:00.0")
fn parse_tenstorrent_device_name(name: &str) -> Option<(String, String)> {
    let name_lower = name.to_lowercase();

    // Check if this is a Tenstorrent architecture name
    let architecture = if name_lower.starts_with("blackhole") {
        "blackhole"
    } else if name_lower.starts_with("wormhole") {
        "wormhole_b0"
    } else if name_lower.starts_with("grayskull") {
        "grayskull"
    } else {
        return None; // Not a Tenstorrent device
    };

    // Try to extract bus ID if present (format: "blackhole-pci-0100")
    let parts: Vec<&str> = name.split('-').collect();
    let bus_id = if parts.len() == 3 && parts[1] == "pci" {
        let short_bus_id = parts[2];
        // Convert short bus ID (e.g., "0100") to full PCI format (e.g., "0000:01:00.0")
        if short_bus_id.len() == 4 {
            format!(
                "0000:{}:{}.0",
                &short_bus_id[0..2],
                &short_bus_id[2..4]
            )
        } else {
            short_bus_id.to_string()
        }
    } else {
        // No bus ID in name, we'll need to discover it from sysfs
        "unknown".to_string()
    };

    Some((architecture.to_string(), bus_id))
}

/// Discover available sensor indices for a given sensor type
/// For example, temp sensors: temp1_input, temp2_input, etc.
fn discover_sensor_indices(hwmon_path: &Path, sensor_type: &str) -> Result<Vec<usize>> {
    let mut indices = Vec::new();

    // Try indices 1 through 32 (arbitrary reasonable limit)
    for i in 1..=32 {
        let sensor_path = hwmon_path.join(format!("{}{}_input", sensor_type, i));
        if sensor_path.exists() {
            indices.push(i);
        }
    }

    Ok(indices)
}

/// Read metrics from a single hwmon device
fn read_device_metrics(device: &HwmonDevice) -> Result<DeviceMetrics> {
    // Read all temperature sensors
    let mut temps = Vec::new();
    let mut temp_labels = HashMap::new();

    for &idx in &device.temp_indices {
        let temp = read_sensor_value(&device.path, "temp", idx, 1000.0)?;
        temps.push(temp);

        // Try to read label if available
        if let Ok(label) = read_sensor_label(&device.path, "temp", idx) {
            temp_labels.insert(idx, label);
        }
    }

    if temps.is_empty() {
        anyhow::bail!("No temperature sensors found for {}", device.name);
    }

    // First temperature is usually ASIC temp
    let asic_temp = temps[0];

    // Remaining temps are typically GDDR memory
    let gddr_temps = if temps.len() > 1 {
        temps[1..].to_vec()
    } else {
        Vec::new()
    };

    // Calculate max temperature
    let max_temp = temps.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    // Read power sensors
    let mut power_watts = 0.0;
    for &idx in &device.power_indices {
        // Power sensors are in microwatts, convert to watts
        let power = read_sensor_value(&device.path, "power", idx, 1_000_000.0)?;
        power_watts += power;
    }

    // If no power sensors, estimate from typical TDP
    if power_watts == 0.0 {
        power_watts = estimate_power_from_temp(asic_temp);
    }

    // TDP estimation based on architecture
    let tdp_watts = match device.architecture.as_str() {
        "blackhole" => 300.0,  // P150/P300C typical TDP
        "wormhole_b0" => 250.0,
        "grayskull" => 200.0,
        _ => 300.0,
    };

    let power_utilization = (power_watts / tdp_watts).min(1.0);

    // Try to read fan RPM (fan1_input)
    let fan_rpm = read_sensor_value(&device.path, "fan", 1, 1.0)
        .unwrap_or(0.0) as u32;

    // Infer board type from architecture
    let board_type = match device.architecture.as_str() {
        "blackhole" => "p300c".to_string(),
        "wormhole_b0" => "n300".to_string(),
        "grayskull" => "e150".to_string(),
        _ => "unknown".to_string(),
    };

    Ok(DeviceMetrics {
        bus_id: device.bus_id.clone(),
        architecture: device.architecture.clone(),
        board_type,
        asic_temp,
        power_watts,
        tdp_watts,
        fan_rpm,
        gddr_temps,
        max_temp,
        power_utilization,
    })
}

/// Read a sensor value from sysfs
/// Values are scaled (e.g., temp in millidegrees, power in microwatts)
fn read_sensor_value(
    hwmon_path: &Path,
    sensor_type: &str,
    index: usize,
    divisor: f32,
) -> Result<f32> {
    let path = hwmon_path.join(format!("{}{}_input", sensor_type, index));
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let raw_value: f32 = contents.trim().parse()
        .with_context(|| format!("Failed to parse sensor value: {}", contents))?;

    Ok(raw_value / divisor)
}

/// Read a sensor label from sysfs (if available)
fn read_sensor_label(hwmon_path: &Path, sensor_type: &str, index: usize) -> Result<String> {
    let path = hwmon_path.join(format!("{}{}_label", sensor_type, index));
    let contents = fs::read_to_string(&path)?;
    Ok(contents.trim().to_string())
}

/// Estimate power consumption from temperature (rough heuristic)
fn estimate_power_from_temp(temp: f32) -> f32 {
    // Very rough estimation: assume idle at 30°C = 50W, full load at 70°C = 300W
    let temp_range = 70.0 - 30.0;
    let temp_above_idle = (temp - 30.0).max(0.0);
    let power_range = 300.0 - 50.0;

    50.0 + (temp_above_idle / temp_range * power_range).min(power_range)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_name() {
        let (arch, bus_id) = parse_tenstorrent_device_name("blackhole-pci-0100").unwrap();
        assert_eq!(arch, "blackhole");
        assert_eq!(bus_id, "0000:01:00.0");

        let (arch, bus_id) = parse_tenstorrent_device_name("wormhole-pci-0200").unwrap();
        assert_eq!(arch, "wormhole_b0");
        assert_eq!(bus_id, "0000:02:00.0");

        assert!(parse_tenstorrent_device_name("coretemp-isa-0000").is_none());
    }

    #[test]
    fn test_power_estimation() {
        assert!(estimate_power_from_temp(30.0) >= 50.0);
        assert!(estimate_power_from_temp(70.0) <= 300.0);
        assert!(estimate_power_from_temp(50.0) > estimate_power_from_temp(30.0));
    }
}
