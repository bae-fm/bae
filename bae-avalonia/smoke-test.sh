#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: bae-avalonia/smoke-test.sh
Builds the Avalonia skeleton and runs its smoke test on a macOS host: the full
bridge as a dylib, the C# bindings generated from it, the app build, and the
test that loads the bridge through those bindings. The Linux and Windows CI jobs
run the same gate with build-linux.sh / build-windows.sh.

Requires dotnet, cargo, and the pinned uniffi-bindgen-cs.
FFMPEG_DIR selects the FFmpeg distribution (default: bae-ffmpeg/dist).
EOF
}

if [[ $# -gt 0 ]]; then
    case "$1" in
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 1 ;;
    esac
fi
if [[ "$(uname -s)" != Darwin ]]; then
    echo 'This script runs the smoke test on macOS; Linux and Windows use their bridge build scripts.' >&2
    exit 1
fi

cd "$(dirname "$0")/.."
REPO="$PWD"
./scripts/build-layout.sh
for tool in cargo dotnet python3 uniffi-bindgen-cs; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "Required command missing: $tool" >&2
        exit 1
    fi
done
if [[ "$(uniffi-bindgen-cs --version)" != 'uniffi-bindgen 0.11.0+v0.32.0' ]]; then
    echo 'Install the C# generator matching this bridge:' >&2
    echo 'cargo install --git https://github.com/bae-fm/uniffi-bindgen-cs --branch uniffi-0.32-bae uniffi-bindgen-cs --locked' >&2
    exit 1
fi
export FFMPEG_DIR="${FFMPEG_DIR:-$REPO/bae-ffmpeg/dist}"
./scripts/check-ffmpeg-dist.sh scripts/setup-ffmpeg.sh "$FFMPEG_DIR"

# The bridge is copied out of Cargo's target directory as soon as it is built,
# so a later build there cannot change it between the build and the binding
# generation that reads it.
LIB_DIR="$REPO/bae-avalonia/bin/smoke"
mkdir -p "$LIB_DIR"
# test-keyring: the smoke test starts its host with an in-memory keychain.
./scripts/cargo.sh rustc -p bae-bridge --lib --features oauth-providers,cast,test-keyring \
    --crate-type cdylib
# The bindings import uniffi_bae_bridge, which .NET resolves to
# libuniffi_bae_bridge.dylib on macOS.
BRIDGE="$LIB_DIR/libuniffi_bae_bridge.dylib"
cp target/debug/libbae_bridge.dylib "$BRIDGE"

BINDINGS="bae-bridge/csharp-bindings-full"
rm -rf "$BINDINGS"
uniffi-bindgen-cs --library "$BRIDGE" --crate bae_bridge --out-dir "$BINDINGS" --no-format
dotnet build bae-avalonia/bae-avalonia.csproj -c Debug -p:EnforceCodeStyleInBuild=true
# A `dotnet` on PATH can be a /bin/bash wrapper (Homebrew's is), and macOS
# strips DYLD_* from what a protected shell launches, so the test host would
# never see the loader path. Run the .NET host binary itself instead.
DOTNET_ROOT="$(dotnet --list-sdks | sed -n '1s/.*\[\(.*\)\/sdk\]$/\1/p')"
if [[ ! -x "$DOTNET_ROOT/dotnet" ]]; then
    echo "Could not find the .NET host under the SDK root '$DOTNET_ROOT'." >&2
    exit 1
fi
export DOTNET_ROOT
export DYLD_LIBRARY_PATH="$LIB_DIR:$FFMPEG_DIR/lib"
"$DOTNET_ROOT/dotnet" test bae-avalonia/bae-avalonia.Tests -c Debug
