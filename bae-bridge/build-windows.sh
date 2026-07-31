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
export CARGO_TARGET_DIR="target-windows"

usage() {
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for Windows. Debug by default."
    echo ""
    echo "  BAE_BRIDGE_TARGET selects the Rust target triple (default:"
    echo "  'x86_64-pc-windows-msvc'; use 'aarch64-pc-windows-msvc' for ARM64)."
    echo "  BAE_BRIDGE_FEATURES selects the cargo feature set (default:"
    echo "  'oauth-providers,desktop'). BAE_BRIDGE_CSHARP_BINDINGS_DIR is the"
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
BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES-oauth-providers,desktop}"
if [[ -z "${BAE_BRIDGE_CSHARP_BINDINGS_DIR:-}" ]]; then
    echo "BAE_BRIDGE_CSHARP_BINDINGS_DIR is required" >&2
    exit 1
fi

# The Rust target triple. Defaults to x86_64; ARM64 CI sets aarch64. The host
# arch is irrelevant here — an arm runner builds the aarch64 target natively.
BAE_BRIDGE_TARGET="${BAE_BRIDGE_TARGET:-x86_64-pc-windows-msvc}"

rustup target add "$BAE_BRIDGE_TARGET"

echo "Building bae-bridge for $BAE_BRIDGE_TARGET ($CARGO_PROFILE, features: ${BAE_BRIDGE_FEATURES:-(none)})..."
# Fat LTO crashes rustc 1.95's codegen on the aarch64 Windows toolchain
# (STATUS_STACK_BUFFER_OVERRUN linking the release cdylib). Thin LTO links the
# same cdylib fine; the other platforms keep the workspace profile's fat LTO.
export CARGO_PROFILE_RELEASE_LTO=thin
# `cargo rustc --crate-type cdylib` builds only the DLL. The crate also lists
# `staticlib` for Apple, whose builds link the .a into the framework — but on
# Windows nothing consumes the .lib, and a debug archive carries embedded
# CodeView for the whole dependency graph (5.8 GB at last measure), which
# uniffi-bindgen-cs cannot even read. The DLL keeps its debug info in the
# adjacent PDB, so bindgen reads it and WinDbg still symbolizes.
RUSTC_WRAPPER="" cargo rustc $CARGO_FLAGS \
    --target "$BAE_BRIDGE_TARGET" \
    -p bae-bridge \
    --lib \
    --features "$BAE_BRIDGE_FEATURES" \
    --crate-type cdylib

BRIDGE_DLL="$CARGO_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/bae_bridge.dll"
UNIFFI_BRIDGE_DLL="$CARGO_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/uniffi_bae_bridge.dll"
if [[ ! -f "$BRIDGE_DLL" ]]; then
    echo "Expected DLL not found: $BRIDGE_DLL" >&2
    exit 1
fi
cp "$BRIDGE_DLL" "$UNIFFI_BRIDGE_DLL"

echo "Generating C# bindings into $BAE_BRIDGE_CSHARP_BINDINGS_DIR ..."
rm -rf "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
mkdir -p "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
uniffi-bindgen-cs \
    --library "$BRIDGE_DLL" \
    --crate bae_bridge \
    --out-dir "$BAE_BRIDGE_CSHARP_BINDINGS_DIR/" \
    --no-format

echo ""
echo "Done ($CARGO_PROFILE). Outputs:"
echo "  $BRIDGE_DLL"
echo "  $UNIFFI_BRIDGE_DLL"
echo "  $BAE_BRIDGE_CSHARP_BINDINGS_DIR/"
