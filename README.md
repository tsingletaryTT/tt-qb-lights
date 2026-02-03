# Tenstorrent RGB Lighting Controller

A Rust-based systemd service that monitors Tenstorrent hardware metrics (temperature, power) and dynamically controls RGB lighting via OpenRGB. The lights change color based on real-time hardware status - cool teal when idle, transitioning through purple to red as temperatures rise.

## QuietBox Quick Start

For Ubuntu 24.04 systems with Tenstorrent hardware, you can use the automated installer or install manually.

### Option 1: Automated Installer (Recommended)

The installer script checks for missing prerequisites and installs them:

```bash
cd /home/ttuser/code/tt-qb-lights
./install.sh
```

The script will:
- Check for Rust, OpenRGB, lm-sensors, and build tools
- Ask permission before installing anything (especially Rust)
- Run sensor detection to find Tenstorrent devices
- Build the project automatically
- Provide next steps for configuration

### Option 2: Manual Installation

If you prefer to install dependencies manually, run these commands:

```bash
# Update package lists
sudo apt update

# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Install OpenRGB (from official repository)
sudo apt install -y openrgb

# Install lm-sensors for hardware monitoring
sudo apt install -y lm-sensors

# Install build dependencies
sudo apt install -y build-essential pkg-config

# Detect sensors (will scan for Tenstorrent devices)
sudo sensors-detect --auto

# Verify Tenstorrent devices are detected
sensors | grep -i blackhole
```

**What you should see:**
```
blackhole-pci-0200
Adapter: PCI adapter
asic_temp:    +42.0°C
```

If you don't see Tenstorrent devices, ensure drivers are loaded:
```bash
# Check if Tenstorrent kernel modules are loaded
lsmod | grep tenstorrent

# If needed, reload drivers
sudo tt-cold-reboot
```

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

**Quick Install**: Run `./install.sh` to automatically check and install prerequisites, build the project, and set up your configuration. See [QuietBox Quick Start](#quietbox-quick-start) for details.

**Manual Installation**: Follow the steps below if you prefer manual control.

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

### 3. Initialize and Edit Configuration

Create your personal configuration file:

```bash
# Initialize default config (creates ~/.config/tt-qb-lights/config.toml)
./target/release/tt-qb-lights --init

# Edit the configuration
nano ~/.config/tt-qb-lights/config.toml
```

**Configuration Location**: Config is stored in your home directory, not in the project folder. This means:
- ✓ No need to rebuild after changing color schemes
- ✓ Settings persist across updates
- ✓ Each user can have their own config

Key settings to review:
- `[openrgb]` section: Set your RGB device name
- `[color_mapping]` section: Choose color scheme (change anytime, just restart service)
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

Six built-in color schemes are available:

**QuietBox Sunset** (default, inspired by QuietBox wallpaper):
- Cool morning → Sunset → Night heat
- 20°C: Sky Teal → 35°C: Bright Cyan → 50°C: Coral → 60°C: Salmon → 70°C+: Deep Red

**Teal → Purple → Red** (cool elegant theme):
- 20°C: Deep Teal → 30°C: Bright Cyan → 40°C: Purple → 50°C: Magenta → 60°C: Hot Pink → 70°C+: Red

**Seafoam → Yellow → Orange** (natural warm theme):
- 20°C: Seafoam → 30°C: Lime Green → 40°C: Yellow-Green → 50°C: Gold → 60°C: Orange → 70°C+: Red-Orange

**TT Dark** (inspired by Tenstorrent dark theme):
- Teal → Pink → Gold → Red
- 20°C: Teal → 35°C: Light Teal → 50°C: Pink → 60°C: Gold → 70°C+: Red

**TT Light** (inspired by Tenstorrent light theme):
- Blue → Purple → Copper → Red
- 20°C: Bright Blue → 35°C: Blue → 50°C: Purple → 60°C: Copper → 70°C+: Red

**Tenstorrent Branding** (official TT colors):
- TT Teal → Blue → Magenta → Orange → Red
- 20°C: TT Teal → 35°C: TT Blue → 50°C: Magenta → 60°C: Orange → 70°C+: Red

Switch schemes by changing `scheme = "scheme_name"` in `config.toml` (e.g., `scheme = "tt_dark"`).

### Effects

- **Power Brightness**: Brightness scales with power consumption (30% idle → 100% full load)
- **Warning Pulse**: Lights pulse when temperature exceeds threshold (default 70°C)

## Customization

**Easy Live Updates**: Your configuration is stored in `~/.config/tt-qb-lights/config.toml`. Edit it anytime and restart the service - no need to rebuild!

```bash
# Edit your config
nano ~/.config/tt-qb-lights/config.toml

# Restart the service to apply changes
sudo systemctl restart tt-qb-lights

# Or if running manually, just stop and restart
```

### Adjusting Color Sensitivity

The color scheme determines which colors appear at which temperatures. To make the lights change faster (more sensitive):

**Option 1: Choose a more sensitive color scheme**

Some schemes have tighter temperature ranges (colors change faster):
- `teal_purple_red` - Moderate, 6 steps across 20-70°C range
- `quietbox_sunset` - Tight range, more reactive (20-70°C, 5 steps)
- `tt_dark` - Moderate (20-70°C, 5 steps)

**Option 2: Adjust temperature thresholds**

Edit the temperature values in your active scheme in `~/.config/tt-qb-lights/config.toml`:

```toml
# Make colors change more quickly (compress temperature range)
[[color_mapping.schemes.custom]]
temp = 30  # Instead of 20 - start transitions later
color = "#4DB8A5"

[[color_mapping.schemes.custom]]
temp = 55  # Instead of 70 - reach red earlier
color = "#C23B3B"
```

### Adjusting Brightness Sensitivity

Control how much the lights dim when idle:

```toml
[effects]
# More dramatic: lights very dim when idle, bright under load
min_brightness = 0.1  # 10% brightness when idle
max_brightness = 1.0  # 100% brightness under load

# More subtle: lights always fairly bright
min_brightness = 0.6  # 60% brightness when idle
max_brightness = 1.0  # 100% brightness under load

# Constant brightness (disable power scaling)
enable_power_brightness = false
```

### Adjusting Warning Threshold

Control when the pulsing warning effect triggers:

```toml
[effects]
# More conservative (warn earlier)
warning_temp_threshold = 60  # Pulse at 60°C

# More relaxed (warn later)
warning_temp_threshold = 75  # Pulse at 75°C

# Disable warning pulse entirely
enable_warning_pulse = false
```

### Creating Custom Color Schemes

Add your own color scheme to `config.toml`:

```toml
# Your custom scheme
[[color_mapping.schemes.my_custom_scheme]]
temp = 25
color = "#YOUR_HEX_COLOR"

[[color_mapping.schemes.my_custom_scheme]]
temp = 45
color = "#YOUR_HEX_COLOR"

[[color_mapping.schemes.my_custom_scheme]]
temp = 65
color = "#YOUR_HEX_COLOR"

[color_mapping]
scheme = "my_custom_scheme"  # Activate your scheme
```

**Tips:**
- Use at least 3-4 temperature points for smooth gradients
- Ensure temperatures are in ascending order
- Colors are interpolated linearly between points
- Use [coolors.co](https://coolors.co) for palette inspiration

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
