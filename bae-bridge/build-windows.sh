#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-windows}"

usage() {
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for Windows x86_64. Debug by default."
    echo ""
    echo "  BAE_BRIDGE_FEATURES selects the cargo feature set (default:"
    echo "  'oauth-providers'). BAE_BRIDGE_CSHARP_BINDINGS_DIR is the"
    echo "  generated C# output directory."
}

CARGO_PROFILE="debug"
CARGO_FLAGS=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        --release) CARGO_PROFILE="release"; CARGO_FLAGS="--release"; shift ;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 1 ;;
    esac
done

if ! command -v uniffi-bindgen-cs >/dev/null 2>&1; then
    echo "uniffi-bindgen-cs is required. Install the pinned generator:" >&2
    echo "cargo install --git https://github.com/NordSecurity/uniffi-bindgen-cs.git --tag 'v0.11.0+v0.31.0' uniffi-bindgen-cs --locked" >&2
    exit 1
fi

# The cargo feature set the bridge compiles with. The generated C# only exports
# the bridge functions whose features are on, so each Windows edition gets
# bindings from the matching feature set.
BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES-oauth-providers}"
if [[ -z "${BAE_BRIDGE_CSHARP_BINDINGS_DIR:-}" ]]; then
    echo "BAE_BRIDGE_CSHARP_BINDINGS_DIR is required" >&2
    exit 1
fi

rustup target add x86_64-pc-windows-msvc

echo "Building bae-bridge for Windows x86_64 ($CARGO_PROFILE, features: ${BAE_BRIDGE_FEATURES:-(none)})..."
RUSTC_WRAPPER="" cargo build $CARGO_FLAGS \
    --target x86_64-pc-windows-msvc \
    -p bae-bridge \
    --features "$BAE_BRIDGE_FEATURES"

STATIC_LIB="$CARGO_TARGET_DIR/x86_64-pc-windows-msvc/$CARGO_PROFILE/bae_bridge.lib"
if [[ ! -f "$STATIC_LIB" ]]; then
    echo "Expected staticlib not found: $STATIC_LIB" >&2
    exit 1
fi

echo "Generating C# bindings into $BAE_BRIDGE_CSHARP_BINDINGS_DIR ..."
rm -rf "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
mkdir -p "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
uniffi-bindgen-cs \
    --library "$STATIC_LIB" \
    --crate bae_bridge \
    --out-dir "$BAE_BRIDGE_CSHARP_BINDINGS_DIR/" \
    --no-format

echo ""
echo "Done ($CARGO_PROFILE). Outputs:"
echo "  $CARGO_TARGET_DIR/x86_64-pc-windows-msvc/$CARGO_PROFILE/bae_bridge.dll"
echo "  $BAE_BRIDGE_CSHARP_BINDINGS_DIR/"
