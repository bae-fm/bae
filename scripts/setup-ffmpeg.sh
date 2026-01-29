#!/bin/bash
set -euo pipefail

# Download or build bae-ffmpeg for local development
# Usage: ./scripts/setup-ffmpeg.sh [--build]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
FFMPEG_DIR="$PROJECT_ROOT/bae-ffmpeg/dist"
VERSION="v8.0.1-bae6"

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

if [[ "$OS" == "darwin" ]]; then
    PLATFORM="macos"
    ARCHIVE="ffmpeg-macos-$ARCH.tar.gz"
elif [[ "$OS" == "linux" ]]; then
    PLATFORM="linux"
    ARCHIVE="ffmpeg-linux-$ARCH.tar.gz"
else
    echo "Unsupported OS: $OS"
    exit 1
fi

mkdir -p "$FFMPEG_DIR"

if [[ "${1:-}" == "--build" ]]; then
    echo "Building bae-ffmpeg locally..."
    cd "$PROJECT_ROOT/bae-ffmpeg"
    "./build-$PLATFORM.sh" "$ARCH"

    # Extract to dist
    tar xzf "dist/$ARCHIVE" -C "$FFMPEG_DIR"
else
    echo "Downloading bae-ffmpeg $VERSION for $PLATFORM-$ARCH..."
    curl -L "https://github.com/bae-fm/bae-ffmpeg/releases/download/$VERSION/$ARCHIVE" | \
        tar xz -C "$FFMPEG_DIR"
fi

echo ""
echo "bae-ffmpeg installed to: $FFMPEG_DIR"
echo ""
echo "Add to your shell profile (.zshrc or .bashrc):"
echo ""
echo "  export FFMPEG_DIR=\"$FFMPEG_DIR\""
echo "  export PKG_CONFIG_PATH=\"$FFMPEG_DIR/lib/pkgconfig:\$PKG_CONFIG_PATH\""
echo "  export LIBRARY_PATH=\"$FFMPEG_DIR/lib:\$LIBRARY_PATH\""
echo "  export DYLD_LIBRARY_PATH=\"$FFMPEG_DIR/lib:\$DYLD_LIBRARY_PATH\""
echo ""
