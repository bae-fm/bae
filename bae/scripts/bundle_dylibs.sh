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

resolve_rpath() {
    local dylib_path="$1"
    local rpath_ref="$2"
    
    # Extract rpath from the dylib
    local rpaths
    rpaths=$(otool -l "$dylib_path" 2>/dev/null | grep -A 2 "LC_RPATH" | grep "path" | awk '{print $2}' || true)
    
    # Get directory containing the dylib
    local dylib_dir
    dylib_dir=$(dirname "$dylib_path")
    
    # Extract library name from rpath reference (e.g., @rpath/libsharpyuv.0.dylib -> libsharpyuv.0.dylib)
    local lib_name
    lib_name=$(echo "$rpath_ref" | sed 's|@rpath/||')
    
    # Try resolving @rpath references
    # First, check if rpath is @loader_path/../lib (common pattern)
    # For /opt/homebrew/lib/libwebpmux.3.dylib, @loader_path is /opt/homebrew/lib
    # So @loader_path/../lib resolves to /opt/homebrew/lib (same directory)
    if echo "$rpaths" | grep -q "@loader_path/../lib"; then
        local resolved="$dylib_dir/$lib_name"
        if [[ -f "$resolved" ]]; then
            echo "$resolved"
            return
        fi
    fi
    
    # Try common Homebrew locations
    for base in "/opt/homebrew/lib" "/usr/local/lib"; do
        local candidate="$base/$lib_name"
        if [[ -f "$candidate" ]]; then
            echo "$candidate"
            return
        fi
    done
    
    # Try resolving each rpath
    for rpath in $rpaths; do
        local resolved_rpath
        if [[ "$rpath" == "@loader_path"* ]]; then
            # Replace @loader_path with actual directory
            resolved_rpath=$(echo "$rpath" | sed "s|@loader_path|$dylib_dir|")
        elif [[ "$rpath" == "@executable_path"* ]]; then
            # Skip executable path references (not applicable here)
            continue
        else
            resolved_rpath="$rpath"
        fi
        
        local candidate="$resolved_rpath/$lib_name"
        if [[ -f "$candidate" ]]; then
            echo "$candidate"
            return
        fi
    done
    
    # Last resort: try same directory as the dylib
    local same_dir="$dylib_dir/$lib_name"
    if [[ -f "$same_dir" ]]; then
        echo "$same_dir"
        return
    fi
    
    return 1
}

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
        local resolved_dep="$dep"
        
        # Resolve @rpath references
        if [[ "$dep" == "@rpath"* ]]; then
            local resolved
            if resolved=$(resolve_rpath "$real_path" "$dep"); then
                resolved_dep="$resolved"
            else
                echo "  Warning: Could not resolve $dep for $dylib_name"
                continue
            fi
        fi
        
        if [[ -n "$resolved_dep" && -f "$resolved_dep" ]]; then
            process_dylib "$resolved_dep"
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
    
    # Also fix any remaining @rpath references that point to bundled libraries
    local rpath_deps
    rpath_deps=$(otool -L "$bundled_path" | tail -n +2 | awk '{print $1}' | grep "^@rpath/" || true)
    for rpath_dep in $rpath_deps; do
        local lib_name
        lib_name=$(echo "$rpath_dep" | sed 's|@rpath/||')
        # Check if this library is already bundled
        if grep -q "|$lib_name$" "$DYLIB_LIST"; then
            install_name_tool -change \
                "$rpath_dep" \
                "@executable_path/../Frameworks/$lib_name" \
                "$bundled_path" 2>/dev/null || true
        fi
    done
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
