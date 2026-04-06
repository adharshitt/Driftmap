#!/usr/bin/env bash
set -e

echo "========================================="
echo "🗺️  Installing DriftMap"
echo "========================================="

OS="$(uname -s)"
ARCH="$(uname -m)"

if [ "$OS" != "Linux" ]; then
    echo "❌ Error: DriftMap requires Linux (eBPF support)."
    exit 1
fi

if [ "$ARCH" = "x86_64" ]; then
    TARGET="x86_64-unknown-linux-gnu"
elif [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ]; then
    TARGET="aarch64-unknown-linux-gnu"
else
    echo "❌ Error: Unsupported architecture: $ARCH"
    exit 1
fi

echo "🔍 Fetching latest release version..."
LATEST_TAG=$(curl -s https://api.github.com/repos/adharshitt/Driftmap/releases/latest | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo "❌ Error: Could not determine latest release version."
    exit 1
fi

DOWNLOAD_URL="https://github.com/adharshitt/Driftmap/releases/download/${LATEST_TAG}/driftmap-${TARGET}"
INSTALL_DIR="/usr/local/bin"
BINARY_PATH="${INSTALL_DIR}/driftmap"

echo "⬇️  Downloading DriftMap ${LATEST_TAG} for ${TARGET}..."
curl -#fLo /tmp/driftmap "${DOWNLOAD_URL}"

echo "🔧 Installing to ${INSTALL_DIR} (requires sudo)..."
chmod +x /tmp/driftmap
sudo mv /tmp/driftmap "${BINARY_PATH}"

echo "✅ Successfully installed!"
echo ""
echo "Get started by running:"
echo "  driftmap init"
echo "  sudo driftmap watch --config driftmap.toml"
