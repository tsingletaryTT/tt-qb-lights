// Tenstorrent hardware monitoring via tt-smi JSON output
// Parses `tt-smi -s` JSON data to extract temperature, power, and other metrics

use super::{DeviceMetrics, HardwareMonitor};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// tt-smi JSON output parser
pub struct TtSmiMonitor {
    // Future: could cache device list or add filtering options
}

impl TtSmiMonitor {
    pub fn new() -> Result<Self> {
        // Verify tt-smi is available
        let output = Command::new("tt-smi")
            .arg("--version")
            .output()
            .context("Failed to execute tt-smi. Is it installed and in PATH?")?;

        if !output.status.success() {
            anyhow::bail!("tt-smi command failed. Please ensure Tenstorrent drivers are installed.");
        }

        Ok(Self {})
    }
}

impl HardwareMonitor for TtSmiMonitor {
    fn poll_metrics(&self) -> Result<Vec<DeviceMetrics>> {
        // Execute tt-smi -s to get JSON snapshot
        let output = Command::new("tt-smi")
            .arg("-s")
            .output()
            .context("Failed to execute tt-smi -s")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("tt-smi -s failed: {}", stderr);
        }

        let stdout = String::from_utf8(output.stdout)
            .context("tt-smi output is not valid UTF-8")?;

        // Parse JSON output
        let smi_data: TtSmiOutput = serde_json::from_str(&stdout)
            .context("Failed to parse tt-smi JSON output")?;

        // Convert to our DeviceMetrics format
        let mut metrics = Vec::new();
        for device in smi_data.device_info {
            if let Some(device_metrics) = parse_device_info(&device)? {
                metrics.push(device_metrics);
            }
        }

        if metrics.is_empty() {
            anyhow::bail!("No Tenstorrent devices found in tt-smi output");
        }

        Ok(metrics)
    }

    fn source_name(&self) -> &str {
        "tt-smi"
    }
}

/// Parse a single device's information from tt-smi JSON
fn parse_device_info(device: &DeviceInfo) -> Result<Option<DeviceMetrics>> {
    // Extract bus ID from board_info
    let bus_id = device.board_info.bus_id.clone();
    let board_type = device.board_info.board_type.clone();

    // Parse telemetry data
    let telemetry = &device.telemetry;

    // Parse ASIC temperature (hex string to float)
    let asic_temp = parse_hex_temp(&telemetry.asic_temperature)
        .context("Failed to parse ASIC temperature")?;

    // Parse power (hex string to float watts)
    let power_watts = parse_hex_value(&telemetry.input_power)
        .context("Failed to parse input power")?;

    // Parse TDP
    let tdp_watts = parse_hex_value(&telemetry.tdp)
        .unwrap_or(300.0); // Default to 300W if not available

    // Parse fan RPM
    let fan_rpm = parse_hex_value(&telemetry.fan_rpm)
        .unwrap_or(0.0) as u32;

    // Parse GDDR temperatures
    let mut gddr_temps = Vec::new();
    for (key, value) in &telemetry.additional_fields {
        if key.starts_with("GDDR") && key.ends_with("_TEMP") {
            if let Ok(temp) = parse_hex_temp(value) {
                gddr_temps.push(temp);
            }
        }
    }

    // Calculate max temperature (ASIC + all GDDR temps)
    let mut all_temps = vec![asic_temp];
    all_temps.extend(&gddr_temps);
    let max_temp = all_temps.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    // Calculate power utilization
    let power_utilization = if tdp_watts > 0.0 {
        (power_watts / tdp_watts).min(1.0)
    } else {
        0.0
    };

    // Determine architecture from board type or other indicators
    // P150/P300C are Blackhole, but we could also check other fields
    let architecture = determine_architecture(&board_type);

    Ok(Some(DeviceMetrics {
        bus_id,
        architecture,
        board_type,
        asic_temp,
        power_watts,
        tdp_watts,
        fan_rpm,
        gddr_temps,
        max_temp,
        power_utilization,
    }))
}

/// Determine device architecture from board type
fn determine_architecture(board_type: &str) -> String {
    match board_type.to_lowercase().as_str() {
        "p150" | "p300c" | "p300" => "blackhole".to_string(),
        "n150" | "n300" => "wormhole_b0".to_string(),
        "e75" | "e150" => "grayskull".to_string(),
        _ => {
            // Unknown board type, default to blackhole for now
            tracing::warn!("Unknown board type '{}', assuming blackhole architecture", board_type);
            "blackhole".to_string()
        }
    }
}

/// Parse hex temperature value to Celsius
/// tt-smi returns temperatures as hex strings (e.g., "0x2D" = 45°C)
fn parse_hex_temp(hex_str: &str) -> Result<f32> {
    let hex_str = hex_str.trim().trim_start_matches("0x");
    let value = u32::from_str_radix(hex_str, 16)
        .context("Failed to parse hex temperature value")?;

    // Temperature is in Celsius, returned directly
    Ok(value as f32)
}

/// Parse generic hex value to float
fn parse_hex_value(hex_str: &str) -> Result<f32> {
    let hex_str = hex_str.trim().trim_start_matches("0x");
    let value = u32::from_str_radix(hex_str, 16)
        .context("Failed to parse hex value")?;
    Ok(value as f32)
}

// JSON structures matching tt-smi output format

#[derive(Debug, Deserialize, Serialize)]
struct TtSmiOutput {
    device_info: Vec<DeviceInfo>,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceInfo {
    board_info: BoardInfo,
    telemetry: Telemetry,
}

#[derive(Debug, Deserialize, Serialize)]
struct BoardInfo {
    bus_id: String,
    board_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Telemetry {
    #[serde(rename = "ASIC_TEMPERATURE")]
    asic_temperature: String,

    #[serde(rename = "INPUT_POWER")]
    input_power: String,

    #[serde(rename = "TDP")]
    tdp: String,

    #[serde(rename = "FAN_RPM")]
    fan_rpm: String,

    // Capture all other fields (like GDDR temps)
    #[serde(flatten)]
    additional_fields: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_temp() {
        assert_eq!(parse_hex_temp("0x2D").unwrap(), 45.0);
        assert_eq!(parse_hex_temp("0x3C").unwrap(), 60.0);
        assert_eq!(parse_hex_temp("2D").unwrap(), 45.0);
    }

    #[test]
    fn test_determine_architecture() {
        assert_eq!(determine_architecture("p300c"), "blackhole");
        assert_eq!(determine_architecture("P150"), "blackhole");
        assert_eq!(determine_architecture("n300"), "wormhole_b0");
        assert_eq!(determine_architecture("e150"), "grayskull");
    }

    #[test]
    fn test_parse_sample_json() {
        let sample = r#"
        {
            "device_info": [
                {
                    "board_info": {
                        "bus_id": "0000:01:00.0",
                        "board_type": "p300c"
                    },
                    "telemetry": {
                        "ASIC_TEMPERATURE": "0x2D",
                        "INPUT_POWER": "0x96",
                        "TDP": "0x12C",
                        "FAN_RPM": "0x0",
                        "GDDR01_TEMP": "0x28",
                        "GDDR23_TEMP": "0x29"
                    }
                }
            ]
        }
        "#;

        let data: TtSmiOutput = serde_json::from_str(sample).unwrap();
        assert_eq!(data.device_info.len(), 1);

        let metrics = parse_device_info(&data.device_info[0]).unwrap().unwrap();
        assert_eq!(metrics.bus_id, "0000:01:00.0");
        assert_eq!(metrics.asic_temp, 45.0);
        assert_eq!(metrics.power_watts, 150.0);
        assert_eq!(metrics.tdp_watts, 300.0);
        assert_eq!(metrics.gddr_temps.len(), 2);
    }
}
