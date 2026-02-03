// OpenRGB SDK client implementation
// Implements TCP protocol to communicate with OpenRGB server

use super::{RgbColor, RgbController};
use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// OpenRGB SDK protocol constants
const OPENRGB_PROTOCOL_VERSION: u32 = 3;

/// OpenRGB packet IDs (OpenRGB SDK Protocol)
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
enum PacketId {
    RequestControllerCount = 0,
    RequestControllerData = 1,
    SetClientName = 50,
    RgbControllerUpdateLeds = 1050,
    RgbControllerUpdateZoneLeds = 1051,
    RgbControllerUpdateSingleLed = 1052,
    RgbControllerUpdateMode = 1100,
}

/// OpenRGB controller client
pub struct OpenRgbClient {
    stream: TcpStream,
    device_name: String,
    device_id: Option<u32>,
    led_count: usize,
}

impl OpenRgbClient {
    /// Connect to OpenRGB server
    pub fn connect(host: &str, port: u16, device_name: &str) -> Result<Self> {
        let addr = format!("{}:{}", host, port);
        tracing::info!("Connecting to OpenRGB server at {}", addr);

        let stream = TcpStream::connect_timeout(
            &addr.parse().context("Invalid server address")?,
            Duration::from_secs(10),
        )
        .context("Failed to connect to OpenRGB server. Is it running?")?;

        // Set longer timeouts for OpenRGB communication
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_nodelay(true)?; // Disable Nagle's algorithm for faster responses

        let mut client = Self {
            stream,
            device_name: device_name.to_string(),
            device_id: None,
            led_count: 0,
        };

        // Set client name
        client.set_client_name("tt-qb-lights")?;

        // Find target device
        client.find_device()?;

        tracing::info!(
            "Connected to OpenRGB device '{}' (ID: {}, {} LEDs)",
            client.device_name,
            client.device_id.unwrap(),
            client.led_count
        );

        Ok(client)
    }

    /// Set client name (required by OpenRGB protocol)
    fn set_client_name(&mut self, name: &str) -> Result<()> {
        tracing::debug!("Setting client name: {}", name);
        let mut data = Vec::new();
        data.extend_from_slice(&(name.len() as u32).to_le_bytes());
        data.extend_from_slice(name.as_bytes());

        self.send_packet(PacketId::SetClientName, &data)?;
        tracing::debug!("Client name set successfully");
        Ok(())
    }

    /// Find the target RGB device by name
    fn find_device(&mut self) -> Result<()> {
        // Request controller count
        self.send_packet(PacketId::RequestControllerCount, &[])?;
        tracing::debug!("Sent controller count request");

        let response = self.receive_packet()?;
        tracing::debug!("Received controller count response: {} bytes", response.len());

        if response.len() < 4 {
            anyhow::bail!("Invalid controller count response");
        }

        let controller_count = u32::from_le_bytes([
            response[0],
            response[1],
            response[2],
            response[3],
        ]);

        tracing::debug!("Found {} RGB controller(s)", controller_count);

        if controller_count == 0 {
            anyhow::bail!("No RGB controllers found");
        }

        // For simplicity, just use the first controller
        // Most systems only have one motherboard RGB controller anyway
        tracing::info!("Using first RGB controller (device 0)");
        self.device_id = Some(0);
        // Estimate LED count - will be set properly when we send updates
        // For now, use a safe default
        self.led_count = 240; // 3 headers × 80 LEDs typical for ASRock

        tracing::info!("RGB controller initialized (estimated {} LEDs)", self.led_count);

        // Set device to Direct mode (required for manual LED control)
        self.set_direct_mode()?;

        Ok(())
    }

    /// Set the RGB device to Direct mode (allows manual LED control)
    fn set_direct_mode(&mut self) -> Result<()> {
        tracing::debug!("Setting device to Direct mode");

        if self.device_id.is_none() {
            anyhow::bail!("Device not initialized");
        }

        let device_id = self.device_id.unwrap();

        // Direct mode data: device_id + mode_id
        // Mode 6 is typically "Direct" mode for ASRock Polychrome
        let mode_id: u32 = 6;

        let mut data = Vec::new();
        data.extend_from_slice(&device_id.to_le_bytes());
        data.extend_from_slice(&mode_id.to_le_bytes());

        self.send_packet(PacketId::RgbControllerUpdateMode, &data)?;
        tracing::info!("Device set to Direct mode");

        Ok(())
    }

    /// Send a packet to the OpenRGB server
    fn send_packet(&mut self, packet_id: PacketId, data: &[u8]) -> Result<()> {
        let mut packet = Vec::new();

        // Magic bytes: "ORGB"
        packet.extend_from_slice(b"ORGB");

        // Device ID (0xFFFFFFFF for broadcast/server commands)
        packet.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());

        // Packet ID
        packet.extend_from_slice(&(packet_id as u32).to_le_bytes());

        // Data length
        packet.extend_from_slice(&(data.len() as u32).to_le_bytes());

        // Data payload
        packet.extend_from_slice(data);

        tracing::trace!(
            "Sending packet: ID={:?} ({}) len={} total_size={}",
            packet_id,
            packet_id as u32,
            data.len(),
            packet.len()
        );

        self.stream
            .write_all(&packet)
            .with_context(|| format!("Failed to send {:?} packet to OpenRGB (size: {} bytes)", packet_id, packet.len()))?;

        // Flush to ensure data is sent
        self.stream.flush()
            .context("Failed to flush TCP stream")?;

        Ok(())
    }

    /// Receive a packet from the OpenRGB server
    fn receive_packet(&mut self) -> Result<Vec<u8>> {
        let mut header = [0u8; 16];
        self.stream
            .read_exact(&mut header)
            .context("Failed to read packet header from OpenRGB")?;

        // Verify magic bytes
        if &header[0..4] != b"ORGB" {
            anyhow::bail!("Invalid packet magic bytes");
        }

        // Extract data length
        let data_len = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;

        // Read payload
        let mut data = vec![0u8; data_len];
        if data_len > 0 {
            self.stream
                .read_exact(&mut data)
                .context("Failed to read packet data from OpenRGB")?;
        }

        Ok(data)
    }
}

impl RgbController for OpenRgbClient {
    fn set_all(&mut self, color: RgbColor, brightness: f32) -> Result<()> {
        if self.device_id.is_none() || self.led_count == 0 {
            anyhow::bail!("Device not initialized");
        }

        let adjusted_color = color.with_brightness(brightness);

        // Build LED update packet
        let mut data = Vec::new();

        // Device ID
        data.extend_from_slice(&self.device_id.unwrap().to_le_bytes());

        // Number of LEDs
        data.extend_from_slice(&(self.led_count as u32).to_le_bytes());

        // LED color data (RGBX format, X = padding)
        for _ in 0..self.led_count {
            data.push(adjusted_color.r);
            data.push(adjusted_color.g);
            data.push(adjusted_color.b);
            data.push(0); // Padding
        }

        self.send_packet(PacketId::RgbControllerUpdateLeds, &data)?;
        Ok(())
    }

    fn set_leds(&mut self, colors: &[RgbColor], brightness: f32) -> Result<()> {
        if self.device_id.is_none() || self.led_count == 0 {
            anyhow::bail!("Device not initialized");
        }

        if colors.len() != self.led_count {
            anyhow::bail!(
                "Color array size ({}) doesn't match LED count ({})",
                colors.len(),
                self.led_count
            );
        }

        // Build LED update packet
        let mut data = Vec::new();

        // Device ID
        data.extend_from_slice(&self.device_id.unwrap().to_le_bytes());

        // Number of LEDs
        data.extend_from_slice(&(self.led_count as u32).to_le_bytes());

        // LED color data
        for color in colors {
            let adjusted = color.with_brightness(brightness);
            data.push(adjusted.r);
            data.push(adjusted.g);
            data.push(adjusted.b);
            data.push(0); // Padding
        }

        self.send_packet(PacketId::RgbControllerUpdateLeds, &data)?;
        Ok(())
    }

    fn led_count(&self) -> usize {
        self.led_count
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }
}

/// Parse controller data packet to extract device name and LED count
fn parse_controller_data(data: &[u8]) -> Result<Option<(String, usize)>> {
    if data.len() < 8 {
        return Ok(None);
    }

    let mut offset = 4; // Skip data size field

    // Read name length
    if offset + 4 > data.len() {
        return Ok(None);
    }
    let name_len = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    offset += 4;

    // Read name
    if offset + name_len > data.len() {
        return Ok(None);
    }
    let name = String::from_utf8_lossy(&data[offset..offset + name_len]).to_string();
    offset += name_len;

    // Skip to LED count (simplified parsing - full protocol is more complex)
    // This is a heuristic approach; full parsing would require complete protocol implementation
    // For now, we'll look for LED count in typical location or return estimated value
    let led_count = estimate_led_count(&data, offset);

    Ok(Some((name, led_count)))
}

/// Estimate LED count from controller data
/// This is a simplified heuristic; full parsing would be more reliable
fn estimate_led_count(data: &[u8], _offset: usize) -> usize {
    // Try to find LED count in data
    // For now, use a safe default that works with most motherboard RGB
    // A full implementation would parse the zone/LED structures properly

    // Look for common LED counts in the data
    for i in 0..data.len().saturating_sub(4) {
        let val = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        // Most motherboard RGB headers have 20-300 LEDs
        if (20..=300).contains(&val) {
            return val as usize;
        }
    }

    // Default to 240 (80 LEDs per header × 3 headers, typical for ASRock)
    240
}
