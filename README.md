# Tenstorrent RGB Lighting Controller

A Rust-based systemd service that monitors Tenstorrent hardware metrics (temperature, power) and dynamically controls RGB lighting via OpenRGB. The lights change color based on real-time hardware status - cool teal when idle, transitioning through purple to red as temperatures rise.

## Features

- **Passive Monitoring**: Reads temperature and power data directly from `/sys/class/hwmon` (lm-sensors interface)
- **Multi-Architecture Support**: Works with all Tenstorrent devices:
  - Blackhole (P150, P300C)
  - Wormhole B0 (N150, N300)
  - Grayskull (E75, E150)
- **Dynamic Color Mapping**: Smooth color gradients based on temperature
- **Power-Based Brightness**: Dimmer when idle, brighter under load
- **Warning Effects**: Pulsing lights when temperatures exceed thresholds
- **OpenRGB Integration**: Works with any RGB device supported by OpenRGB
- **Low Overhead**: Minimal CPU usage, passive data collection

## Architecture

```
┌─────────────────────┐
│  Hardware Sensors   │
│  /sys/class/hwmon   │
└──────────┬──────────┘
           │
           │ Poll (1Hz)
           ▼
┌─────────────────────┐
│  tt-qb-lights       │
│  (Rust service)     │
│                     │
│  • Read temps       │
│  • Map to colors    │
│  • Calculate FX     │
└──────────┬──────────┘
           │
           │ TCP (port 6742)
           ▼
┌─────────────────────┐
│  OpenRGB Server     │
│                     │
│  • ASRock B850M-C   │
│  • 240 RGB LEDs     │
└─────────────────────┘
```

## Requirements

- Linux system with Tenstorrent hardware
- Rust 1.70+ (for building)
- OpenRGB installed and running with SDK server enabled
- Tenstorrent drivers loaded (creates `/sys/class/hwmon/blackhole-pci-*` entries)

## Installation

### 1. Build the Project

```bash
cd /home/ttuser/code/tt-qb-lights
cargo build --release
```

The binary will be created at `target/release/tt-qb-lights`.

### 2. Configure OpenRGB

Start OpenRGB with SDK server enabled:

```bash
openrgb --server
```

Or enable it in the OpenRGB GUI: Settings → Enable SDK Server

List available RGB devices:

```bash
openrgb --list-devices
```

Note your device name (e.g., "ASRock B850M-C") for the configuration file.

### 3. Edit Configuration

Copy and edit `config.toml` to match your hardware:

```bash
cp config.toml config.toml.bak
nano config.toml
```

Key settings to review:
- `[openrgb]` section: Set your RGB device name
- `[color_mapping]` section: Choose color scheme or customize
- `[effects]` section: Enable/disable power brightness and warning pulse

### 4. Test Manually

Run in single-shot mode to verify sensor detection:

```bash
./target/release/tt-qb-lights --single-shot
```

Expected output:
```
================================================================================
                        Tenstorrent Device Metrics
================================================================================
Bus ID          Board      Architecture Temp (°C)  Power (W) Fan RPM
--------------------------------------------------------------------------------
0000:01:00.0    p300c      blackhole          45.0      150.0        0
0000:02:00.0    p300c      blackhole          43.0      145.0        0
================================================================================
```

Run in dry-run mode to test color mapping without controlling lights:

```bash
./target/release/tt-qb-lights --dry-run --debug
```

### 5. Install as systemd Service

Copy the service file:

```bash
sudo cp tt-qb-lights.service /etc/systemd/system/
sudo systemctl daemon-reload
```

Enable and start the service:

```bash
sudo systemctl enable tt-qb-lights
sudo systemctl start tt-qb-lights
```

Check status:

```bash
sudo systemctl status tt-qb-lights
```

View logs:

```bash
journalctl -u tt-qb-lights -f
```

## Configuration

### Color Schemes

Two built-in color schemes are available:

**Teal → Purple → Red** (default, cool elegant theme):
- 20°C: Deep Teal `#008B8B`
- 30°C: Bright Cyan `#00CED1`
- 40°C: Purple `#8B00FF`
- 50°C: Magenta `#FF00FF`
- 60°C: Hot Pink `#FF0066`
- 70°C+: Red `#FF0000`

**Seafoam → Yellow → Orange** (natural warm theme):
- 20°C: Seafoam `#20B2AA`
- 30°C: Lime Green `#32CD32`
- 40°C: Yellow-Green `#9ACD32`
- 50°C: Gold `#FFD700`
- 60°C: Orange `#FF8C00`
- 70°C+: Red-Orange `#FF4500`

Switch schemes by changing `scheme = "seafoam_yellow_orange"` in `config.toml`.

### Effects

- **Power Brightness**: Brightness scales with power consumption (30% idle → 100% full load)
- **Warning Pulse**: Lights pulse when temperature exceeds threshold (default 70°C)

## Usage

### Command-Line Options

```
tt-qb-lights [OPTIONS]

Options:
  -c, --config <FILE>    Path to configuration file [default: config.toml]
  -d, --debug            Enable debug logging
      --dry-run          Don't control RGB lights (test mode)
  -s, --single-shot      Print metrics once and exit
  -h, --help             Print help
  -V, --version          Print version
```

### Examples

**Single measurement:**
```bash
tt-qb-lights --single-shot
```

**Test color mapping without controlling lights:**
```bash
tt-qb-lights --dry-run --debug
```

**Run with custom config:**
```bash
tt-qb-lights --config /etc/tt-qb-lights/custom.toml
```

## Troubleshooting

### No Devices Found

**Error**: `No Tenstorrent devices found in /sys/class/hwmon`

**Solution**: Check that Tenstorrent drivers are loaded:
```bash
sensors | grep blackhole
tt-smi
```

If devices aren't showing up, try:
```bash
sudo tt-cold-reboot  # Reload kernel drivers
```

### OpenRGB Connection Failed

**Error**: `Failed to connect to OpenRGB server`

**Solution**: Ensure OpenRGB server is running:
```bash
openrgb --server
```

Check that port 6742 is accessible:
```bash
netstat -tlnp | grep 6742
```

### Device Name Not Found

**Error**: `Device 'ASRock B850M-C' not found`

**Solution**: List available devices:
```bash
openrgb --list-devices
```

Update `config.toml` with the exact device name shown.

### Permission Denied

**Error**: `Permission denied` when reading `/sys/class/hwmon`

**Solution**: Run as user with access to hwmon devices, or add user to appropriate group:
```bash
sudo usermod -a -G video $USER
```

## Architecture Details

### Device Detection

The service automatically detects Tenstorrent devices by scanning `/sys/class/hwmon` for entries matching the pattern:
- `blackhole-pci-XXXX` (Blackhole architecture)
- `wormhole-pci-XXXX` (Wormhole architecture)
- `grayskull-pci-XXXX` (Grayskull architecture)

### Sensor Reading

Temperature sensors are read from:
- `temp1_input`: ASIC temperature (main chip)
- `temp2_input`, `temp3_input`, etc.: GDDR memory temperatures

Power sensors (if available):
- `power1_input`: Input power in microwatts

All sensor values are read directly from sysfs without invoking external commands.

### Color Interpolation

Colors are linearly interpolated between temperature thresholds. For example, at 35°C (midpoint between 20°C and 50°C thresholds), the color will be a 50/50 mix of the two threshold colors.

### OpenRGB Protocol

The service implements the OpenRGB SDK protocol (v3) over TCP:
- Packet format: Magic bytes (ORGB) + Device ID + Packet ID + Data length + Payload
- Commands used: SetClientName, RequestControllerCount, RequestControllerData, UpdateLeds

## Development

### Project Structure

```
tt-qb-lights/
├── src/
│   ├── main.rs              # Entry point and main loop
│   ├── config.rs            # Configuration file handling
│   ├── monitoring/
│   │   ├── mod.rs           # Monitoring trait definitions
│   │   ├── sensors.rs       # lm-sensors hwmon reader
│   │   └── tenstorrent.rs   # tt-smi JSON parser (alternative)
│   └── rgb/
│       ├── mod.rs           # RGB controller trait
│       ├── openrgb.rs       # OpenRGB TCP client
│       └── color_mapping.rs # Temperature to color mapping
├── config.toml              # User configuration
├── tt-qb-lights.service     # systemd service file
└── README.md                # This file
```

### Running Tests

```bash
cargo test
```

### Debugging

Enable debug logging:
```bash
RUST_LOG=debug ./target/release/tt-qb-lights
```

Or use the `--debug` flag:
```bash
./target/release/tt-qb-lights --debug
```

## Future Enhancements

- [ ] Per-device zone mapping (each RGB zone shows a different device)
- [ ] Gradient mode (smooth gradient across all LEDs)
- [ ] Web dashboard for live monitoring and configuration
- [ ] Support for other RGB protocols (RGB Fusion, Aura Sync)
- [ ] Historical temperature graphing
- [ ] Alert notifications (desktop/email)
- [ ] Multi-host support (monitor remote Tenstorrent systems)
- [ ] Integration with Prometheus/Grafana

## License

This project is open source. Use it, modify it, share it!

## Credits

Built for the Tenstorrent community to make those QuadBeast lights actually useful!

Hardware monitoring inspired by lm-sensors and the Linux hwmon subsystem.
RGB control powered by the excellent OpenRGB project.
