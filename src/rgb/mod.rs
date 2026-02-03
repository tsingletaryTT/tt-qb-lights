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
    fn test_rgb_brightness() {
        let color = RgbColor::new(255, 128, 64);
        let dimmed = color.with_brightness(0.5);
        assert_eq!(dimmed.r, 127);
        assert_eq!(dimmed.g, 64);
        assert_eq!(dimmed.b, 32);
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
}
