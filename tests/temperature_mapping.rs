// Integration tests for end-to-end temperature to color mapping
// Tests realistic scenarios of temperature changes and color transitions

use tt_qb_lights::{Config, ColorMapper};
use std::path::PathBuf;

#[test]
fn test_idle_to_load_transition() {
    // Simulate a realistic temperature progression: idle → light load → heavy load
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    // Idle temperature (cool)
    let idle_color = mapper.map_temperature(25.0);

    // Light workload
    let light_load = mapper.map_temperature(40.0);

    // Heavy workload
    let heavy_load = mapper.map_temperature(55.0);

    // Thermal throttling territory
    let hot = mapper.map_temperature(70.0);

    // Verify progression: colors should change as temperature increases
    assert_ne!(idle_color, light_load, "Idle and light load should have different colors");
    assert_ne!(light_load, heavy_load, "Light and heavy load should have different colors");
    assert_ne!(heavy_load, hot, "Heavy load and hot should have different colors");

    // Verify visual progression makes sense (getting "hotter" looking)
    // At high temps, red component typically increases
    assert!(
        hot.r >= heavy_load.r || hot.r >= 150,
        "Hot temperature should have significant red: got R={}",
        hot.r
    );
}

#[test]
fn test_temperature_spike_and_recovery() {
    // Simulate a temperature spike (inference burst) and recovery
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    let temps = vec![
        30.0,  // Idle
        35.0,  // Starting to work
        45.0,  // Working
        60.0,  // Spike!
        55.0,  // Cooling down
        40.0,  // Back to normal
        30.0,  // Idle again
    ];

    let colors: Vec<_> = temps.iter().map(|&t| mapper.map_temperature(t)).collect();

    // Verify we get smooth transitions (no sudden jumps)
    for i in 0..colors.len()-1 {
        let c1 = &colors[i];
        let c2 = &colors[i+1];

        let r_diff = (c2.r as i16 - c1.r as i16).abs();
        let g_diff = (c2.g as i16 - c1.g as i16).abs();
        let b_diff = (c2.b as i16 - c1.b as i16).abs();

        let temp_diff = (temps[i+1] - temps[i]).abs();

        // Allow larger jumps for larger temperature differences
        // For 5°C change, allow up to 100 units; for 10°C, allow up to 150 units
        let max_allowed = (temp_diff / 5.0 * 100.0) as i16;

        assert!(
            r_diff <= max_allowed,
            "R channel jump too large between {}°C and {}°C: {} → {} (diff: {}, max allowed: {})",
            temps[i], temps[i+1], c1.r, c2.r, r_diff, max_allowed
        );
        assert!(
            g_diff <= max_allowed,
            "G channel jump too large between {}°C and {}°C: {} → {} (diff: {}, max allowed: {})",
            temps[i], temps[i+1], c1.g, c2.g, g_diff, max_allowed
        );
        assert!(
            b_diff <= max_allowed,
            "B channel jump too large between {}°C and {}°C: {} → {} (diff: {}, max allowed: {})",
            temps[i], temps[i+1], c1.b, c2.b, b_diff, max_allowed
        );
    }

    // Verify we return to similar color after recovery
    let initial = colors[0];
    let recovered = colors[colors.len()-1];

    // Should be the same or very close (within 10 units per channel)
    assert!(
        (initial.r as i16 - recovered.r as i16).abs() <= 10,
        "After recovery, color should be similar to initial"
    );
}

#[test]
fn test_extreme_temperature_clamping() {
    // Test behavior at unrealistic extreme temperatures
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    // Sub-zero temperatures (should clamp to minimum)
    let sub_zero = mapper.map_temperature(-10.0);
    let min_temp = mapper.map_temperature(scheme[0].temp);
    assert_eq!(sub_zero, min_temp, "Sub-zero should clamp to minimum color");

    // Extreme high temperature (should clamp to maximum)
    let extreme_hot = mapper.map_temperature(150.0);
    let max_temp = mapper.map_temperature(scheme[scheme.len()-1].temp);
    assert_eq!(extreme_hot, max_temp, "Extreme hot should clamp to maximum color");

    // Verify both are distinct
    assert_ne!(sub_zero, extreme_hot, "Min and max colors should be different");
}

#[test]
fn test_warning_threshold_colors() {
    // Test that colors near the warning threshold are visually distinct
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    let warning_threshold = config.effects.warning_temp_threshold;

    // Colors just below and above warning threshold
    let below_warning = mapper.map_temperature(warning_threshold - 5.0);
    let at_warning = mapper.map_temperature(warning_threshold);
    let above_warning = mapper.map_temperature(warning_threshold + 5.0);

    // Verify progression exists
    // At high temps, we expect higher red component typically
    let reds = vec![below_warning.r, at_warning.r, above_warning.r];

    // Check that we're not all the same color
    let all_same = reds.iter().all(|&r| r == reds[0]);
    assert!(!all_same, "Colors near warning threshold should vary");

    // Verify we're in the "hot" part of the spectrum (high red or low green/blue)
    assert!(
        at_warning.r >= 150 || (at_warning.g <= 100 && at_warning.b <= 100),
        "Color at warning threshold should look 'hot': {:?}",
        at_warning
    );
}

#[test]
fn test_typical_workload_scenarios() {
    // Test colors for typical Tenstorrent workload scenarios
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    // Define typical scenarios
    let scenarios = vec![
        ("Idle", 28.0),
        ("Light inference", 38.0),
        ("Training start", 48.0),
        ("Heavy training", 58.0),
        ("Peak load", 68.0),
    ];

    let mut colors = Vec::new();
    for (name, temp) in &scenarios {
        let color = mapper.map_temperature(*temp);
        colors.push(color);

        // Verify color is valid (not black)
        let brightness = color.r as u32 + color.g as u32 + color.b as u32;
        assert!(
            brightness > 50,
            "Color for '{}' at {}°C is too dark: {:?}",
            name, temp, color
        );
    }

    // Verify we get progression across all scenarios
    for i in 0..colors.len()-1 {
        assert_ne!(
            colors[i], colors[i+1],
            "Scenarios '{}' and '{}' should have different colors",
            scenarios[i].0, scenarios[i+1].0
        );
    }
}

#[test]
fn test_color_mapping_with_brightness_scaling() {
    // Test that brightness scaling works correctly with color mapping
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    let temp = 50.0;
    let base_color = mapper.map_temperature(temp);

    // Test various brightness levels
    let brightnesses = vec![0.1, 0.3, 0.5, 0.7, 1.0];

    for &brightness in &brightnesses {
        let scaled = base_color.with_brightness(brightness);

        // Verify scaling is approximately correct
        let expected_r = (base_color.r as f32 * brightness) as u8;
        let expected_g = (base_color.g as f32 * brightness) as u8;
        let expected_b = (base_color.b as f32 * brightness) as u8;

        // Allow ±1 for rounding errors
        assert!(
            (scaled.r as i16 - expected_r as i16).abs() <= 1,
            "R channel brightness scaling incorrect at {}: expected {}, got {}",
            brightness, expected_r, scaled.r
        );
        assert!(
            (scaled.g as i16 - expected_g as i16).abs() <= 1,
            "G channel brightness scaling incorrect at {}: expected {}, got {}",
            brightness, expected_g, scaled.g
        );
        assert!(
            (scaled.b as i16 - expected_b as i16).abs() <= 1,
            "B channel brightness scaling incorrect at {}: expected {}, got {}",
            brightness, expected_b, scaled.b
        );
    }
}

#[test]
fn test_multi_device_temperature_mapping() {
    // Simulate multiple Tenstorrent devices with different temperatures
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    // Simulate 4 devices with varying temperatures
    let device_temps = vec![
        ("Device 0", 35.0),
        ("Device 1", 48.0),
        ("Device 2", 42.0),
        ("Device 3", 55.0),
    ];

    let colors: Vec<_> = device_temps
        .iter()
        .map(|(name, temp)| (name, mapper.map_temperature(*temp)))
        .collect();

    // For unified strategy, we'd show the hottest device
    let max_temp = device_temps.iter().map(|(_, t)| t).fold(0.0f32, |a, &b| a.max(b));
    let unified_color = mapper.map_temperature(max_temp);

    // Verify the unified color matches the hottest device
    let hottest_device_color = colors.iter().find(|(name, _)| {
        device_temps.iter().any(|(dn, dt)| dn == *name && *dt == max_temp)
    }).unwrap().1;

    assert_eq!(
        unified_color, hottest_device_color,
        "Unified color should match the hottest device"
    );

    // Verify all colors are valid
    for (name, color) in &colors {
        let brightness = color.r as u32 + color.g as u32 + color.b as u32;
        assert!(
            brightness > 50,
            "{} has too dark color: {:?}",
            name, color
        );
    }
}

#[test]
fn test_temperature_hysteresis_simulation() {
    // Simulate temperature bouncing around a threshold
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    // Temperature hovering around 50°C
    let temps = vec![48.0, 50.0, 49.0, 51.0, 50.0, 52.0, 50.0];

    let colors: Vec<_> = temps.iter().map(|&t| mapper.map_temperature(t)).collect();

    // Verify colors don't vary wildly (should be similar for similar temps)
    for i in 0..colors.len()-1 {
        let c1 = &colors[i];
        let c2 = &colors[i+1];

        // Small temperature changes (±2°C) should yield small color changes
        let max_channel_diff = [
            (c2.r as i16 - c1.r as i16).abs(),
            (c2.g as i16 - c1.g as i16).abs(),
            (c2.b as i16 - c1.b as i16).abs(),
        ].into_iter().max().unwrap();

        assert!(
            max_channel_diff <= 30,
            "Color change too large for small temp change ({}°C → {}°C): max channel diff = {}",
            temps[i], temps[i+1], max_channel_diff
        );
    }
}

#[test]
fn test_coldstart_to_steady_state() {
    // Simulate a cold start warmup to steady state
    let config = Config::from_file(PathBuf::from("config.toml"))
        .expect("Failed to load config.toml");

    let scheme = config.get_active_scheme();
    let mapper = ColorMapper::new(scheme).expect("Failed to create mapper");

    // Gradual warmup from cold start
    let temps: Vec<f32> = (20..=55).step_by(5).map(|t| t as f32).collect();

    let colors: Vec<_> = temps.iter().map(|&t| mapper.map_temperature(t)).collect();

    // Verify gradual progression (each step should change color)
    for i in 0..colors.len()-1 {
        assert_ne!(
            colors[i], colors[i+1],
            "Colors should change during warmup at {}°C → {}°C",
            temps[i], temps[i+1]
        );
    }

    // Verify first color (cold) is visibly different from last (steady state)
    let first = &colors[0];
    let last = &colors[colors.len()-1];

    let color_distance = (
        (first.r as i32 - last.r as i32).pow(2) +
        (first.g as i32 - last.g as i32).pow(2) +
        (first.b as i32 - last.b as i32).pow(2)
    ) as f32;
    let color_distance = color_distance.sqrt();

    assert!(
        color_distance > 50.0,
        "Cold start and steady state colors should be visibly different (distance: {})",
        color_distance
    );
}
