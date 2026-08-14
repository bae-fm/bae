#!/bin/bash
set -euo pipefail

BUILD_SCRIPT="bae-bridge/build-macos.sh"

required_fragments=(
    'if [[ "$RUST_HOST" == "$MACOS_TARGET" ]]'
    'CARGO_ARTIFACT_DIR="$CARGO_TARGET_DIR/$CARGO_PROFILE"'
    'CARGO_ARTIFACT_DIR="$CARGO_TARGET_DIR/$MACOS_TARGET/$CARGO_PROFILE"'
    'BINDGEN="$CARGO_TARGET_DIR/$CARGO_PROFILE/uniffi-bindgen"'
    '--lib --bin uniffi-bindgen --features "$BAE_BRIDGE_FEATURES"'
)

for fragment in "${required_fragments[@]}"; do
    if ! grep -Fq -- "$fragment" "$BUILD_SCRIPT"; then
        echo "macOS bridge target selection is missing: $fragment" >&2
        exit 1
    fi
done

echo "macOS bridge build reuses Cargo's native host cache"
