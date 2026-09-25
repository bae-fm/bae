#!/usr/bin/env bash
set -euo pipefail

SKIP_RUST=false
RELEASE=false
OPEN=true
EDITION=bae

usage() {
    cat <<'EOF'
Usage: bae-avalonia/run.sh [--skip-rust] [--release] [--no-open] [--edition bae|baeium]
Builds and launches the Avalonia skeleton on macOS, including .NET and FFmpeg.
  --skip-rust  Reuse this runner's last Rust build for this edition/configuration
  --release    Build Rust and .NET in release mode
  --no-open    Build and sign without launching
  --edition    Build bae (default) or baeium

Requires dotnet, cargo, the pinned uniffi-bindgen-cs, and macOS signing tools.
Signing uses the development profile embedded by bae-macos/run.sh's Debug build.
BAE_AVALONIA_PROVISIONING_PROFILE can instead name an explicit profile; its
signing certificate and private key must be available in the login keychain.
FFMPEG_DIR selects the FFmpeg distribution (default: bae-ffmpeg/dist).
Quit this runner's app before rebuilding it.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-rust) SKIP_RUST=true ;;
        --release) RELEASE=true ;;
        --no-open) OPEN=false ;;
        --edition)
            if [[ $# -lt 2 ]]; then
                echo 'Missing value for --edition' >&2
                exit 1
            fi
            EDITION="$2"
            shift
            ;;
        --edition=*) EDITION="${1#*=}" ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown flag: $1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done

case "$EDITION" in
    bae) FEATURES=oauth-providers,desktop; BINDINGS_EDITION=full; BUNDLE_ID=fm.bae.desktop ;;
    baeium) FEATURES=desktop; BINDINGS_EDITION=baeium; BUNDLE_ID=fm.bae.desktop.baeium ;;
    *) echo "Unknown edition: $EDITION" >&2; exit 1 ;;
esac
if [[ "$(uname -s)" != Darwin ]]; then
    echo 'This runner builds the Avalonia skeleton for macOS.' >&2
    exit 1
fi

cd "$(dirname "$0")/.."
REPO="$PWD"
for tool in cargo rustc dotnet python3 uniffi-bindgen-cs codesign security install_name_tool otool; do
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
case "$(rustc -vV | awk '/^host:/ {print $2}')" in
    aarch64-apple-darwin) RID=osx-arm64 ;;
    x86_64-apple-darwin) RID=osx-x64 ;;
    *) echo 'A macOS Rust host toolchain is required.' >&2; exit 1 ;;
esac

CONFIG=Debug
CARGO_FLAGS=()
if [[ "$RELEASE" == true ]]; then
    CONFIG=Release
    CARGO_FLAGS+=(--release)
fi
export FFMPEG_DIR="${FFMPEG_DIR:-$REPO/bae-ffmpeg/dist}"
if [[ ! -f "$FFMPEG_DIR/include/libavutil/avutil.h" ]]; then
    echo "FFmpeg missing at $FFMPEG_DIR; run scripts/setup-ffmpeg.sh first." >&2
    exit 1
fi

RUN_DIR="$REPO/bae-avalonia/bin/run/$RID/$EDITION/$CONFIG"
APP="$RUN_DIR/$EDITION Avalonia.app"
PROFILE="${BAE_AVALONIA_PROVISIONING_PROFILE:-$REPO/bae-macos/bae/.build/runDerivedData/Build/Products/Debug/$EDITION.app/Contents/embedded.provisionprofile}"
BUNDLE_ARGS=(--profile "$PROFILE" --bundle-id "$BUNDLE_ID" --app "$APP")
python3 bae-avalonia/macos_bundle.py "${BUNDLE_ARGS[@]}" --check-signing
mkdir -p "$RUN_DIR"

# Keep a copy for --skip-rust, so another edition built in the shared Cargo
# target directory cannot silently change the bridge this runner reuses.
BRIDGE="$RUN_DIR/libbae_bridge.dylib"
if [[ "$SKIP_RUST" == false ]]; then
    cargo rustc ${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"} -p bae-bridge --lib \
        --features "$FEATURES" --crate-type cdylib --message-format=json-render-diagnostics \
        > "$RUN_DIR/cargo-artifacts.jsonl"
    python3 - "$RUN_DIR/cargo-artifacts.jsonl" "$BRIDGE" <<'PY'
import json
from pathlib import Path
import shutil
import sys

artifacts = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()]
libraries = [Path(name) for artifact in artifacts
             if artifact.get("reason") == "compiler-artifact"
             and artifact["target"]["name"] == "bae_bridge"
             for name in artifact["filenames"] if name.endswith(".dylib")]
if len(libraries) != 1:
    raise SystemExit(f"Expected one bridge dylib from Cargo, got {libraries}")
shutil.copy2(libraries[0], sys.argv[2])
PY
elif [[ ! -f "$BRIDGE" ]]; then
    echo "No cached bridge at $BRIDGE; run without --skip-rust first." >&2
    exit 1
fi

BINDINGS="bae-bridge/csharp-bindings-$BINDINGS_EDITION"
uniffi-bindgen-cs --library "$BRIDGE" --crate bae_bridge --out-dir "$BINDINGS" --no-format
rm -rf "$RUN_DIR/publish"
dotnet publish bae-avalonia/bae-avalonia.csproj \
    --framework net8.0 --configuration "$CONFIG" --runtime "$RID" --self-contained true \
    -p:BridgeBindingsDir="../$BINDINGS" \
    --output "$RUN_DIR/publish"
python3 bae-avalonia/macos_bundle.py "${BUNDLE_ARGS[@]}" \
    --publish "$RUN_DIR/publish" --bridge "$BRIDGE" --ffmpeg "$FFMPEG_DIR"

echo "Built: $APP"
if [[ "$OPEN" == true ]]; then
    # The development profile shares the native UI's bundle identifier. Launch
    # this bundle explicitly even when that other UI is already running.
    open -n "$APP"
fi
