#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

# The bridge builds in target-linux, where this script reads the library back
# from to generate the bindings.
LINUX_TARGET_DIR="target-linux"

usage() {
    echo "Usage: $0 [--release]"
    echo "  Builds bae-bridge for Linux. Debug by default."
    echo ""
    echo "  BAE_BRIDGE_TARGET selects the Rust target triple (default:"
    echo "  'x86_64-unknown-linux-gnu'; use 'aarch64-unknown-linux-gnu' for ARM64)."
    echo "  BAE_BRIDGE_FEATURES selects the cargo feature set (default:"
    echo "  'oauth-providers,cast'). BAE_BRIDGE_CSHARP_BINDINGS_DIR is the"
    echo "  generated C# output directory. A debug build adds 'test-keyring', the"
    echo "  in-memory keychain the Avalonia smoke test starts its host with."
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

BINDGEN="$(scripts/csharp-bindgen.sh --path)"

# The cargo feature set the bridge compiles with. The generated C# only exports
# the bridge functions whose features are on, so each edition gets bindings from
# the matching feature set.
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
BAE_BRIDGE_TARGET="${BAE_BRIDGE_TARGET:-x86_64-unknown-linux-gnu}"

rustup target add "$BAE_BRIDGE_TARGET"

# FFMPEG_DIR defaults to .cargo/config.toml's bae-ffmpeg/dist.
./scripts/check-ffmpeg-dist.sh scripts/setup-ffmpeg.sh "${FFMPEG_DIR:-bae-ffmpeg/dist}"

echo "Building bae-bridge for $BAE_BRIDGE_TARGET ($CARGO_PROFILE, features: ${BAE_BRIDGE_FEATURES:-(none)})..."
# `cargo rustc --crate-type cdylib` builds only the shared library. The crate
# also lists `staticlib` for Apple, whose builds link the .a into the framework
# — but the C# apps only load the cdylib, and a debug archive carries the whole
# dependency graph's debug info, so the .a is pure build-time waste here.
# Bindgen reads the same uniffi metadata from the cdylib.
./scripts/cargo.sh rustc $CARGO_FLAGS \
    --target "$BAE_BRIDGE_TARGET" \
    --target-dir "$LINUX_TARGET_DIR" \
    -p bae-bridge \
    --lib \
    --features "$BAE_BRIDGE_FEATURES" \
    --crate-type cdylib

# The uniffi C# bindings DllImport("uniffi_bae_bridge"); .NET's native probing
# resolves that to libuniffi_bae_bridge.so on Linux, so the cdylib is copied to
# that name.
BRIDGE_SO="$LINUX_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/libbae_bridge.so"
UNIFFI_BRIDGE_SO="$LINUX_TARGET_DIR/$BAE_BRIDGE_TARGET/$CARGO_PROFILE/libuniffi_bae_bridge.so"
if [[ ! -f "$BRIDGE_SO" ]]; then
    echo "Expected shared library not found: $BRIDGE_SO" >&2
    exit 1
fi
cp "$BRIDGE_SO" "$UNIFFI_BRIDGE_SO"

echo "Generating C# bindings into $BAE_BRIDGE_CSHARP_BINDINGS_DIR ..."
rm -rf "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
mkdir -p "$BAE_BRIDGE_CSHARP_BINDINGS_DIR"
"$BINDGEN" \
    --library "$BRIDGE_SO" \
    --crate bae_bridge \
    --out-dir "$BAE_BRIDGE_CSHARP_BINDINGS_DIR/" \
    --no-format

echo ""
echo "Done ($CARGO_PROFILE). Outputs:"
echo "  $BRIDGE_SO"
echo "  $UNIFFI_BRIDGE_SO"
echo "  $BAE_BRIDGE_CSHARP_BINDINGS_DIR/"
