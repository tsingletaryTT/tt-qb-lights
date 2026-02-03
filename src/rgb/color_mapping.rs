// Color mapping logic: converts temperature readings to RGB colors
// Handles interpolation between temperature thresholds and color schemes

use super::RgbColor;
use crate::config::ColorThreshold;
use anyhow::Result;

/// Color mapper that interpolates colors based on temperature thresholds
#[derive(Debug)]
pub struct ColorMapper {
    thresholds: Vec<(f32, RgbColor)>,
}

impl ColorMapper {
    /// Create a new color mapper from configuration thresholds
    pub fn new(thresholds: &[ColorThreshold]) -> Result<Self> {
        if thresholds.is_empty() {
            anyhow::bail!("Color mapper requires at least one threshold");
        }

        let mut parsed_thresholds = Vec::new();
        for threshold in thresholds {
            let color = RgbColor::from_hex(&threshold.color)?;
            parsed_thresholds.push((threshold.temp, color));
        }

        // Ensure sorted by temperature
        parsed_thresholds.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        Ok(Self {
            thresholds: parsed_thresholds,
        })
    }

    /// Map a temperature to a color using linear interpolation
    pub fn map_temperature(&self, temp: f32) -> RgbColor {
        // Handle edge cases
        if temp <= self.thresholds[0].0 {
            return self.thresholds[0].1;
        }
        if temp >= self.thresholds[self.thresholds.len() - 1].0 {
            return self.thresholds[self.thresholds.len() - 1].1;
        }

        // Find the two thresholds to interpolate between
        for i in 0..self.thresholds.len() - 1 {
            let (temp_low, color_low) = self.thresholds[i];
            let (temp_high, color_high) = self.thresholds[i + 1];

            if temp >= temp_low && temp <= temp_high {
                // Calculate interpolation factor (0.0 to 1.0)
                let t = (temp - temp_low) / (temp_high - temp_low);
                return color_low.lerp(&color_high, t);
            }
        }

        // Fallback (should never reach here)
        self.thresholds[0].1
    }

    /// Get the minimum temperature threshold
    pub fn min_temp(&self) -> f32 {
        self.thresholds[0].0
    }

    /// Get the maximum temperature threshold
    pub fn max_temp(&self) -> f32 {
        self.thresholds[self.thresholds.len() - 1].0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ColorThreshold;

    fn create_test_mapper() -> ColorMapper {
        let thresholds = vec![
            ColorThreshold { temp: 20.0, color: "#00FF00".to_string() }, // Green
            ColorThreshold { temp: 50.0, color: "#FFFF00".to_string() }, // Yellow
            ColorThreshold { temp: 70.0, color: "#FF0000".to_string() }, // Red
        ];
        ColorMapper::new(&thresholds).unwrap()
    }

    #[test]
    fn test_color_mapper_edges() {
        let mapper = create_test_mapper();

        // Below minimum should return first color (green)
        let color = mapper.map_temperature(10.0);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 255);

        // Above maximum should return last color (red)
        let color = mapper.map_temperature(80.0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
    }

    #[test]
    fn test_color_mapper_interpolation() {
        let mapper = create_test_mapper();

        // Midpoint between green (20°C) and yellow (50°C) at 35°C
        let color = mapper.map_temperature(35.0);
        // Should be roughly halfway between green and yellow
        assert!(color.r > 100 && color.r < 150);
        assert_eq!(color.g, 255);

        // Midpoint between yellow (50°C) and red (70°C) at 60°C
        let color = mapper.map_temperature(60.0);
        assert_eq!(color.r, 255);
        // Should be roughly halfway in green channel
        assert!(color.g > 100 && color.g < 150);
    }

    #[test]
    fn test_exact_threshold_values() {
        let mapper = create_test_mapper();

        let color = mapper.map_temperature(20.0);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 0);

        let color = mapper.map_temperature(70.0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn test_color_mapper_min_max_temp() {
        let mapper = create_test_mapper();
        assert_eq!(mapper.min_temp(), 20.0);
        assert_eq!(mapper.max_temp(), 70.0);
    }

    #[test]
    fn test_color_mapper_empty_thresholds() {
        let thresholds: Vec<ColorThreshold> = vec![];
        let result = ColorMapper::new(&thresholds);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least one threshold"));
    }

    #[test]
    fn test_color_mapper_single_threshold() {
        let thresholds = vec![
            ColorThreshold { temp: 50.0, color: "#FF00FF".to_string() },
        ];
        let mapper = ColorMapper::new(&thresholds).unwrap();

        // All temperatures should map to the single color
        let color = mapper.map_temperature(0.0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 255);

        let color = mapper.map_temperature(50.0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 255);

        let color = mapper.map_temperature(100.0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 255);
    }

    #[test]
    fn test_color_mapper_unsorted_thresholds() {
        // Mapper should sort thresholds automatically
        let thresholds = vec![
            ColorThreshold { temp: 70.0, color: "#FF0000".to_string() },
            ColorThreshold { temp: 20.0, color: "#00FF00".to_string() },
            ColorThreshold { temp: 50.0, color: "#FFFF00".to_string() },
        ];
        let mapper = ColorMapper::new(&thresholds).unwrap();

        // Should work correctly despite unsorted input
        assert_eq!(mapper.min_temp(), 20.0);
        assert_eq!(mapper.max_temp(), 70.0);

        let color = mapper.map_temperature(20.0);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn test_color_mapper_invalid_hex() {
        let thresholds = vec![
            ColorThreshold { temp: 20.0, color: "INVALID".to_string() },
        ];
        let result = ColorMapper::new(&thresholds);
        assert!(result.is_err());
    }

    #[test]
    fn test_quietbox_sunset_scheme() {
        // Test the new quietbox_sunset color scheme
        let thresholds = vec![
            ColorThreshold { temp: 20.0, color: "#4DB8A5".to_string() },  // Sky Teal
            ColorThreshold { temp: 35.0, color: "#6FD8D5".to_string() },  // Bright Cyan
            ColorThreshold { temp: 50.0, color: "#E88B8B".to_string() },  // Coral
            ColorThreshold { temp: 60.0, color: "#F5A4A4".to_string() },  // Salmon
            ColorThreshold { temp: 70.0, color: "#C23B3B".to_string() },  // Deep Red
        ];
        let mapper = ColorMapper::new(&thresholds).unwrap();

        // Test exact thresholds
        let color = mapper.map_temperature(20.0);
        assert_eq!(color.r, 77);
        assert_eq!(color.g, 184);

        let color = mapper.map_temperature(70.0);
        assert_eq!(color.r, 194);
        assert_eq!(color.g, 59);
        assert_eq!(color.b, 59);

        // Test interpolation between thresholds
        let color = mapper.map_temperature(27.5);  // Midpoint between 20 and 35
        // Should be roughly between Sky Teal and Bright Cyan
        assert!(color.r > 77 && color.r < 111);
        assert!(color.g > 184 && color.g < 216);
    }

    #[test]
    fn test_tt_dark_scheme() {
        // Test the new tt_dark color scheme
        let thresholds = vec![
            ColorThreshold { temp: 20.0, color: "#4FD1C5".to_string() },  // Teal
            ColorThreshold { temp: 35.0, color: "#81E6D9".to_string() },  // Light Teal
            ColorThreshold { temp: 50.0, color: "#EC96B8".to_string() },  // Pink
            ColorThreshold { temp: 60.0, color: "#F4C471".to_string() },  // Gold
            ColorThreshold { temp: 70.0, color: "#FF6B6B".to_string() },  // Red
        ];
        let mapper = ColorMapper::new(&thresholds).unwrap();

        // Test exact thresholds
        let color = mapper.map_temperature(20.0);
        assert_eq!(color.r, 79);

        let color = mapper.map_temperature(50.0);
        assert_eq!(color.r, 236);
        assert_eq!(color.g, 150);

        let color = mapper.map_temperature(70.0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 107);
    }

    #[test]
    fn test_tt_light_scheme() {
        // Test the new tt_light color scheme
        let thresholds = vec![
            ColorThreshold { temp: 20.0, color: "#3fb7de".to_string() },  // Bright Blue
            ColorThreshold { temp: 35.0, color: "#3293b2".to_string() },  // Blue
            ColorThreshold { temp: 50.0, color: "#5347a4".to_string() },  // Purple
            ColorThreshold { temp: 60.0, color: "#82672b".to_string() },  // Copper
            ColorThreshold { temp: 70.0, color: "#d03a1b".to_string() },  // Red
        ];
        let mapper = ColorMapper::new(&thresholds).unwrap();

        // Test exact thresholds
        let color = mapper.map_temperature(20.0);
        assert_eq!(color.r, 63);
        assert_eq!(color.b, 222);

        let color = mapper.map_temperature(50.0);
        assert_eq!(color.r, 83);
        assert_eq!(color.b, 164);

        let color = mapper.map_temperature(70.0);
        assert_eq!(color.r, 208);
        assert_eq!(color.g, 58);
        assert_eq!(color.b, 27);
    }

    #[test]
    fn test_fine_grained_interpolation() {
        // Test interpolation with very close temperature values
        let thresholds = vec![
            ColorThreshold { temp: 50.0, color: "#000000".to_string() },  // Black
            ColorThreshold { temp: 50.1, color: "#FFFFFF".to_string() },  // White
        ];
        let mapper = ColorMapper::new(&thresholds).unwrap();

        // At 50.05 (midpoint), should be roughly gray
        let color = mapper.map_temperature(50.05);
        // Allow some tolerance for rounding
        assert!(color.r > 120 && color.r < 135);
        assert!(color.g > 120 && color.g < 135);
        assert!(color.b > 120 && color.b < 135);
    }
}
