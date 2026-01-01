#!/bin/bash
set -euo pipefail

# Bundle Homebrew dylibs into macOS app and fix paths
# Run after: dx bundle --release

APP_PATH="${1:-target/dx/bae/bundle/macos/bundle/macos/Bae.app}"
FRAMEWORKS_DIR="$APP_PATH/Contents/Frameworks"
BINARY="$APP_PATH/Contents/MacOS/bae"

if [[ ! -f "$BINARY" ]]; then
    echo "Error: Binary not found at $BINARY"
    exit 1
fi

mkdir -p "$FRAMEWORKS_DIR"

echo "Scanning binary for non-system dylibs..."

# Get all non-system dylibs from binary
DYLIB_PATHS=$(otool -L "$BINARY" | grep -v "^/" | grep -v "/System" | grep -v "/usr/lib" | awk '{print $1}')

declare -A PROCESSED
declare -A DYLIB_MAP  # original_path -> bundled_name

process_dylib() {
    local dylib_path="$1"
    local dylib_name
    dylib_name=$(basename "$dylib_path")
    
    # Skip if already processed
    if [[ -n "${PROCESSED[$dylib_path]:-}" ]]; then
        return
    fi
    PROCESSED[$dylib_path]=1
    
    # Resolve symlinks
    local real_path
    real_path=$(realpath "$dylib_path")
    
    echo "  Processing: $dylib_name"
    
    # Copy to Frameworks
    cp "$real_path" "$FRAMEWORKS_DIR/$dylib_name"
    chmod +w "$FRAMEWORKS_DIR/$dylib_name"
    DYLIB_MAP[$dylib_path]="$dylib_name"
    
    # Recursively process this dylib's non-system dependencies
    local deps
    deps=$(otool -L "$real_path" | grep -v "^/" | grep -v "/System" | grep -v "/usr/lib" | grep -v "$dylib_name" | awk '{print $1}') || true
    
    for dep in $deps; do
        if [[ -n "$dep" ]]; then
            process_dylib "$dep"
        fi
    done
}

# Process all dylibs recursively
for dylib in $DYLIB_PATHS; do
    process_dylib "$dylib"
done

echo ""
echo "Fixing paths in binary..."

# Fix all dylib references in main binary
for original_path in "${!DYLIB_MAP[@]}"; do
    bundled_name="${DYLIB_MAP[$original_path]}"
    install_name_tool -change \
        "$original_path" \
        "@executable_path/../Frameworks/$bundled_name" \
        "$BINARY"
done

echo "Fixing paths in bundled dylibs..."

# Fix paths in each bundled dylib
for bundled_name in "${DYLIB_MAP[@]}"; do
    bundled_path="$FRAMEWORKS_DIR/$bundled_name"
    
    # Set the dylib's own id
    install_name_tool -id "@executable_path/../Frameworks/$bundled_name" "$bundled_path"
    
    # Fix references to other bundled dylibs
    for original_path in "${!DYLIB_MAP[@]}"; do
        dep_name="${DYLIB_MAP[$original_path]}"
        install_name_tool -change \
            "$original_path" \
            "@executable_path/../Frameworks/$dep_name" \
            "$bundled_path" 2>/dev/null || true
    done
done

echo ""
echo "Bundled dylibs:"
ls -la "$FRAMEWORKS_DIR"

echo ""
echo "Verifying no unbundled dylibs remain..."

# Check binary
REMAINING=$(otool -L "$BINARY" | grep -E "/opt/homebrew|/usr/local/Cellar" || true)
if [[ -n "$REMAINING" ]]; then
    echo "ERROR: Binary still references unbundled dylibs:"
    echo "$REMAINING"
    exit 1
fi

# Check all bundled dylibs
for bundled_name in "${DYLIB_MAP[@]}"; do
    REMAINING=$(otool -L "$FRAMEWORKS_DIR/$bundled_name" | grep -E "/opt/homebrew|/usr/local/Cellar" || true)
    if [[ -n "$REMAINING" ]]; then
        echo "ERROR: $bundled_name still references unbundled dylibs:"
        echo "$REMAINING"
        exit 1
    fi
done

echo "✓ All dylibs properly bundled"
echo ""
echo "Bundled ${#DYLIB_MAP[@]} dylibs:"
ls "$FRAMEWORKS_DIR" | sed 's/^/  /'
