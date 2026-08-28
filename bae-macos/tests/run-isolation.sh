#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT

stub_bin="$test_root/bin"
capture="$test_root/xcodebuild-arguments"
ffmpeg_capture="$test_root/ffmpeg-dir"
configured_ffmpeg="$test_root/ffmpeg"
mkdir -p "$stub_bin" "$configured_ffmpeg/include/libavutil"
: > "$configured_ffmpeg/include/libavutil/avutil.h"

printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$stub_bin/xcodegen"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$@\" > \"\$BAE_RUN_ISOLATION_CAPTURE\"" \
    "printf '%s\\n' \"\$FFMPEG_DIR\" > \"\$BAE_RUN_ISOLATION_FFMPEG_CAPTURE\"" \
    > "$stub_bin/xcodebuild"
chmod +x "$stub_bin/xcodegen" "$stub_bin/xcodebuild"

(
    cd "$repo_root"
    PATH="$stub_bin:$PATH" \
        FFMPEG_DIR="$configured_ffmpeg" \
        BAE_RUN_ISOLATION_CAPTURE="$capture" \
        BAE_RUN_ISOLATION_FFMPEG_CAPTURE="$ffmpeg_capture" \
        BAE_MACOS_DERIVED_DATA_PATH="$test_root/derived-data" \
        ./bae-macos/run.sh --skip-rust --no-open
)

if [[ "$(< "$ffmpeg_capture")" != "$configured_ffmpeg" ]]; then
    echo "run.sh did not preserve the configured FFMPEG_DIR" >&2
    exit 1
fi

require_argument() {
    local expected="$1"
    if ! grep -Fxq -- "$expected" "$capture"; then
        echo "run.sh did not pass expected xcodebuild argument: $expected" >&2
        exit 1
    fi
}

require_argument "-configuration"
require_argument "Local"
require_argument "PRODUCT_BUNDLE_IDENTIFIER=fm.bae.desktop"
require_argument "PRODUCT_NAME=bae"

show_build_settings() {
    local configuration="$1"
    cd "$repo_root/bae-macos/bae"
    xcodebuild -project bae.xcodeproj -scheme bae \
        -configuration "$configuration" \
        -derivedDataPath .build/derivedData \
        -skipPackageUpdates -disableAutomaticPackageResolution \
        -showBuildSettings 2>&1
}

if ! local_settings="$(show_build_settings Local)"; then
    echo "$local_settings" >&2
    exit 1
fi

if ! debug_settings="$(show_build_settings Debug)"; then
    echo "$debug_settings" >&2
    exit 1
fi

if ! grep -Eq '^[[:space:]]*CONFIGURATION = Local$' <<< "$local_settings"; then
    echo "The bae scheme has no Local build configuration" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*PRODUCT_BUNDLE_IDENTIFIER = fm\.bae\.desktop$' \
    <<< "$local_settings"
then
    echo "The Local build does not use fm.bae.desktop" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*PRODUCT_BUNDLE_IDENTIFIER = fm\.bae\.desktop\.xcode$' \
    <<< "$debug_settings"
then
    echo "The Debug test and preview host does not use its separate identity" >&2
    exit 1
fi

local_product_dir="$(
    awk -F ' = ' '/^[[:space:]]*CONFIGURATION_BUILD_DIR = / { print $2; exit }' \
        <<< "$local_settings"
)"
debug_product_dir="$(
    awk -F ' = ' '/^[[:space:]]*CONFIGURATION_BUILD_DIR = / { print $2; exit }' \
        <<< "$debug_settings"
)"

if [[ -z "$local_product_dir" || -z "$debug_product_dir" \
    || "$local_product_dir" == "$debug_product_dir" ]]
then
    echo "Local and Debug must write separate app products" >&2
    exit 1
fi
