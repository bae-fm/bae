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

if [[ "$SKIP_RUST" == false ]]; then
    if [[ "$RELEASE" == true ]]; then
        ./bae-bridge/build-macos.sh --release
    else
        ./bae-bridge/build-macos.sh
    fi
    ./bae-bridge/install-swift-bindings.sh macos
fi

if [[ "$RELEASE" == true ]]; then
    CONFIG=Release
else
    CONFIG=Local
fi

cd bae-macos/bae && xcodegen && cd ../..

# Build into the same derived-data cache the pre-commit/post-checkout hooks
# use (they run xcodebuild from bae-macos/bae with -derivedDataPath
# .build/derivedData, overridable via BAE_MACOS_DERIVED_DATA_PATH; relative
# overrides resolve against bae-macos/bae, matching the hooks). Caveat: the
# hooks build with CODE_SIGNING_ALLOWED=NO while this script signs, so
# alternating a hook build and a run.sh build invalidates some incremental
# state — the module caches and SourcePackages checkouts, the bulk of the
# cache, are still shared.
DERIVED_DATA="${BAE_MACOS_DERIVED_DATA_PATH:-.build/derivedData}"
DERIVED_DATA="$(cd bae-macos/bae && mkdir -p "$DERIVED_DATA" && cd "$DERIVED_DATA" && pwd)"

# The Local configuration has its own product directory, separate from Debug
# XCTest and preview hosts. Keep the installed app identity explicit here for
# both Local and Release builds.
xcodebuild -project bae-macos/bae/bae.xcodeproj \
    -scheme bae \
    -configuration "$CONFIG" \
    -derivedDataPath "$DERIVED_DATA" \
    GENERATE_INFOPLIST_FILE=NO \
    INFOPLIST_FILE=bae/Info.plist \
    PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
    PRODUCT_NAME="$PRODUCT_NAME" \
    build

if [[ "$OPEN" == true ]]; then
    open "$DERIVED_DATA/Build/Products/$CONFIG/$PRODUCT_NAME.app" --env BAE_IMPORT_TRACE=1
fi
