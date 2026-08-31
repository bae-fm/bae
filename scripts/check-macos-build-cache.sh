#!/bin/bash
set -euo pipefail

BUILD_SCRIPT="bae-bridge/build-macos.sh"

required_fragments=(
    'if [[ "$RUST_HOST" == "$MACOS_TARGET" ]]'
    'CARGO_ARTIFACT_DIR="$CARGO_TARGET_DIR/$CARGO_PROFILE"'
    'CARGO_ARTIFACT_DIR="$CARGO_TARGET_DIR/$MACOS_TARGET/$CARGO_PROFILE"'
    'BINDGEN="$CARGO_TARGET_DIR/$CARGO_PROFILE/uniffi-bindgen"'
    '-p bae-bridge --lib --features "$BAE_BRIDGE_FEATURES"'
    '-p bae-uniffi-bindgen'
)

for fragment in "${required_fragments[@]}"; do
    if ! grep -Fq -- "$fragment" "$BUILD_SCRIPT"; then
        echo "macOS bridge target selection is missing: $fragment" >&2
        exit 1
    fi
done

if grep -F -A1 -- '-p bae-bridge --lib' "$BUILD_SCRIPT" \
    | grep -Fq -- '-p bae-uniffi-bindgen'; then
    echo "macOS bridge --lib selection also captures the binary-only generator package" >&2
    exit 1
fi

echo "macOS bridge build reuses Cargo's native host cache"
