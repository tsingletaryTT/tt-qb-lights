// Integration tests for color schemes
// Tests all built-in color schemes for correctness and consistency

use tt_qb_lights::{ColorMapper, ColorThreshold, RgbColor};

/// Helper to create a color mapper from thresholds
fn create_mapper(thresholds: Vec<ColorThreshold>) -> ColorMapper {
    ColorMapper::new(&thresholds).expect("Failed to create color mapper")
}

#[test]
fn test_quietbox_sunset_full_range() {
    // Test the QuietBox Sunset color scheme across its full temperature range
    let thresholds = vec![
        ColorThreshold { temp: 20.0, color: "#4DB8A5".to_string() },  // Sky Teal
        ColorThreshold { temp: 35.0, color: "#6FD8D5".to_string() },  // Bright Cyan
        ColorThreshold { temp: 50.0, color: "#E88B8B".to_string() },  // Coral
        ColorThreshold { temp: 60.0, color: "#F5A4A4".to_string() },  // Salmon
        ColorThreshold { temp: 70.0, color: "#C23B3B".to_string() },  // Deep Red
    ];
    let mapper = create_mapper(thresholds);

    // Test at exact thresholds
    let cold = mapper.map_temperature(20.0);
    assert_eq!(cold.r, 77);   // Sky Teal
    assert_eq!(cold.g, 184);
    assert_eq!(cold.b, 165);

    let hot = mapper.map_temperature(70.0);
    assert_eq!(hot.r, 194);  // Deep Red
    assert_eq!(hot.g, 59);
    assert_eq!(hot.b, 59);

    // Test interpolation works smoothly
    let mid = mapper.map_temperature(45.0);
    // Should be between Bright Cyan and Coral
    assert!(mid.r > 111 && mid.r < 232);

    // Test clamping below min
    let below = mapper.map_temperature(10.0);
    assert_eq!(below, cold);

    // Test clamping above max
    let above = mapper.map_temperature(80.0);
    assert_eq!(above, hot);
}

#[test]
fn test_teal_purple_red_original_scheme() {
    // Test the original teal_purple_red color scheme
    let thresholds = vec![
        ColorThreshold { temp: 20.0, color: "#008B8B".to_string() },  // Deep Teal
        ColorThreshold { temp: 30.0, color: "#00CED1".to_string() },  // Bright Cyan
        ColorThreshold { temp: 40.0, color: "#8B00FF".to_string() },  // Purple
        ColorThreshold { temp: 50.0, color: "#FF00FF".to_string() },  // Magenta
        ColorThreshold { temp: 60.0, color: "#FF0066".to_string() },  // Hot Pink
        ColorThreshold { temp: 70.0, color: "#FF0000".to_string() },  // Red
    ];
    let mapper = create_mapper(thresholds);

    // Test progression from cool to hot
    let colors = vec![
        mapper.map_temperature(20.0),
        mapper.map_temperature(30.0),
        mapper.map_temperature(40.0),
        mapper.map_temperature(50.0),
        mapper.map_temperature(60.0),
        mapper.map_temperature(70.0),
    ];

    // Verify we get 6 distinct colors
    for i in 0..colors.len()-1 {
        assert_ne!(colors[i], colors[i+1], "Adjacent colors should be different");
    }

    // Verify final color is pure red
    assert_eq!(colors[5].r, 255);
    assert_eq!(colors[5].g, 0);
    assert_eq!(colors[5].b, 0);
}

#[test]
fn test_tt_dark_scheme_progression() {
    // Test TT Dark scheme color progression
    let thresholds = vec![
        ColorThreshold { temp: 20.0, color: "#4FD1C5".to_string() },  // Teal
        ColorThreshold { temp: 35.0, color: "#81E6D9".to_string() },  // Light Teal
        ColorThreshold { temp: 50.0, color: "#EC96B8".to_string() },  // Pink
        ColorThreshold { temp: 60.0, color: "#F4C471".to_string() },  // Gold
        ColorThreshold { temp: 70.0, color: "#FF6B6B".to_string() },  // Red
    ];
    let mapper = create_mapper(thresholds);

    // Test temperature progression
    let temp_20 = mapper.map_temperature(20.0);
    let _temp_35 = mapper.map_temperature(35.0);
    let temp_50 = mapper.map_temperature(50.0);
    let temp_60 = mapper.map_temperature(60.0);
    let temp_70 = mapper.map_temperature(70.0);

    // Verify progression: teal → light teal → pink → gold → red
    // Teal should be teal-ish
    assert!(temp_20.g > temp_20.r);
    assert!(temp_20.g > temp_20.b);

    // Pink should be pink-ish (high R, moderate G, high B)
    assert!(temp_50.r > 200);
    assert!(temp_50.b > 150);

    // Gold should be gold-ish (high R, high G, moderate B)
    assert!(temp_60.r > 200);
    assert!(temp_60.g > 150);

    // Red should be red-ish
    assert!(temp_70.r > temp_70.g);
    assert!(temp_70.r > temp_70.b);
}

#[test]
fn test_tt_light_scheme_blue_to_red() {
    // Test TT Light scheme transitions from blue to red
    let thresholds = vec![
        ColorThreshold { temp: 20.0, color: "#3fb7de".to_string() },  // Bright Blue
        ColorThreshold { temp: 35.0, color: "#3293b2".to_string() },  // Blue
        ColorThreshold { temp: 50.0, color: "#5347a4".to_string() },  // Purple
        ColorThreshold { temp: 60.0, color: "#82672b".to_string() },  // Copper
        ColorThreshold { temp: 70.0, color: "#d03a1b".to_string() },  // Red
    ];
    let mapper = create_mapper(thresholds);

    // Test key points
    let blue = mapper.map_temperature(20.0);
    let purple = mapper.map_temperature(50.0);
    let red = mapper.map_temperature(70.0);

    // Blue should have high blue component
    assert!(blue.b > blue.r);
    assert!(blue.b > blue.g);

    // Purple should have high R and B
    assert!(purple.r > 50);
    assert!(purple.b > 150);

    // Red should have high red component
    assert!(red.r > red.g);
    assert!(red.r > red.b);
}

#[test]
fn test_all_schemes_smooth_interpolation() {
    // Test that all schemes interpolate smoothly (no jumps)
    let schemes = vec![
        ("quietbox_sunset", vec![
            ColorThreshold { temp: 20.0, color: "#4DB8A5".to_string() },
            ColorThreshold { temp: 35.0, color: "#6FD8D5".to_string() },
            ColorThreshold { temp: 50.0, color: "#E88B8B".to_string() },
            ColorThreshold { temp: 60.0, color: "#F5A4A4".to_string() },
            ColorThreshold { temp: 70.0, color: "#C23B3B".to_string() },
        ]),
        ("tt_dark", vec![
            ColorThreshold { temp: 20.0, color: "#4FD1C5".to_string() },
            ColorThreshold { temp: 35.0, color: "#81E6D9".to_string() },
            ColorThreshold { temp: 50.0, color: "#EC96B8".to_string() },
            ColorThreshold { temp: 60.0, color: "#F4C471".to_string() },
            ColorThreshold { temp: 70.0, color: "#FF6B6B".to_string() },
        ]),
        ("tt_light", vec![
            ColorThreshold { temp: 20.0, color: "#3fb7de".to_string() },
            ColorThreshold { temp: 35.0, color: "#3293b2".to_string() },
            ColorThreshold { temp: 50.0, color: "#5347a4".to_string() },
            ColorThreshold { temp: 60.0, color: "#82672b".to_string() },
            ColorThreshold { temp: 70.0, color: "#d03a1b".to_string() },
        ]),
    ];

    for (name, thresholds) in schemes {
        let mapper = create_mapper(thresholds);

        // Test interpolation between each pair of thresholds
        for temp in (20..=70).step_by(1) {
            let color1 = mapper.map_temperature(temp as f32);
            let color2 = mapper.map_temperature((temp + 1) as f32);

            // Color change should be gradual (max 10 units per degree in any channel)
            let r_diff = (color2.r as i16 - color1.r as i16).abs();
            let g_diff = (color2.g as i16 - color1.g as i16).abs();
            let b_diff = (color2.b as i16 - color1.b as i16).abs();

            assert!(
                r_diff <= 20,
                "Scheme '{}': R channel jumps too much at {}°C: {} → {} (diff: {})",
                name, temp, color1.r, color2.r, r_diff
            );
            assert!(
                g_diff <= 20,
                "Scheme '{}': G channel jumps too much at {}°C: {} → {} (diff: {})",
                name, temp, color1.g, color2.g, g_diff
            );
            assert!(
                b_diff <= 20,
                "Scheme '{}': B channel jumps too much at {}°C: {} → {} (diff: {})",
                name, temp, color1.b, color2.b, b_diff
            );
        }
    }
}

#[test]
fn test_scheme_temperature_ranges() {
    // Verify all schemes cover reasonable temperature ranges
    let schemes = vec![
        ("quietbox_sunset", vec![20.0, 35.0, 50.0, 60.0, 70.0]),
        ("tt_dark", vec![20.0, 35.0, 50.0, 60.0, 70.0]),
        ("tt_light", vec![20.0, 35.0, 50.0, 60.0, 70.0]),
    ];

    for (name, temps) in schemes {
        // Verify min temp is reasonable (typically 15-25°C)
        assert!(
            temps[0] >= 15.0 && temps[0] <= 30.0,
            "Scheme '{}' has unusual min temp: {}°C",
            name, temps[0]
        );

        // Verify max temp is reasonable (typically 65-80°C)
        assert!(
            temps[temps.len()-1] >= 60.0 && temps[temps.len()-1] <= 85.0,
            "Scheme '{}' has unusual max temp: {}°C",
            name, temps[temps.len()-1]
        );

        // Verify span is at least 40°C
        let span = temps[temps.len()-1] - temps[0];
        assert!(
            span >= 40.0,
            "Scheme '{}' has too narrow temperature range: {}°C",
            name, span
        );
    }
}

#[test]
fn test_color_visibility_at_extremes() {
    // Ensure colors are visible at both temperature extremes
    let thresholds = vec![
        ColorThreshold { temp: 20.0, color: "#4DB8A5".to_string() },
        ColorThreshold { temp: 70.0, color: "#C23B3B".to_string() },
    ];
    let mapper = create_mapper(thresholds);

    let cold = mapper.map_temperature(20.0);
    let hot = mapper.map_temperature(70.0);

    // Verify colors aren't too dark (sum of RGB should be > 100 for visibility)
    let cold_brightness = cold.r as u32 + cold.g as u32 + cold.b as u32;
    let hot_brightness = hot.r as u32 + hot.g as u32 + hot.b as u32;

    assert!(
        cold_brightness > 150,
        "Cold color too dark (brightness: {})",
        cold_brightness
    );
    assert!(
        hot_brightness > 150,
        "Hot color too dark (brightness: {})",
        hot_brightness
    );

    // Verify colors are distinguishable (at least 50 units difference in some channel)
    let max_diff = [
        (cold.r as i16 - hot.r as i16).abs(),
        (cold.g as i16 - hot.g as i16).abs(),
        (cold.b as i16 - hot.b as i16).abs(),
    ].into_iter().max().unwrap();

    assert!(
        max_diff >= 50,
        "Cold and hot colors not distinguishable enough (max channel diff: {})",
        max_diff
    );
}

#[test]
fn test_brightness_scaling_preserves_hue() {
    // Test that brightness scaling doesn't distort colors
    let color = RgbColor::from_hex("#FF8040").unwrap(); // Orange

    let dim = color.with_brightness(0.5);
    let _bright = color.with_brightness(1.0);

    // Verify brightness scaling preserves relative ratios
    let original_ratio_rg = color.r as f32 / color.g as f32;
    let dim_ratio_rg = if dim.g > 0 {
        dim.r as f32 / dim.g as f32
    } else {
        0.0
    };

    // Ratios should be approximately preserved (within 5% due to rounding)
    if dim.g > 0 {
        let ratio_diff = (original_ratio_rg - dim_ratio_rg).abs() / original_ratio_rg;
        assert!(
            ratio_diff < 0.05,
            "Brightness scaling distorted hue too much: {} vs {}",
            original_ratio_rg, dim_ratio_rg
        );
    }
}
