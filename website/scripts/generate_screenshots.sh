#!/bin/bash
# Generate screenshots for the website
# This script builds the bae app and generates screenshots

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WEBSITE_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(dirname "$WEBSITE_DIR")"
BAE_DIR="$REPO_ROOT/bae"

echo "Generating screenshots..."
echo "Website directory: $WEBSITE_DIR"
echo "Bae directory: $BAE_DIR"

# Build the bae app if needed
cd "$BAE_DIR"

if [ ! -f "target/release/bae" ] && [ ! -f "target/dx/bae/bundle/macos/bundle/macos/Bae.app/Contents/MacOS/Bae" ]; then
    echo "Building bae app..."
    cargo build --release --bin bae
fi

# Set environment variables for screenshot generation
export BAE_SCREENSHOT_FIXTURES_DIR="$WEBSITE_DIR/fixtures/screenshots"
export BAE_SCREENSHOT_OUTPUT_DIR="$WEBSITE_DIR/public/screenshots"

# Generate screenshots using the existing binary
echo "Running screenshot generation..."
echo "Fixtures: $BAE_SCREENSHOT_FIXTURES_DIR"
echo "Output: $BAE_SCREENSHOT_OUTPUT_DIR"
cargo run --release --bin generate_screenshots

if [ -d "$BAE_SCREENSHOT_OUTPUT_DIR" ] && [ "$(ls -A $BAE_SCREENSHOT_OUTPUT_DIR)" ]; then
    echo "Screenshots generated successfully in $BAE_SCREENSHOT_OUTPUT_DIR"
else
    echo "Warning: Screenshot directory not found or empty at $BAE_SCREENSHOT_OUTPUT_DIR"
    exit 1
fi

echo "Done!"
