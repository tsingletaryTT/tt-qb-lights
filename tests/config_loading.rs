// Integration tests for configuration loading
// Tests loading and validating the actual config.toml file

use tt_qb_lights::{Config, config::MonitoringSource, config::ZoneStrategy};
use std::path::PathBuf;

/// Helper to get project config.toml path for testing
fn get_test_config_path() -> PathBuf {
    // In tests, we use the project's config.toml as the reference
    PathBuf::from("config.toml")
}

#[test]
fn test_load_default_config() {
    // Load the actual config.toml from the project root
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Verify it loaded successfully
    assert!(config.monitoring.poll_interval_ms > 0);
    assert_eq!(config.monitoring.source, MonitoringSource::LmSensors);
}

#[test]
fn test_default_config_has_quietbox_sunset_scheme() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Verify the default scheme is quietbox_sunset
    assert_eq!(config.color_mapping.scheme, "quietbox_sunset");
}

#[test]
fn test_default_config_has_all_schemes() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Verify all expected schemes are present
    let schemes = &config.color_mapping.schemes;

    // Original schemes
    assert!(schemes.contains_key("teal_purple_red"));

    // New schemes
    assert!(schemes.contains_key("quietbox_sunset"));

    // Note: tt_dark, tt_light, tenstorrent_branding are commented out by default
    // but quietbox_sunset is active
}

#[test]
fn test_default_config_brightness_settings() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Verify brightness settings are reasonable
    assert!(config.effects.min_brightness >= 0.0);
    assert!(config.effects.min_brightness <= 1.0);
    assert!(config.effects.max_brightness >= 0.0);
    assert!(config.effects.max_brightness <= 1.0);
    assert!(config.effects.min_brightness <= config.effects.max_brightness);
}

#[test]
fn test_default_config_warning_threshold() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Verify warning threshold is reasonable (typically 60-75°C)
    assert!(config.effects.warning_temp_threshold >= 50.0);
    assert!(config.effects.warning_temp_threshold <= 90.0);
}

#[test]
fn test_default_config_openrgb_settings() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Verify OpenRGB settings
    assert_eq!(config.openrgb.server_host, "127.0.0.1");
    assert_eq!(config.openrgb.server_port, 6742);
    assert!(!config.openrgb.device_name.is_empty());
    assert_eq!(config.openrgb.zone_strategy, ZoneStrategy::Unified);
}

#[test]
fn test_quietbox_sunset_scheme_structure() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();

    // Verify the scheme has the expected number of thresholds
    assert_eq!(scheme.len(), 5, "QuietBox Sunset should have 5 color stops");

    // Verify temperatures are in ascending order
    for i in 1..scheme.len() {
        assert!(
            scheme[i].temp > scheme[i-1].temp,
            "Temperatures must be in ascending order"
        );
    }

    // Verify temperature range (should span 20-70°C)
    assert_eq!(scheme[0].temp, 20.0);
    assert_eq!(scheme[scheme.len()-1].temp, 70.0);
}

#[test]
fn test_config_validation_passes() {
    // This tests that the actual config.toml passes all validation rules
    let result = Config::from_file(get_test_config_path());

    assert!(
        result.is_ok(),
        "config.toml should pass all validation checks: {:?}",
        result.err()
    );
}

#[test]
fn test_config_effects_enabled() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Verify effects are configured
    assert!(config.effects.enable_power_brightness, "Power brightness should be enabled by default");
    assert!(config.effects.enable_warning_pulse, "Warning pulse should be enabled by default");
    assert!(config.effects.pulse_speed_ms > 0, "Pulse speed should be positive");
}

#[test]
fn test_all_scheme_colors_valid_hex() {
    let config = Config::from_file(get_test_config_path())
        .expect("Failed to load config.toml");

    // Test all schemes in the config have valid hex colors
    for (scheme_name, thresholds) in &config.color_mapping.schemes {
        for threshold in thresholds {
            // Verify hex format
            assert!(
                threshold.color.starts_with('#'),
                "Scheme '{}' has color without # prefix: {}",
                scheme_name, threshold.color
            );
            assert_eq!(
                threshold.color.len(), 7,
                "Scheme '{}' has invalid color length: {}",
                scheme_name, threshold.color
            );

            // Verify it can be parsed
            let result = tt_qb_lights::RgbColor::from_hex(&threshold.color);
            assert!(
                result.is_ok(),
                "Scheme '{}' has unparseable color '{}': {:?}",
                scheme_name, threshold.color, result.err()
            );
        }
    }
}
