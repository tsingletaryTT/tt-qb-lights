// Configuration file handling for tt-qb-lights
// Loads and validates the config.toml file

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Find and load configuration file from standard locations
    ///
    /// Search order:
    /// 1. Explicit path if provided (not empty)
    /// 2. ~/.config/tt-qb-lights/config.toml (XDG standard)
    /// 3. ~/.tt-qb-lights.toml (simple dotfile)
    /// 4. ./config.toml (current directory, for development)
    pub fn load(explicit_path: Option<&Path>) -> Result<Self> {
        let config_path = if let Some(path) = explicit_path {
            if path.as_os_str().is_empty() {
                Self::find_config_file()?
            } else {
                path.to_path_buf()
            }
        } else {
            Self::find_config_file()?
        };

        Self::from_file(&config_path)
    }

    /// Find configuration file in standard locations
    fn find_config_file() -> Result<PathBuf> {
        let candidates = vec![
            // XDG config directory (preferred)
            dirs::config_dir().map(|d| d.join("tt-qb-lights").join("config.toml")),
            // Simple dotfile in home
            dirs::home_dir().map(|d| d.join(".tt-qb-lights.toml")),
            // Current directory (development fallback)
            Some(PathBuf::from("config.toml")),
        ];

        for candidate in candidates.into_iter().flatten() {
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        anyhow::bail!(
            "Configuration file not found. Searched:\n\
             - ~/.config/tt-qb-lights/config.toml\n\
             - ~/.tt-qb-lights.toml\n\
             - ./config.toml\n\
             \n\
             Run 'tt-qb-lights --init' to create a default configuration."
        )
    }

    /// Get the default config file path (XDG standard location)
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
        Ok(config_dir.join("tt-qb-lights").join("config.toml"))
    }

    /// Initialize default configuration file
    pub fn init_default_config() -> Result<PathBuf> {
        let config_path = Self::default_path()?;
        let config_dir = config_path.parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid config path"))?;

        // Create config directory if it doesn't exist
        fs::create_dir_all(config_dir)
            .with_context(|| format!("Failed to create config directory: {}", config_dir.display()))?;

        // Check if config already exists
        if config_path.exists() {
            anyhow::bail!(
                "Configuration file already exists at: {}\n\
                 To reconfigure, either delete this file or edit it manually.",
                config_path.display()
            );
        }

        // Copy default config from project directory or use embedded default
        let default_config = include_str!("../config.toml");
        fs::write(&config_path, default_config)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(config_path)
    }

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

    fn create_valid_config() -> Config {
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

        Config {
            monitoring: MonitoringConfig {
                poll_interval_ms: 1000,
                source: MonitoringSource::LmSensors,
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
        }
    }

    #[test]
    fn test_config_validation() {
        let config = create_valid_config();
        config.validate().unwrap();
    }

    #[test]
    fn test_config_invalid_min_brightness() {
        let mut config = create_valid_config();

        // Test below 0.0
        config.effects.min_brightness = -0.1;
        assert!(config.validate().is_err());

        // Test above 1.0
        config.effects.min_brightness = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_invalid_max_brightness() {
        let mut config = create_valid_config();

        // Test below 0.0
        config.effects.max_brightness = -0.1;
        assert!(config.validate().is_err());

        // Test above 1.0
        config.effects.max_brightness = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_min_greater_than_max() {
        let mut config = create_valid_config();
        config.effects.min_brightness = 0.8;
        config.effects.max_brightness = 0.5;

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be greater than"));
    }

    #[test]
    fn test_config_nonexistent_scheme() {
        let mut config = create_valid_config();
        config.color_mapping.scheme = "nonexistent".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_config_empty_scheme() {
        let mut config = create_valid_config();
        config.color_mapping.schemes.insert("empty".to_string(), vec![]);
        config.color_mapping.scheme = "empty".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no thresholds"));
    }

    #[test]
    fn test_config_unsorted_temperatures() {
        let mut config = create_valid_config();
        config.color_mapping.schemes.insert(
            "unsorted".to_string(),
            vec![
                ColorThreshold { temp: 70.0, color: "#FF0000".to_string() },
                ColorThreshold { temp: 20.0, color: "#00FF00".to_string() },
            ],
        );
        config.color_mapping.scheme = "unsorted".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ascending order"));
    }

    #[test]
    fn test_config_duplicate_temperatures() {
        let mut config = create_valid_config();
        config.color_mapping.schemes.insert(
            "duplicate".to_string(),
            vec![
                ColorThreshold { temp: 50.0, color: "#FF0000".to_string() },
                ColorThreshold { temp: 50.0, color: "#00FF00".to_string() },
            ],
        );
        config.color_mapping.scheme = "duplicate".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ascending order"));
    }

    #[test]
    fn test_config_invalid_hex_format_no_hash() {
        let mut config = create_valid_config();
        config.color_mapping.schemes.insert(
            "nohash".to_string(),
            vec![
                ColorThreshold { temp: 20.0, color: "FF0000".to_string() },
            ],
        );
        config.color_mapping.scheme = "nohash".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid color format"));
    }

    #[test]
    fn test_config_invalid_hex_format_wrong_length() {
        let mut config = create_valid_config();
        config.color_mapping.schemes.insert(
            "short".to_string(),
            vec![
                ColorThreshold { temp: 20.0, color: "#FF00".to_string() },
            ],
        );
        config.color_mapping.scheme = "short".to_string();

        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid color format"));
    }

    #[test]
    fn test_config_get_active_scheme() {
        let config = create_valid_config();
        let scheme = config.get_active_scheme();

        assert_eq!(scheme.len(), 2);
        assert_eq!(scheme[0].temp, 20.0);
        assert_eq!(scheme[1].temp, 70.0);
    }

    #[test]
    fn test_config_brightness_boundary_values() {
        let mut config = create_valid_config();

        // Test exactly 0.0 and 1.0 are valid
        config.effects.min_brightness = 0.0;
        config.effects.max_brightness = 1.0;
        assert!(config.validate().is_ok());

        // Test min = max is valid
        config.effects.min_brightness = 0.5;
        config.effects.max_brightness = 0.5;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_zone_strategies() {
        let mut config = create_valid_config();

        config.openrgb.zone_strategy = ZoneStrategy::Unified;
        assert!(config.validate().is_ok());

        config.openrgb.zone_strategy = ZoneStrategy::PerDevice;
        assert!(config.validate().is_ok());

        config.openrgb.zone_strategy = ZoneStrategy::Gradient;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_monitoring_sources() {
        let mut config = create_valid_config();

        config.monitoring.source = MonitoringSource::LmSensors;
        assert!(config.validate().is_ok());

        config.monitoring.source = MonitoringSource::TtSmi;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_quietbox_sunset_scheme_validation() {
        let mut config = create_valid_config();

        // Add the quietbox_sunset scheme
        config.color_mapping.schemes.insert(
            "quietbox_sunset".to_string(),
            vec![
                ColorThreshold { temp: 20.0, color: "#4DB8A5".to_string() },
                ColorThreshold { temp: 35.0, color: "#6FD8D5".to_string() },
                ColorThreshold { temp: 50.0, color: "#E88B8B".to_string() },
                ColorThreshold { temp: 60.0, color: "#F5A4A4".to_string() },
                ColorThreshold { temp: 70.0, color: "#C23B3B".to_string() },
            ],
        );
        config.color_mapping.scheme = "quietbox_sunset".to_string();

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tt_dark_scheme_validation() {
        let mut config = create_valid_config();

        // Add the tt_dark scheme
        config.color_mapping.schemes.insert(
            "tt_dark".to_string(),
            vec![
                ColorThreshold { temp: 20.0, color: "#4FD1C5".to_string() },
                ColorThreshold { temp: 35.0, color: "#81E6D9".to_string() },
                ColorThreshold { temp: 50.0, color: "#EC96B8".to_string() },
                ColorThreshold { temp: 60.0, color: "#F4C471".to_string() },
                ColorThreshold { temp: 70.0, color: "#FF6B6B".to_string() },
            ],
        );
        config.color_mapping.scheme = "tt_dark".to_string();

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_tt_light_scheme_validation() {
        let mut config = create_valid_config();

        // Add the tt_light scheme
        config.color_mapping.schemes.insert(
            "tt_light".to_string(),
            vec![
                ColorThreshold { temp: 20.0, color: "#3fb7de".to_string() },
                ColorThreshold { temp: 35.0, color: "#3293b2".to_string() },
                ColorThreshold { temp: 50.0, color: "#5347a4".to_string() },
                ColorThreshold { temp: 60.0, color: "#82672b".to_string() },
                ColorThreshold { temp: 70.0, color: "#d03a1b".to_string() },
            ],
        );
        config.color_mapping.scheme = "tt_light".to_string();

        assert!(config.validate().is_ok());
    }
}
