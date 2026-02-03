#!/bin/bash
# Installation script for tt-qb-lights

set -e

echo "===================================="
echo "  tt-qb-lights Installation Script"
echo "===================================="
echo ""

# Check if running as root for systemd install
if [ "$EUID" -eq 0 ] && [ "$1" != "--service-only" ]; then
    echo "Error: Do not run this script as root (except with --service-only)"
    echo "Run as normal user to build, then run with sudo and --service-only to install service"
    exit 1
fi

if [ "$1" == "--service-only" ]; then
    if [ "$EUID" -ne 0 ]; then
        echo "Error: --service-only requires root (use sudo)"
        exit 1
    fi

    echo "Installing systemd service..."
    cp tt-qb-lights.service /etc/systemd/system/
    systemctl daemon-reload
    echo "Service installed. To enable and start:"
    echo "  sudo systemctl enable tt-qb-lights"
    echo "  sudo systemctl start tt-qb-lights"
    exit 0
fi

# Build the project
echo "Step 1: Building tt-qb-lights (release mode)..."
cargo build --release

if [ ! -f target/release/tt-qb-lights ]; then
    echo "Error: Build failed, binary not found"
    exit 1
fi

echo ""
echo "Step 2: Testing hardware detection..."
./target/release/tt-qb-lights --single-shot

echo ""
echo "Step 3: Checking configuration..."
if [ ! -f config.toml ]; then
    echo "Error: config.toml not found"
    exit 1
fi
echo "Configuration file: config.toml"
echo "  Device: $(grep device_name config.toml | head -1)"
echo "  Scheme: $(grep 'scheme =' config.toml | head -1)"

echo ""
echo "===================================="
echo "  Build Complete!"
echo "===================================="
echo ""
echo "Binary location: ./target/release/tt-qb-lights"
echo ""
echo "Next steps:"
echo ""
echo "1. Test in dry-run mode (no RGB control):"
echo "   ./target/release/tt-qb-lights --dry-run --debug"
echo ""
echo "2. Make sure OpenRGB is running with SDK server:"
echo "   openrgb --server"
echo ""
echo "3. Test with actual RGB control:"
echo "   ./target/release/tt-qb-lights"
echo ""
echo "4. Install as systemd service:"
echo "   sudo ./install.sh --service-only"
echo "   sudo systemctl enable tt-qb-lights"
echo "   sudo systemctl start tt-qb-lights"
echo ""
