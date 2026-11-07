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
./scripts/build-layout.sh

# FFmpeg comes from the configured bae-ffmpeg distribution, or the local
# distribution populated by scripts/setup-ffmpeg.sh when none is configured.
# The shipped .pc files carry a CI-baked prefix (/Users/runner/...), so point
# ffmpeg-sys-next at the distribution directly via FFMPEG_DIR: it reads headers
# from $FFMPEG_DIR/include and emits the link search for $FFMPEG_DIR/lib,
# bypassing pkg-config's dead prefix.
FFMPEG_DIR="${FFMPEG_DIR:-$PWD/bae-ffmpeg/dist}"
export FFMPEG_DIR

./scripts/check-ffmpeg-dist.sh scripts/setup-ffmpeg.sh "$FFMPEG_DIR"

case "$EDITION" in
    bae)
        BAE_BRIDGE_FEATURES_VALUE="oauth-providers,cloudkit,cast"
        BUNDLE_ID="fm.bae.desktop"
        PRODUCT_NAME="bae"
        ;;
    baeium)
        BAE_BRIDGE_FEATURES_VALUE="cast"
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

# The runnable app has its own tree (scripts/xcode-trees.sh says what differs
# from the unsigned tree the checks and tests build into): it is signed and has
# its own bundle id and plist, so a build into one tree would replace the
# other's app bundle.
source scripts/xcode-trees.sh
mkdir -p "$XCODE_MACOS_RUN_TREE" "$XCODE_MACOS_PACKAGES"

# Pass app inputs under private names. The app target maps these to Xcode build
# settings; package targets do not, so their resource bundles keep their own
# names and generated plists.
XCODEBUILD_ARGUMENTS=(
    -project bae-macos/bae/bae.xcodeproj
    -scheme bae
    -configuration "$CONFIG"
    "${XCODE_MACOS_RUN_ARGS[@]}"
    "${XCODE_MACOS_PACKAGES_ARGS[@]}"
    BAE_APP_GENERATE_INFOPLIST_FILE=NO
    BAE_APP_INFOPLIST_FILE=bae/Info.plist
    BAE_APP_BUNDLE_IDENTIFIER="$BUNDLE_ID"
    BAE_APP_PRODUCT_NAME="$PRODUCT_NAME"
    BAE_APP_LSUIELEMENT=NO
)
if [[ "$SKIP_RUST" == true ]]; then
    XCODEBUILD_ARGUMENTS+=(BAE_SKIP_RUST_BRIDGE=YES)
fi
BAE_SKIP_RUST_BRIDGE="$([[ "$SKIP_RUST" == true ]] && echo YES || echo NO)" \
    ./bae-macos/ensure-bridge.sh "$CONFIG"
xcodebuild "${XCODEBUILD_ARGUMENTS[@]}" build

# The app must load only what it bundles and what macOS provides. A Debug build
# also carries Xcode's own runpath to the build products' PackageFrameworks.
python3 scripts/check-bundled-libraries.py \
    --allow-rpath "$XCODE_MACOS_RUN_TREE/Build/Products/$CONFIG/PackageFrameworks" \
    "$XCODE_MACOS_RUN_TREE/Build/Products/$CONFIG/$PRODUCT_NAME.app"

if [[ "$OPEN" == true ]]; then
    open "$XCODE_MACOS_RUN_TREE/Build/Products/$CONFIG/$PRODUCT_NAME.app"
fi
