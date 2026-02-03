// Color mapping logic: converts temperature readings to RGB colors
// Handles interpolation between temperature thresholds and color schemes

use super::RgbColor;
use crate::config::ColorThreshold;
use anyhow::Result;

/// Color mapper that interpolates colors based on temperature thresholds
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
}
