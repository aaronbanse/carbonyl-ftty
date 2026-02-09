#!/usr/bin/env bash
set -euo pipefail

CARBONYL_VERSION="v0.0.3"
FIDELITTY_VERSION="0.1.0"
INSTALL_DIR="/usr/local/lib/carbonyl"
BIN_LINK="/usr/local/bin/carbonyl"
REPO="fathyb/carbonyl"

# Resolve project root
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
PROJECT_ROOT=$(dirname "$SCRIPT_DIR")

# Detect platform and architecture
case "$(uname -s)" in
    Linux)  platform="linux" ;;
    Darwin) platform="macos" ;;
    *)      echo "Unsupported OS: $(uname -s)"; exit 1 ;;
esac

case "$(uname -m)" in
    x86_64)  arch="amd64" ;;
    aarch64|arm64) arch="arm64" ;;
    *)       echo "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac

asset="carbonyl.${platform}-${arch}.zip"
echo "Platform: ${platform}/${arch}"

# Find our local libcarbonyl
if [ -f "$PROJECT_ROOT/build/release/libcarbonyl.so" ]; then
    local_lib="$PROJECT_ROOT/build/release/libcarbonyl.so"
elif [ -f "$PROJECT_ROOT/build/debug/libcarbonyl.so" ]; then
    local_lib="$PROJECT_ROOT/build/debug/libcarbonyl.so"
    echo "Warning: using debug build of libcarbonyl.so"
else
    echo "No local libcarbonyl.so found. Build first with: cargo build --release"
    exit 1
fi
echo "Using libcarbonyl: $local_lib"

# Find libfidelitty
fidelitty_lib_dir="${FIDELITTY_LIB_DIR:-/usr/local/lib}"
fidelitty_lib="$fidelitty_lib_dir/libfidelitty.so.${FIDELITTY_VERSION}"
if [ ! -f "$fidelitty_lib" ]; then
    echo "libfidelitty.so.${FIDELITTY_VERSION} not found in $fidelitty_lib_dir"
    echo "Install fidelitty ${FIDELITTY_VERSION} first, or set FIDELITTY_LIB_DIR"
    exit 1
fi
echo "Using libfidelitty: $fidelitty_lib"

# Download upstream carbonyl release
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

echo "Downloading ${asset} from ${REPO} ${CARBONYL_VERSION}..."
gh release download "$CARBONYL_VERSION" \
    --repo "$REPO" \
    --pattern "$asset" \
    --dir "$tmpdir"

echo "Extracting..."
unzip -q "$tmpdir/$asset" -d "$tmpdir"

# The zip contains a carbonyl-{version}/ directory
src_dir=$(find "$tmpdir" -maxdepth 1 -type d -name 'carbonyl-*' | head -1)
if [ -z "$src_dir" ]; then
    # Fallback: files might be at top level
    src_dir="$tmpdir"
fi

# Install
echo "Installing to $INSTALL_DIR..."
sudo rm -rf "$INSTALL_DIR"
sudo mkdir -p "$INSTALL_DIR"

# Copy upstream release files
sudo cp -a "$src_dir"/* "$INSTALL_DIR/"

# Swap in our libcarbonyl
sudo cp "$local_lib" "$INSTALL_DIR/libcarbonyl.so"

# Bundle libfidelitty
sudo cp "$fidelitty_lib" "$INSTALL_DIR/libfidelitty.so.${FIDELITTY_VERSION}"
sudo ln -sf "libfidelitty.so.${FIDELITTY_VERSION}" "$INSTALL_DIR/libfidelitty.so.0"
sudo ln -sf "libfidelitty.so.0" "$INSTALL_DIR/libfidelitty.so"

# Create symlink on PATH
sudo ln -sf "$INSTALL_DIR/carbonyl" "$BIN_LINK"

echo ""
echo "Installed carbonyl to $INSTALL_DIR"
echo "  upstream: ${REPO} ${CARBONYL_VERSION}"
echo "  libcarbonyl: $local_lib"
echo "  libfidelitty: ${FIDELITTY_VERSION}"
echo "  binary: $BIN_LINK -> $INSTALL_DIR/carbonyl"
