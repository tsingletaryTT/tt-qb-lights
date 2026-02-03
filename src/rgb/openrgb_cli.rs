// OpenRGB CLI wrapper - uses the openrgb command-line tool directly
// This is more reliable than implementing the TCP protocol ourselves

use super::{RgbColor, RgbController};
use anyhow::{Context, Result};
use std::process::Command;

/// Capture the current active mode for a device
fn capture_current_mode(device_id: u32) -> Option<String> {
    // Use openrgb --list-devices to get current device info
    let output = Command::new("openrgb")
        .arg("--list-devices")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse the output to find the active mode (shown in brackets like [Off] or ['Spectrum Cycle'])
    // Look for a line like: "  Modes: [Off] Static Breathing..."
    for line in stdout.lines() {
        if line.trim().starts_with("Modes:") {
            // Extract the mode in brackets
            if let Some(start) = line.find('[') {
                if let Some(end) = line.find(']') {
                    if end > start + 1 {
                        let mut mode = line[start + 1..end].to_string();
                        // Remove surrounding single quotes if present
                        if mode.starts_with('\'') && mode.ends_with('\'') && mode.len() > 2 {
                            mode = mode[1..mode.len() - 1].to_string();
                        }
                        return Some(mode);
                    }
                }
            }
        }
    }

    None
}

/// OpenRGB CLI controller
/// Uses the `openrgb` command-line tool for RGB control
pub struct OpenRgbCliController {
    device_id: u32,
    device_name: String,
    led_count: usize,
    original_mode: Option<String>,
}

impl OpenRgbCliController {
    /// Create a new OpenRGB CLI controller
    pub fn connect(_host: &str, _port: u16, device_name: &str) -> Result<Self> {
        tracing::info!("Using OpenRGB CLI for RGB control");

        // Verify openrgb command is available
        let output = Command::new("openrgb")
            .arg("--list-devices")
            .output()
            .context("Failed to execute openrgb. Is it installed?")?;

        if !output.status.success() {
            anyhow::bail!("openrgb command failed. Please install OpenRGB.");
        }

        tracing::info!("OpenRGB CLI detected");

        // For simplicity, use device 0 (most systems have one RGB controller)
        let device_id = 0;
        let led_count = 240; // Typical for ASRock 3-header setup

        // Capture the original mode before we change it
        let original_mode = capture_current_mode(device_id);
        if let Some(ref mode) = original_mode {
            tracing::info!("Saved original RGB mode: {}", mode);
        } else {
            tracing::warn!("Could not detect original RGB mode, will turn off on exit");
        }

        // Set to Static mode (more stable than Direct)
        let _ = Command::new("openrgb")
            .args(&["--device", &device_id.to_string(), "--mode", "static", "--color", "00CED1"])
            .output();

        tracing::info!("Connected to OpenRGB device '{}' (ID: {}, {} LEDs)", device_name, device_id, led_count);

        Ok(Self {
            device_id,
            device_name: device_name.to_string(),
            led_count,
            original_mode,
        })
    }

    /// Restore the original RGB mode
    pub fn restore_original(&self) {
        if let Some(ref mode) = self.original_mode {
            tracing::info!("Restoring original RGB mode: {}", mode);
            let _ = Command::new("openrgb")
                .args(&["--device", &self.device_id.to_string(), "--mode", mode])
                .output();
        } else {
            tracing::info!("Turning off RGB lights");
            let _ = Command::new("openrgb")
                .args(&["--device", &self.device_id.to_string(), "--mode", "off"])
                .output();
        }
    }
}

impl RgbController for OpenRgbCliController {
    fn set_all(&mut self, color: RgbColor, brightness: f32) -> Result<()> {
        let adjusted = color.with_brightness(brightness);

        // Convert to hex format (RRGGBB)
        let hex_color = format!("{:02X}{:02X}{:02X}", adjusted.r, adjusted.g, adjusted.b);

        // Execute openrgb command with static mode (more stable)
        let output = Command::new("openrgb")
            .args(&[
                "--device",
                &self.device_id.to_string(),
                "--mode",
                "static",
                "--color",
                &hex_color,
            ])
            .output()
            .context("Failed to execute openrgb command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("openrgb command failed: {}", stderr);
        }

        tracing::debug!("Updated RGB to #{} @ {:.0}%", hex_color, brightness * 100.0);

        Ok(())
    }

    fn set_leds(&mut self, colors: &[RgbColor], brightness: f32) -> Result<()> {
        // For now, just set all to the first color
        // Full per-LED control would require more complex CLI usage
        if let Some(first_color) = colors.first() {
            self.set_all(*first_color, brightness)
        } else {
            Ok(())
        }
    }

    fn led_count(&self) -> usize {
        self.led_count
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }
}
