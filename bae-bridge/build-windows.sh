#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

# The bridge builds in target-windows, where this script reads the library back
# from to generate the bindings.
WINDOWS_TARGET_DIR="target-windows"

usage() {
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for Windows. Debug by default."
    echo ""
    echo "  BAE_BRIDGE_TARGET selects the Rust target triple (default:"
    echo "  'x86_64-pc-windows-msvc'; use 'aarch64-pc-windows-msvc' for ARM64)."
    echo "  BAE_BRIDGE_FEATURES selects the cargo feature set (default:"
    echo "  'oauth-providers,cast'). BAE_BRIDGE_CSHARP_BINDINGS_DIR is the"
    echo "  generated C# output directory. A debug build adds 'test-keyring', the"
    echo "  in-memory keychain the Avalonia smoke test starts its host with."
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

BINDGEN="$(scripts/csharp-bindgen.sh --path)"

# The cargo feature set the bridge compiles with. The generated C# only exports
# the bridge functions whose features are on; Windows ships the one full
# edition (OAuth + desktop).
BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES-oauth-providers,cast}"
# The smoke test starts its host with an in-memory keychain, so it never
# touches the person's keychain; a release build has none.
if [[ "$CARGO_PROFILE" == "debug" ]]; then
    case ",$BAE_BRIDGE_FEATURES," in
        *,test-keyring,*) ;;
        *) BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES:+$BAE_BRIDGE_FEATURES,}test-keyring" ;;
    esac
fi
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
# `cargo rustc --crate-type cdylib` builds only the DLL. The crate also lists
# `staticlib` for Apple, whose builds link the .a into the framework — but on
# Windows nothing consumes the .lib, and a debug archive carries embedded
# CodeView for the whole dependency graph (5.8 GB at last measure), which
# uniffi-bindgen-cs cannot even read. The DLL keeps its debug info in the
# adjacent PDB, so bindgen reads it and WinDbg still symbolizes.
#
# .cargo/config.toml's plain sccache wrapper can't build here: ffmpeg-next's
# command line only fits Windows' 32K limit in an argfile, which sccache expands
# again (os error 206). A caller's own wrapper is kept (CI passes one that
# leaves argfile compiles to rustc); with none, the build is uncached.
RUSTC_WRAPPER="${RUSTC_WRAPPER-}" ./scripts/cargo.sh rustc $CARGO_FLAGS \
    --config 'profile.release.lto="thin"' \
    --target "$BAE_BRIDGE_TARGET" \
    --target-dir "$WINDOWS_TARGET_DIR" \
    -p bae-bridge \
    --lib \
    --features "$BAE_BRIDGE_FEATURES" \
    --crate-type cdylib

BRIDGE_DLL="$WINDOWS_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/bae_bridge.dll"
UNIFFI_BRIDGE_DLL="$WINDOWS_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/uniffi_bae_bridge.dll"
if [[ ! -f "$BRIDGE_DLL" ]]; then
    echo "Expected DLL not found: $BRIDGE_DLL" >&2
    exit 1
fi
cp "$BRIDGE_DLL" "$UNIFFI_BRIDGE_DLL"

echo "Generating C# bindings into $BAE_BRIDGE_CSHARP_BINDINGS_DIR ..."
rm -rf "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
mkdir -p "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
"$BINDGEN" \
    --library "$BRIDGE_DLL" \
    --crate bae_bridge \
    --out-dir "$BAE_BRIDGE_CSHARP_BINDINGS_DIR/" \
    --no-format

echo ""
echo "Done ($CARGO_PROFILE). Outputs:"
echo "  $BRIDGE_DLL"
echo "  $UNIFFI_BRIDGE_DLL"
echo "  $BAE_BRIDGE_CSHARP_BINDINGS_DIR/"
