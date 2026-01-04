#!/bin/bash
set -euo pipefail

# Bundle Homebrew dylibs into macOS app, fix paths, and merge Info.plist
# Run after: dx bundle --release

APP_PATH="${1:-target/dx/bae/bundle/macos/bundle/macos/bae.app}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
FRAMEWORKS_DIR="$APP_PATH/Contents/Frameworks"
BINARY="$APP_PATH/Contents/MacOS/bae"

if [[ ! -f "$BINARY" ]]; then
    echo "Error: Binary not found at $BINARY"
    exit 1
fi

mkdir -p "$FRAMEWORKS_DIR"

echo "Scanning binary for non-system dylibs..."

# Temp files to track state (bash 3.2 compatible - no associative arrays)
PROCESSED_FILE=$(mktemp)
DYLIB_LIST=$(mktemp)
trap "rm -f $PROCESSED_FILE $DYLIB_LIST" EXIT

process_dylib() {
    local dylib_path="$1"
    local dylib_name
    dylib_name=$(basename "$dylib_path")
    
    # Skip if already processed
    if grep -qxF "$dylib_path" "$PROCESSED_FILE" 2>/dev/null; then
        return
    fi
    echo "$dylib_path" >> "$PROCESSED_FILE"
    
    # Resolve symlinks
    local real_path
    real_path=$(realpath "$dylib_path")
    
    echo "  Processing: $dylib_name"
    
    # Copy to Frameworks
    cp "$real_path" "$FRAMEWORKS_DIR/$dylib_name"
    chmod +w "$FRAMEWORKS_DIR/$dylib_name"
    
    # Record mapping: original_path|bundled_name
    echo "$dylib_path|$dylib_name" >> "$DYLIB_LIST"
    
    # Recursively process this dylib's non-system dependencies
    local deps
    deps=$(otool -L "$real_path" | tail -n +2 | awk '{print $1}' | grep -v "/System" | grep -v "/usr/lib" | grep -v "$dylib_name") || true
    
    for dep in $deps; do
        if [[ -n "$dep" && -f "$dep" ]]; then
            process_dylib "$dep"
        fi
    done
}

# Get all non-system dylibs from binary
DYLIB_PATHS=$(otool -L "$BINARY" | tail -n +2 | awk '{print $1}' | grep -v "/System" | grep -v "/usr/lib") || true

# Process all dylibs recursively
for dylib in $DYLIB_PATHS; do
    if [[ -f "$dylib" ]]; then
        process_dylib "$dylib"
    fi
done

echo ""
echo "Fixing paths in binary..."

# Fix all dylib references in main binary
while IFS='|' read -r original_path bundled_name; do
    install_name_tool -change \
        "$original_path" \
        "@executable_path/../Frameworks/$bundled_name" \
        "$BINARY"
done < "$DYLIB_LIST"

echo "Fixing paths in bundled dylibs..."

# Fix paths in each bundled dylib
while IFS='|' read -r _ bundled_name; do
    bundled_path="$FRAMEWORKS_DIR/$bundled_name"
    
    # Set the dylib's own id
    install_name_tool -id "@executable_path/../Frameworks/$bundled_name" "$bundled_path"
    
    # Fix references to other bundled dylibs
    while IFS='|' read -r orig dep_name; do
        install_name_tool -change \
            "$orig" \
            "@executable_path/../Frameworks/$dep_name" \
            "$bundled_path" 2>/dev/null || true
    done < "$DYLIB_LIST"
done < "$DYLIB_LIST"

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
while IFS='|' read -r _ bundled_name; do
    REMAINING=$(otool -L "$FRAMEWORKS_DIR/$bundled_name" | grep -E "/opt/homebrew|/usr/local/Cellar" || true)
    if [[ -n "$REMAINING" ]]; then
        echo "ERROR: $bundled_name still references unbundled dylibs:"
        echo "$REMAINING"
        exit 1
    fi
done < "$DYLIB_LIST"

echo "✓ All dylibs properly bundled"
echo ""
echo "Bundled dylibs:"
ls "$FRAMEWORKS_DIR" | sed 's/^/  /'

# Merge custom Info.plist entries
INFO_PLIST="$APP_PATH/Contents/Info.plist"
CUSTOM_PLIST="$PROJECT_ROOT/Info.plist"

if [[ -f "$CUSTOM_PLIST" ]]; then
    echo ""
    echo "Merging custom Info.plist entries..."
    
    # Extract keys from custom plist and add them to the bundle's plist
    # Using PlistBuddy to add NSLocalNetworkUsageDescription
    if /usr/libexec/PlistBuddy -c "Print :NSLocalNetworkUsageDescription" "$CUSTOM_PLIST" &>/dev/null; then
        VALUE=$(/usr/libexec/PlistBuddy -c "Print :NSLocalNetworkUsageDescription" "$CUSTOM_PLIST")
        /usr/libexec/PlistBuddy -c "Delete :NSLocalNetworkUsageDescription" "$INFO_PLIST" 2>/dev/null || true
        /usr/libexec/PlistBuddy -c "Add :NSLocalNetworkUsageDescription string '$VALUE'" "$INFO_PLIST"
        echo "  ✓ Added NSLocalNetworkUsageDescription"
    fi
    
    echo "✓ Info.plist merged"
fi
