#!/bin/bash
set -euo pipefail

# Download and bundle Sparkle.framework into macOS app
# Run after: dx bundle --release (called from bundle_dylibs.sh)

SPARKLE_VERSION="2.8.1"
SPARKLE_URL="https://github.com/sparkle-project/Sparkle/releases/download/${SPARKLE_VERSION}/Sparkle-${SPARKLE_VERSION}.tar.xz"

APP_PATH="${1:-target/dx/bae/bundle/macos/bundle/macos/bae.app}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CACHE_DIR="$SCRIPT_DIR/../.sparkle-cache"
FRAMEWORKS_DIR="$APP_PATH/Contents/Frameworks"

if [[ ! -d "$APP_PATH" ]]; then
    echo "Error: App bundle not found at $APP_PATH"
    exit 1
fi

mkdir -p "$FRAMEWORKS_DIR"
mkdir -p "$CACHE_DIR"

# Download Sparkle if not cached
SPARKLE_ARCHIVE="$CACHE_DIR/Sparkle-${SPARKLE_VERSION}.tar.xz"
SPARKLE_DIR="$CACHE_DIR/Sparkle-${SPARKLE_VERSION}"

if [[ ! -d "$SPARKLE_DIR" ]]; then
    echo "Downloading Sparkle ${SPARKLE_VERSION}..."
    curl -L -o "$SPARKLE_ARCHIVE" "$SPARKLE_URL"
    
    echo "Extracting Sparkle..."
    mkdir -p "$SPARKLE_DIR"
    tar -xf "$SPARKLE_ARCHIVE" -C "$SPARKLE_DIR"
    rm "$SPARKLE_ARCHIVE"
fi

# Copy Sparkle.framework to app bundle
echo "Bundling Sparkle.framework..."
cp -R "$SPARKLE_DIR/Sparkle.framework" "$FRAMEWORKS_DIR/"

# Sparkle 2 requires the XPC services for sandboxed apps
# For non-sandboxed apps, we just need the main framework
# Remove XPC services if present (we're not sandboxed)
rm -rf "$FRAMEWORKS_DIR/Sparkle.framework/XPCServices" 2>/dev/null || true

echo "✓ Sparkle.framework bundled"
