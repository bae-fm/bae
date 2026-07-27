#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

# This script compiles the bridge and then reads the resulting staticlib back
# out of the target dir to generate bindings from it, so it has to own the
# directory it reads from. An inherited CARGO_TARGET_DIR can be shared with
# other checkouts, and a concurrent build there rewrites the same artifact from
# different sources between the build and the read — the generators then emit
# bindings for a bridge nobody asked for, and it surfaces later as a Swift or
# C# compile error nowhere near its cause. Local and unconditional on purpose.
export CARGO_TARGET_DIR="target-linux"

usage() {
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for Linux. Debug by default."
    echo ""
    echo "  BAE_BRIDGE_TARGET selects the Rust target triple (default:"
    echo "  'x86_64-unknown-linux-gnu'; use 'aarch64-unknown-linux-gnu' for ARM64)."
    echo "  BAE_BRIDGE_FEATURES selects the cargo feature set (default:"
    echo "  'oauth-providers,desktop'). BAE_BRIDGE_CSHARP_BINDINGS_DIR is the"
    echo "  generated C# output directory."
    echo ""
    echo "  FFmpeg comes from bae-ffmpeg/dist (FFMPEG_DIR in .cargo/config.toml);"
    echo "  scripts/setup-ffmpeg.sh populates it with the matching linux dist"
    echo "  (ffmpeg-linux-{x86_64,aarch64}.tar.gz). A cross build sets FFMPEG_DIR"
    echo "  and BINDGEN_EXTRA_CLANG_ARGS to the target-arch dist."
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
# the bridge functions whose features are on, so each edition gets bindings from
# the matching feature set.
BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES-oauth-providers,desktop}"
if [[ -z "${BAE_BRIDGE_CSHARP_BINDINGS_DIR:-}" ]]; then
    echo "BAE_BRIDGE_CSHARP_BINDINGS_DIR is required" >&2
    exit 1
fi

# The Rust target triple. Defaults to x86_64; ARM64 CI sets aarch64. The host
# arch is irrelevant here — an arm runner builds the aarch64 target natively.
BAE_BRIDGE_TARGET="${BAE_BRIDGE_TARGET:-x86_64-unknown-linux-gnu}"

rustup target add "$BAE_BRIDGE_TARGET"

echo "Building bae-bridge for $BAE_BRIDGE_TARGET ($CARGO_PROFILE, features: ${BAE_BRIDGE_FEATURES:-(none)})..."
RUSTC_WRAPPER="" cargo build $CARGO_FLAGS \
    --target "$BAE_BRIDGE_TARGET" \
    -p bae-bridge \
    --features "$BAE_BRIDGE_FEATURES"

STATIC_LIB="$CARGO_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/libbae_bridge.a"
if [[ ! -f "$STATIC_LIB" ]]; then
    echo "Expected staticlib not found: $STATIC_LIB" >&2
    exit 1
fi

# The uniffi C# bindings DllImport("uniffi_bae_bridge"); .NET's native probing
# resolves that to libuniffi_bae_bridge.so on Linux, so the cdylib is copied to
# that name.
BRIDGE_SO="$CARGO_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/libbae_bridge.so"
UNIFFI_BRIDGE_SO="$CARGO_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/libuniffi_bae_bridge.so"
if [[ ! -f "$BRIDGE_SO" ]]; then
    echo "Expected shared library not found: $BRIDGE_SO" >&2
    exit 1
fi
cp "$BRIDGE_SO" "$UNIFFI_BRIDGE_SO"

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
echo "  $BRIDGE_SO"
echo "  $UNIFFI_BRIDGE_SO"
echo "  $BAE_BRIDGE_CSHARP_BINDINGS_DIR/"
