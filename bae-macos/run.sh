#!/usr/bin/env bash
set -euo pipefail

SKIP_RUST=false
RELEASE=false
OPEN=true
EDITION=bae

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-rust) SKIP_RUST=true ;;
        --release) RELEASE=true ;;
        --no-open) OPEN=false ;;
        --edition)
            if [[ $# -lt 2 ]]; then
                echo "Missing value for --edition"
                exit 1
            fi
            EDITION="${2:-}"
            shift
            ;;
        --edition=*)
            EDITION="${1#*=}"
            ;;
        -h|--help)
            echo "Usage: $0 [--skip-rust] [--release] [--no-open] [--edition bae|baeium]"
            echo "  Builds (and optionally runs) the macOS app."
            echo "  --skip-rust  Skip the Rust bridge build"
            echo "  --release    Build Rust in release mode, Swift in Release config"
            echo "  --no-open    Build only, don't launch the app"
            echo "  --edition    Build bae or baeium"
            exit 0
            ;;
        *) echo "Unknown flag: $1"; exit 1 ;;
    esac
    shift
done

cd "$(dirname "$0")/.."

# FFmpeg comes from the configured bae-ffmpeg distribution, or the local
# distribution populated by scripts/setup-ffmpeg.sh when none is configured.
# The shipped .pc files carry a CI-baked prefix (/Users/runner/...), so point
# ffmpeg-sys-next at the distribution directly via FFMPEG_DIR: it reads headers
# from $FFMPEG_DIR/include and emits the link search for $FFMPEG_DIR/lib,
# bypassing pkg-config's dead prefix.
FFMPEG_DIR="${FFMPEG_DIR:-$PWD/bae-ffmpeg/dist}"
export FFMPEG_DIR

if [[ ! -f "$FFMPEG_DIR/include/libavutil/avutil.h" ]]; then
    echo "bae-ffmpeg dist missing at $FFMPEG_DIR — run scripts/setup-ffmpeg.sh first" >&2
    exit 1
fi

case "$EDITION" in
    bae)
        BAE_BRIDGE_FEATURES_VALUE="oauth-providers,cloudkit,desktop"
        BUNDLE_ID="fm.bae.desktop"
        PRODUCT_NAME="bae"
        ;;
    baeium)
        BAE_BRIDGE_FEATURES_VALUE="desktop"
        BUNDLE_ID="fm.bae.desktop.baeium"
        PRODUCT_NAME="baeium"
        ;;
    *)
        echo "Unknown edition: $EDITION"
        exit 1
        ;;
esac

export BAE_BRIDGE_FEATURES="$BAE_BRIDGE_FEATURES_VALUE"

if [[ "$RELEASE" == true ]]; then
    CONFIG=Release
else
    CONFIG=Debug
fi

cd bae-macos/bae && xcodegen && cd ../..

# Keep this runnable app separate from Xcode's preview and test app host. Both
# use the Debug configuration, so Swift packages receive the same DEBUG
# compilation condition, while separate DerivedData roots prevent either build
# from replacing the other's app bundle or build records.
DERIVED_DATA="${BAE_MACOS_RUN_DERIVED_DATA_PATH:-.build/runDerivedData}"
DERIVED_DATA="$(cd bae-macos/bae && mkdir -p "$DERIVED_DATA" && cd "$DERIVED_DATA" && pwd)"

# Pass app inputs under private names. The app target maps these to Xcode build
# settings; package resource bundles do not, so they retain generated plists.
XCODEBUILD_ARGUMENTS=(
    -project bae-macos/bae/bae.xcodeproj
    -scheme bae
    -configuration "$CONFIG"
    -derivedDataPath "$DERIVED_DATA"
    BAE_RUN_APP_GENERATE_INFOPLIST_FILE=NO
    BAE_RUN_APP_INFOPLIST_FILE=bae/Info.plist
    BAE_RUN_APP_BUNDLE_IDENTIFIER="$BUNDLE_ID"
    BAE_RUN_APP_PRODUCT_NAME="$PRODUCT_NAME"
    BAE_RUN_APP_LSUIELEMENT=NO
)
if [[ "$SKIP_RUST" == true ]]; then
    XCODEBUILD_ARGUMENTS+=(BAE_SKIP_RUST_BRIDGE=YES)
fi
xcodebuild "${XCODEBUILD_ARGUMENTS[@]}" build

if [[ "$OPEN" == true ]]; then
    open "$DERIVED_DATA/Build/Products/$CONFIG/$PRODUCT_NAME.app"
fi
