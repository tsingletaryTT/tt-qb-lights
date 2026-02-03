#!/bin/bash
# tt-qb-lights Installation Script
# Checks prerequisites and installs missing dependencies for Ubuntu 24.04

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Track what needs to be installed
MISSING_PACKAGES=()
NEEDS_RUST=false
NEEDS_SENSOR_DETECT=false

# Check for --service-only flag (for systemd installation)
if [ "$1" == "--service-only" ]; then
    if [ "$EUID" -ne 0 ]; then
        echo -e "${RED}Error: --service-only requires root (use sudo)${NC}"
        exit 1
    fi

    echo -e "${BLUE}Installing systemd service...${NC}"
    cp tt-qb-lights.service /etc/systemd/system/
    systemctl daemon-reload
    echo -e "${GREEN}✓ Service installed${NC}"
    echo ""
    echo "To enable and start the service:"
    echo "  sudo systemctl enable tt-qb-lights"
    echo "  sudo systemctl start tt-qb-lights"
    exit 0
fi

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  tt-qb-lights Installation Script${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Check if running as root
if [ "$EUID" -eq 0 ]; then
    echo -e "${RED}Error: Do not run this script as root${NC}"
    echo "Run as normal user to check dependencies and build."
    echo "Use 'sudo ./install.sh --service-only' to install systemd service."
    exit 1
fi

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to check if a package is installed
package_installed() {
    dpkg -l "$1" 2>/dev/null | grep -q "^ii"
}

echo -e "${BLUE}[1/7]${NC} Checking prerequisites..."
echo ""

# Check for Rust
echo -n "  Checking Rust toolchain... "
if command_exists rustc && command_exists cargo; then
    RUST_VERSION=$(rustc --version | awk '{print $2}')
    echo -e "${GREEN}✓ Found ($RUST_VERSION)${NC}"
else
    echo -e "${YELLOW}✗ Not found${NC}"
    NEEDS_RUST=true
fi

# Check for OpenRGB
echo -n "  Checking OpenRGB... "
if command_exists openrgb; then
    echo -e "${GREEN}✓ Found${NC}"
else
    echo -e "${YELLOW}✗ Not found${NC}"
    MISSING_PACKAGES+=("openrgb")
fi

# Check for lm-sensors
echo -n "  Checking lm-sensors... "
if command_exists sensors; then
    echo -e "${GREEN}✓ Found${NC}"
else
    echo -e "${YELLOW}✗ Not found${NC}"
    MISSING_PACKAGES+=("lm-sensors")
fi

# Check for build-essential
echo -n "  Checking build-essential... "
if package_installed build-essential; then
    echo -e "${GREEN}✓ Found${NC}"
else
    echo -e "${YELLOW}✗ Not found${NC}"
    MISSING_PACKAGES+=("build-essential")
fi

# Check for pkg-config
echo -n "  Checking pkg-config... "
if command_exists pkg-config; then
    echo -e "${GREEN}✓ Found${NC}"
else
    echo -e "${YELLOW}✗ Not found${NC}"
    MISSING_PACKAGES+=("pkg-config")
fi

# Check if Tenstorrent devices are detected
echo -n "  Checking Tenstorrent devices... "
if sensors 2>/dev/null | grep -qi "blackhole\|wormhole\|grayskull"; then
    DEVICE_COUNT=$(sensors 2>/dev/null | grep -ci "blackhole\|wormhole\|grayskull")
    echo -e "${GREEN}✓ Found ($DEVICE_COUNT device(s))${NC}"
else
    echo -e "${YELLOW}⚠ Not detected${NC}"
    NEEDS_SENSOR_DETECT=true
fi

echo ""

# If everything is installed, skip to build
if [ "$NEEDS_RUST" = false ] && [ ${#MISSING_PACKAGES[@]} -eq 0 ]; then
    echo -e "${GREEN}✓ All prerequisites installed!${NC}"
    echo ""

    if [ "$NEEDS_SENSOR_DETECT" = true ]; then
        echo -e "${YELLOW}⚠ Tenstorrent devices not detected${NC}"
        echo ""
        read -p "Run sensor detection? [y/N] " -n 1 -r
        echo ""

        if [[ $REPLY =~ ^[Yy]$ ]]; then
            echo -e "${BLUE}Running sensors-detect...${NC}"
            sudo sensors-detect --auto
            echo ""

            # Verify detection
            if sensors 2>/dev/null | grep -qi "blackhole\|wormhole\|grayskull"; then
                echo -e "${GREEN}✓ Tenstorrent devices now detected!${NC}"
                sensors | grep -i "blackhole\|wormhole\|grayskull" -A 2
            else
                echo -e "${YELLOW}⚠ Still not detected. Check drivers with: lsmod | grep tenstorrent${NC}"
            fi
            echo ""
        fi
    fi

    # Skip to build
    BUILD_ONLY=true
else
    BUILD_ONLY=false
fi

# Installation phase (if needed)
if [ "$BUILD_ONLY" = false ]; then
    # Summary of what needs to be installed
    echo -e "${BLUE}[2/7]${NC} Installation Summary"
    echo ""

    if [ "$NEEDS_RUST" = true ]; then
        echo -e "  ${YELLOW}•${NC} Rust toolchain (via rustup)"
    fi

    for pkg in "${MISSING_PACKAGES[@]}"; do
        echo -e "  ${YELLOW}•${NC} $pkg"
    done

    echo ""

    # Ask for permission to proceed
    read -p "Install missing dependencies? [y/N] " -n 1 -r
    echo ""

    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo -e "${RED}Installation cancelled.${NC}"
        echo ""
        echo "You can install dependencies manually using the commands in README.md"
        exit 1
    fi

    # Update package list
    echo ""
    echo -e "${BLUE}[3/7]${NC} Updating package lists..."
    sudo apt update

    # Install missing packages
    if [ ${#MISSING_PACKAGES[@]} -gt 0 ]; then
        echo ""
        echo -e "${BLUE}[4/7]${NC} Installing packages: ${MISSING_PACKAGES[*]}"
        sudo apt install -y "${MISSING_PACKAGES[@]}"
        echo -e "${GREEN}✓ Packages installed${NC}"
    else
        echo ""
        echo -e "${BLUE}[4/7]${NC} No packages to install"
    fi

    # Install Rust if needed
    if [ "$NEEDS_RUST" = true ]; then
        echo ""
        echo -e "${BLUE}[5/7]${NC} Installing Rust toolchain"
        echo ""
        echo -e "${YELLOW}⚠ This will install Rust via rustup (the official installer)${NC}"
        echo -e "${YELLOW}  • Installs to ~/.cargo and ~/.rustup${NC}"
        echo -e "${YELLOW}  • Modifies your shell profile (~/.bashrc or ~/.zshrc)${NC}"
        echo -e "${YELLOW}  • ~200MB download${NC}"
        echo ""
        read -p "Proceed with Rust installation? [y/N] " -n 1 -r
        echo ""

        if [[ $REPLY =~ ^[Yy]$ ]]; then
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

            # Source cargo env for this script
            if [ -f "$HOME/.cargo/env" ]; then
                source "$HOME/.cargo/env"
            fi

            echo -e "${GREEN}✓ Rust installed${NC}"
            echo ""
            echo -e "${YELLOW}Note: You may need to restart your shell or run:${NC}"
            echo -e "  source \$HOME/.cargo/env"
        else
            echo -e "${YELLOW}⚠ Skipped Rust installation${NC}"
            echo ""
            echo "You'll need to install Rust manually before building:"
            echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
            exit 1
        fi
    else
        echo ""
        echo -e "${BLUE}[5/7]${NC} Rust already installed"
    fi

    # Run sensor detection if needed
    if [ "$NEEDS_SENSOR_DETECT" = true ]; then
        echo ""
        echo -e "${BLUE}[6/7]${NC} Detecting hardware sensors"
        echo ""

        # Check if devices appeared after package installation
        if sensors 2>/dev/null | grep -qi "blackhole\|wormhole\|grayskull"; then
            echo -e "${GREEN}✓ Tenstorrent devices now detected${NC}"
        else
            echo -e "${YELLOW}Running sensors-detect to find Tenstorrent devices...${NC}"
            sudo sensors-detect --auto

            # Verify detection
            echo ""
            echo "Checking for Tenstorrent devices:"
            if sensors 2>/dev/null | grep -qi "blackhole\|wormhole\|grayskull"; then
                echo -e "${GREEN}✓ Tenstorrent devices detected!${NC}"
                echo ""
                sensors | grep -i "blackhole\|wormhole\|grayskull" -A 2
            else
                echo -e "${RED}⚠ Tenstorrent devices not found${NC}"
                echo ""
                echo "Possible reasons:"
                echo "  • Tenstorrent drivers not loaded (check: lsmod | grep tenstorrent)"
                echo "  • Devices need power cycle (try: sudo tt-cold-reboot)"
                echo "  • Hardware not properly connected"
            fi
        fi
    else
        echo ""
        echo -e "${BLUE}[6/7]${NC} Sensor detection"
        echo -e "${GREEN}✓ Tenstorrent devices already detected${NC}"
    fi
fi

# Build phase
echo ""
echo -e "${BLUE}[7/7]${NC} Building tt-qb-lights..."
echo ""

if ! command_exists cargo; then
    echo -e "${RED}Error: cargo not found in PATH${NC}"
    echo "Please restart your shell or run: source \$HOME/.cargo/env"
    exit 1
fi

cargo build --release

if [ ! -f target/release/tt-qb-lights ]; then
    echo -e "${RED}Error: Build failed, binary not found${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}✓ Build successful!${NC}"

# Test hardware detection
echo ""
echo -e "${BLUE}Testing hardware detection...${NC}"
./target/release/tt-qb-lights --single-shot --config config.toml

# Initialize user configuration
echo ""
echo -e "${BLUE}Setting up configuration...${NC}"

CONFIG_DIR="$HOME/.config/tt-qb-lights"
CONFIG_FILE="$CONFIG_DIR/config.toml"

if [ -f "$CONFIG_FILE" ]; then
    echo -e "${YELLOW}Configuration already exists at: $CONFIG_FILE${NC}"
    echo "Skipping config initialization (keeping your existing settings)"
else
    # Create config directory
    mkdir -p "$CONFIG_DIR"

    # Copy default config
    cp config.toml "$CONFIG_FILE"

    echo -e "${GREEN}✓ Created configuration at: $CONFIG_FILE${NC}"
fi

# Final summary
echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${GREEN}Installation Complete!${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "Binary location: ./target/release/tt-qb-lights"
echo "Config location: $CONFIG_FILE"
echo ""
echo "Next steps:"
echo ""
echo "  1. Configure your RGB device name:"
echo "     nano $CONFIG_FILE"
echo ""
echo "  2. Start OpenRGB with SDK server enabled:"
echo "     openrgb --server"
echo ""
echo "  3. Test in dry-run mode (no RGB control):"
echo "     ./target/release/tt-qb-lights --dry-run --debug"
echo ""
echo "  4. Test with RGB control:"
echo "     ./target/release/tt-qb-lights"
echo ""
echo "  5. Change color schemes anytime by editing:"
echo "     nano $CONFIG_FILE"
echo "     (no need to rebuild or restart - just edit and restart the service)"
echo ""
echo "  6. Install as systemd service:"
echo "     sudo ./install.sh --service-only"
echo "     sudo systemctl enable tt-qb-lights"
echo "     sudo systemctl start tt-qb-lights"
echo ""
echo "See README.md for detailed documentation."
echo ""
