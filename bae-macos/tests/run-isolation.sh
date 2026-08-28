#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT

stub_bin="$test_root/bin"
capture="$test_root/xcodebuild-arguments"
ffmpeg_capture="$test_root/ffmpeg-dir"
configured_ffmpeg="$test_root/ffmpeg"
run_derived_data="$test_root/run-derived-data"
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
        BAE_MACOS_RUN_DERIVED_DATA_PATH="$run_derived_data" \
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
require_argument "Debug"
require_argument "-derivedDataPath"
require_argument "$run_derived_data"
require_argument "PRODUCT_BUNDLE_IDENTIFIER=fm.bae.desktop"
require_argument "PRODUCT_NAME=bae"
require_argument "GENERATE_INFOPLIST_FILE=NO"
require_argument "INFOPLIST_FILE=bae/Info.plist"

show_build_settings() {
    local configuration="$1"
    cd "$repo_root/bae-macos/bae"
    xcodebuild -project bae.xcodeproj -scheme bae \
        -configuration "$configuration" \
        -derivedDataPath .build/derivedData \
        -skipPackageUpdates -disableAutomaticPackageResolution \
        -showBuildSettings 2>&1
}

if ! debug_settings="$(show_build_settings Debug)"; then
    echo "$debug_settings" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*PRODUCT_BUNDLE_IDENTIFIER = fm\.bae\.desktop\.xcode$' \
    <<< "$debug_settings"
then
    echo "The Debug test and preview host does not use its separate identity" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*GENERATE_INFOPLIST_FILE = YES$' \
    <<< "$debug_settings"
then
    echo "The Debug test and preview host does not generate its own Info.plist" >&2
    exit 1
fi

if grep -Eq '^[[:space:]]*INFOPLIST_FILE = .+$' <<< "$debug_settings"; then
    echo "The Debug test and preview host uses the application Info.plist" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*INFOPLIST_KEY_LSUIElement = YES$' \
    <<< "$debug_settings"
then
    echo "The Debug test and preview host appears in the Dock" >&2
    exit 1
fi
