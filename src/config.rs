// Configuration file handling for tt-qb-lights
// Loads and validates the config.toml file

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Main configuration structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub monitoring: MonitoringConfig,
    pub openrgb: OpenRgbConfig,
    pub color_mapping: ColorMappingConfig,
    pub effects: EffectsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// Monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringConfig {
    /// Polling interval in milliseconds
    pub poll_interval_ms: u64,

    /// Data source: "tt-smi" or "lm-sensors"
    pub source: MonitoringSource,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum MonitoringSource {
    TtSmi,
    LmSensors,
}

/// OpenRGB server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenRgbConfig {
    pub server_host: String,
    pub server_port: u16,
    pub device_name: String,
    pub zone_strategy: ZoneStrategy,
}

/// Strategy for mapping temperatures to RGB zones
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ZoneStrategy {
    /// All zones show the hottest device
    Unified,
    /// Each zone represents one device
    PerDevice,
    /// Smooth gradient across all zones
    Gradient,
}

/// Color mapping configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorMappingConfig {
    /// Active color scheme name
    pub scheme: String,

    /// Map of scheme name to color thresholds
    #[serde(default)]
    pub schemes: HashMap<String, Vec<ColorThreshold>>,
}

/// A temperature threshold with associated color
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorThreshold {
    /// Temperature in Celsius
    pub temp: f32,

    /// RGB color in hex format (e.g., "#FF0000")
    pub color: String,
}

/// Visual effects configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EffectsConfig {
    /// Scale brightness based on power consumption
    pub enable_power_brightness: bool,

    /// Minimum brightness (0.0 to 1.0)
    #[serde(default = "default_min_brightness")]
    pub min_brightness: f32,

    /// Maximum brightness (0.0 to 1.0)
    #[serde(default = "default_max_brightness")]
    pub max_brightness: f32,

    /// Enable pulsing effect for high temperatures
    pub enable_warning_pulse: bool,

    /// Temperature threshold for warning pulse
    #[serde(default = "default_warning_temp")]
    pub warning_temp_threshold: f32,

    /// Pulse speed in milliseconds
    pub pulse_speed_ms: u64,
}

fn default_min_brightness() -> f32 { 0.3 }
fn default_max_brightness() -> f32 { 1.0 }
fn default_warning_temp() -> f32 { 70.0 }

/// Logging configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,

    #[serde(default)]
    pub log_file: Option<String>,
}

fn default_log_level() -> String { "info".to_string() }

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            log_file: None,
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        // Validate brightness ranges
        if self.effects.min_brightness < 0.0 || self.effects.min_brightness > 1.0 {
            anyhow::bail!("min_brightness must be between 0.0 and 1.0");
        }
        if self.effects.max_brightness < 0.0 || self.effects.max_brightness > 1.0 {
            anyhow::bail!("max_brightness must be between 0.0 and 1.0");
        }
        if self.effects.min_brightness > self.effects.max_brightness {
            anyhow::bail!("min_brightness cannot be greater than max_brightness");
        }

        // Validate that the selected scheme exists
        if !self.color_mapping.schemes.contains_key(&self.color_mapping.scheme) {
            anyhow::bail!(
                "Color scheme '{}' not found in configuration. Available schemes: {:?}",
                self.color_mapping.scheme,
                self.color_mapping.schemes.keys().collect::<Vec<_>>()
            );
        }

        // Validate color thresholds
        let scheme = &self.color_mapping.schemes[&self.color_mapping.scheme];
        if scheme.is_empty() {
            anyhow::bail!("Color scheme '{}' has no thresholds defined", self.color_mapping.scheme);
        }

        // Check that temperatures are in ascending order
        for i in 1..scheme.len() {
            if scheme[i].temp <= scheme[i-1].temp {
                anyhow::bail!(
                    "Color scheme temperatures must be in ascending order. Found {} <= {}",
                    scheme[i].temp, scheme[i-1].temp
                );
            }
        }

        // Validate hex color format
        for threshold in scheme {
            if !threshold.color.starts_with('#') || threshold.color.len() != 7 {
                anyhow::bail!(
                    "Invalid color format '{}'. Expected hex format like #FF0000",
                    threshold.color
                );
            }
        }

        Ok(())
    }

    /// Get the active color scheme's thresholds
    pub fn get_active_scheme(&self) -> &[ColorThreshold] {
        &self.color_mapping.schemes[&self.color_mapping.scheme]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        // Build config programmatically instead of parsing TOML
        // (TOML syntax for nested arrays is tricky)
        let mut schemes = HashMap::new();
        schemes.insert(
            "test".to_string(),
            vec![
                ColorThreshold {
                    temp: 20.0,
                    color: "#00FF00".to_string(),
                },
                ColorThreshold {
                    temp: 70.0,
                    color: "#FF0000".to_string(),
                },
            ],
        );

        let config = Config {
            monitoring: MonitoringConfig {
                poll_interval_ms: 1000,
                source: MonitoringSource::TtSmi,
            },
            openrgb: OpenRgbConfig {
                server_host: "127.0.0.1".to_string(),
                server_port: 6742,
                device_name: "Test Device".to_string(),
                zone_strategy: ZoneStrategy::Unified,
            },
            color_mapping: ColorMappingConfig {
                scheme: "test".to_string(),
                schemes,
            },
            effects: EffectsConfig {
                enable_power_brightness: true,
                min_brightness: 0.3,
                max_brightness: 1.0,
                enable_warning_pulse: true,
                warning_temp_threshold: 70.0,
                pulse_speed_ms: 500,
            },
            logging: LoggingConfig::default(),
        };

        config.validate().unwrap();
    }
}
