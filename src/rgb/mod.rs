// RGB lighting control module
// Manages OpenRGB connection and color updates

pub mod color_mapping;
pub mod openrgb;
pub mod openrgb_cli;

use anyhow::Result;

/// RGB color in 8-bit format
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Create color from hex string (e.g., "#FF0000")
    pub fn from_hex(hex: &str) -> Result<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            anyhow::bail!("Invalid hex color format: expected 6 characters");
        }

        let r = u8::from_str_radix(&hex[0..2], 16)?;
        let g = u8::from_str_radix(&hex[2..4], 16)?;
        let b = u8::from_str_radix(&hex[4..6], 16)?;

        Ok(Self { r, g, b })
    }

    /// Scale color by brightness (0.0 to 1.0)
    pub fn with_brightness(&self, brightness: f32) -> Self {
        let brightness = brightness.max(0.0).min(1.0);
        Self {
            r: (self.r as f32 * brightness) as u8,
            g: (self.g as f32 * brightness) as u8,
            b: (self.b as f32 * brightness) as u8,
        }
    }

    /// Linearly interpolate between two colors
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        let t = t.max(0.0).min(1.0);
        Self {
            r: (self.r as f32 * (1.0 - t) + other.r as f32 * t) as u8,
            g: (self.g as f32 * (1.0 - t) + other.g as f32 * t) as u8,
            b: (self.b as f32 * (1.0 - t) + other.b as f32 * t) as u8,
        }
    }
}

/// Trait for RGB controller implementations
pub trait RgbController: Send + Sync {
    /// Set all LEDs to a single color
    fn set_all(&mut self, color: RgbColor, brightness: f32) -> Result<()>;

    /// Set individual LED colors (for gradient effects)
    fn set_leds(&mut self, colors: &[RgbColor], brightness: f32) -> Result<()>;

    /// Get number of controllable LEDs
    fn led_count(&self) -> usize;

    /// Get RGB device name
    fn device_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgb_from_hex() {
        let color = RgbColor::from_hex("#FF0000").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);

        let color = RgbColor::from_hex("00FF00").unwrap();
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn test_rgb_from_hex_lowercase() {
        let color = RgbColor::from_hex("#ff00ff").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 255);
    }

    #[test]
    fn test_rgb_from_hex_mixed_case() {
        let color = RgbColor::from_hex("#FfA5c0").unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 165);
        assert_eq!(color.b, 192);
    }

    #[test]
    fn test_rgb_from_hex_invalid_length() {
        let result = RgbColor::from_hex("#FF00");
        assert!(result.is_err());

        let result = RgbColor::from_hex("#FF00000");
        assert!(result.is_err());
    }

    #[test]
    fn test_rgb_from_hex_invalid_chars() {
        let result = RgbColor::from_hex("#GGGGGG");
        assert!(result.is_err());

        let result = RgbColor::from_hex("#FF00GG");
        assert!(result.is_err());
    }

    #[test]
    fn test_rgb_brightness() {
        let color = RgbColor::new(255, 128, 64);
        let dimmed = color.with_brightness(0.5);
        assert_eq!(dimmed.r, 127);
        assert_eq!(dimmed.g, 64);
        assert_eq!(dimmed.b, 32);
    }

    #[test]
    fn test_rgb_brightness_zero() {
        let color = RgbColor::new(255, 255, 255);
        let black = color.with_brightness(0.0);
        assert_eq!(black.r, 0);
        assert_eq!(black.g, 0);
        assert_eq!(black.b, 0);
    }

    #[test]
    fn test_rgb_brightness_full() {
        let color = RgbColor::new(128, 64, 32);
        let full = color.with_brightness(1.0);
        assert_eq!(full.r, 128);
        assert_eq!(full.g, 64);
        assert_eq!(full.b, 32);
    }

    #[test]
    fn test_rgb_brightness_clamping() {
        let color = RgbColor::new(255, 128, 64);

        // Test values above 1.0 are clamped
        let clamped = color.with_brightness(2.0);
        assert_eq!(clamped.r, 255);
        assert_eq!(clamped.g, 128);
        assert_eq!(clamped.b, 64);

        // Test negative values are clamped to 0
        let clamped = color.with_brightness(-0.5);
        assert_eq!(clamped.r, 0);
        assert_eq!(clamped.g, 0);
        assert_eq!(clamped.b, 0);
    }

    #[test]
    fn test_rgb_lerp() {
        let c1 = RgbColor::new(0, 0, 0);
        let c2 = RgbColor::new(100, 100, 100);
        let mid = c1.lerp(&c2, 0.5);
        assert_eq!(mid.r, 50);
        assert_eq!(mid.g, 50);
        assert_eq!(mid.b, 50);
    }

    #[test]
    fn test_rgb_lerp_extremes() {
        let c1 = RgbColor::new(255, 0, 128);
        let c2 = RgbColor::new(0, 255, 64);

        // t=0.0 should return c1
        let at_zero = c1.lerp(&c2, 0.0);
        assert_eq!(at_zero, c1);

        // t=1.0 should return c2
        let at_one = c1.lerp(&c2, 1.0);
        assert_eq!(at_one, c2);
    }

    #[test]
    fn test_rgb_lerp_clamping() {
        let c1 = RgbColor::new(100, 100, 100);
        let c2 = RgbColor::new(200, 200, 200);

        // Values beyond 1.0 should clamp to c2
        let clamped = c1.lerp(&c2, 2.0);
        assert_eq!(clamped, c2);

        // Negative values should clamp to c1
        let clamped = c1.lerp(&c2, -0.5);
        assert_eq!(clamped, c1);
    }

    #[test]
    fn test_rgb_lerp_asymmetric() {
        // Test with different R, G, B values
        let c1 = RgbColor::new(0, 100, 200);
        let c2 = RgbColor::new(200, 150, 0);

        let quarter = c1.lerp(&c2, 0.25);
        assert_eq!(quarter.r, 50);   // 0 + (200-0)*0.25 = 50
        assert_eq!(quarter.g, 112);  // 100 + (150-100)*0.25 = 112.5 → 112
        assert_eq!(quarter.b, 150);  // 200 + (0-200)*0.25 = 150
    }

    #[test]
    fn test_quietbox_sunset_colors() {
        // Test colors from the new quietbox_sunset scheme
        let sky_teal = RgbColor::from_hex("#4DB8A5").unwrap();
        assert_eq!(sky_teal.r, 77);
        assert_eq!(sky_teal.g, 184);
        assert_eq!(sky_teal.b, 165);

        let bright_cyan = RgbColor::from_hex("#6FD8D5").unwrap();
        assert_eq!(bright_cyan.r, 111);
        assert_eq!(bright_cyan.g, 216);
        assert_eq!(bright_cyan.b, 213);

        let coral = RgbColor::from_hex("#E88B8B").unwrap();
        assert_eq!(coral.r, 232);
        assert_eq!(coral.g, 139);
        assert_eq!(coral.b, 139);
    }

    #[test]
    fn test_tt_dark_colors() {
        // Test colors from the new tt_dark scheme
        let tt_teal = RgbColor::from_hex("#4FD1C5").unwrap();
        assert_eq!(tt_teal.r, 79);
        assert_eq!(tt_teal.g, 209);
        assert_eq!(tt_teal.b, 197);

        let pink = RgbColor::from_hex("#EC96B8").unwrap();
        assert_eq!(pink.r, 236);
        assert_eq!(pink.g, 150);
        assert_eq!(pink.b, 184);

        let gold = RgbColor::from_hex("#F4C471").unwrap();
        assert_eq!(gold.r, 244);
        assert_eq!(gold.g, 196);
        assert_eq!(gold.b, 113);
    }

    #[test]
    fn test_tt_light_colors() {
        // Test colors from the new tt_light scheme
        let bright_blue = RgbColor::from_hex("#3fb7de").unwrap();
        assert_eq!(bright_blue.r, 63);
        assert_eq!(bright_blue.g, 183);
        assert_eq!(bright_blue.b, 222);

        let purple = RgbColor::from_hex("#5347a4").unwrap();
        assert_eq!(purple.r, 83);
        assert_eq!(purple.g, 71);
        assert_eq!(purple.b, 164);

        let copper = RgbColor::from_hex("#82672b").unwrap();
        assert_eq!(copper.r, 130);
        assert_eq!(copper.g, 103);
        assert_eq!(copper.b, 43);
    }
}
