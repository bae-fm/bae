#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
test_root="$(mktemp -d)"
trap 'rm -rf "$test_root"' EXIT

stub_bin="$test_root/bin"
capture="$test_root/xcodebuild-arguments"
ffmpeg_capture="$test_root/ffmpeg-dir"
check_capture="$test_root/library-check"
configured_ffmpeg="$test_root/ffmpeg"
run_derived_data="$test_root/run-derived-data"
mkdir -p "$stub_bin" "$configured_ffmpeg"
# run.sh accepts only a dist holding the pinned bae-ffmpeg release.
cp "$repo_root/scripts/bae-ffmpeg-release" "$configured_ffmpeg/VERSION"

printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$stub_bin/xcodegen"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$@\" > \"\$BAE_RUN_ISOLATION_CAPTURE\"" \
    "printf '%s\\n' \"\$FFMPEG_DIR\" > \"\$BAE_RUN_ISOLATION_FFMPEG_CAPTURE\"" \
    > "$stub_bin/xcodebuild"
# The stubbed xcodebuild builds no app, so the library check run.sh runs on it
# is stubbed too, recording what it was asked to check.
printf '%s\n' \
    '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$@\" > \"$check_capture\"" \
    > "$stub_bin/python3"
chmod +x "$stub_bin/xcodegen" "$stub_bin/xcodebuild" "$stub_bin/python3"

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

for expected in scripts/check-bundled-libraries.py "$run_derived_data/Build/Products/Debug/bae.app"; do
    if ! grep -Fxq -- "$expected" "$check_capture"; then
        echo "run.sh did not check the built app's libraries ($expected)" >&2
        exit 1
    fi
done

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
require_argument "-clonedSourcePackagesDirPath"
require_argument "$repo_root/bae-macos/bae/.build/SourcePackages"
require_argument "BAE_APP_BUNDLE_IDENTIFIER=fm.bae.desktop"
require_argument "BAE_APP_PRODUCT_NAME=bae"
require_argument "BAE_APP_GENERATE_INFOPLIST_FILE=NO"
require_argument "BAE_APP_INFOPLIST_FILE=bae/Info.plist"
require_argument "BAE_APP_LSUIELEMENT=NO"
require_argument "BAE_SKIP_RUST_BRIDGE=YES"

reject_setting() {
    local rejected="$1"
    if grep -Eq -- "^${rejected}=" "$capture"; then
        echo "run.sh passed a target setting globally: $rejected" >&2
        exit 1
    fi
}

reject_setting "PRODUCT_BUNDLE_IDENTIFIER"
reject_setting "PRODUCT_NAME"
reject_setting "GENERATE_INFOPLIST_FILE"
reject_setting "INFOPLIST_FILE"

rust_fixture="$test_root/rust-owner-repo"
rust_build_capture="$test_root/rust-builds"
binding_install_capture="$test_root/binding-installs"
mkdir -p \
    "$rust_fixture/bae-macos/bae" \
    "$rust_fixture/bae-bridge" \
    "$rust_fixture/scripts" \
    "$rust_fixture/bae-ffmpeg/dist"
cp "$repo_root/bae-macos/run.sh" "$rust_fixture/bae-macos/run.sh"
cp "$repo_root/bae-macos/prepare-build.sh" \
    "$rust_fixture/bae-macos/prepare-build.sh"
cp "$repo_root/scripts/check-ffmpeg-dist.sh" "$repo_root/scripts/bae-ffmpeg-release" \
    "$rust_fixture/scripts/"
cp "$repo_root/scripts/bae-ffmpeg-release" "$rust_fixture/bae-ffmpeg/dist/VERSION"

printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'if [[ "${1:-}" == "--release" ]]; then' \
    "    printf 'release\\n' >> '$rust_build_capture'" \
    'else' \
    "    printf 'debug\\n' >> '$rust_build_capture'" \
    'fi' \
    > "$rust_fixture/bae-bridge/build-macos.sh"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    "printf '%s\\n' \"\$*\" >> '$binding_install_capture'" \
    > "$rust_fixture/bae-bridge/install-swift-bindings.sh"
# run.sh links the checkout's build folders first; the fixture is no git
# checkout, so it gets a no-op in place of scripts/build-layout.sh.
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$rust_fixture/scripts/build-layout.sh"
chmod +x \
    "$rust_fixture/scripts/build-layout.sh" \
    "$rust_fixture/bae-macos/run.sh" \
    "$rust_fixture/bae-macos/prepare-build.sh" \
    "$rust_fixture/bae-bridge/build-macos.sh" \
    "$rust_fixture/bae-bridge/install-swift-bindings.sh"

printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'configuration=Debug' \
    'skip_rust=NO' \
    'while [[ $# -gt 0 ]]; do' \
    '    case "$1" in' \
    '        -configuration) configuration="$2"; shift ;;' \
    '        BAE_SKIP_RUST_BRIDGE=YES) skip_rust=YES ;;' \
    '    esac' \
    '    shift' \
    'done' \
    'CONFIGURATION="$configuration" BAE_SKIP_RUST_BRIDGE="$skip_rust" \' \
    '    "$BAE_RUST_FIXTURE_ROOT/bae-macos/prepare-build.sh"' \
    > "$stub_bin/xcodebuild"
chmod +x "$stub_bin/xcodebuild"

run_rust_fixture() {
    : > "$rust_build_capture"
    : > "$binding_install_capture"
    (
        cd "$rust_fixture"
        PATH="$stub_bin:$PATH" \
            BAE_RUST_FIXTURE_ROOT="$rust_fixture" \
            ./bae-macos/run.sh --no-open "$@"
    )
}

expect_rust_builds() {
    local expected="$1"
    if [[ "$(< "$rust_build_capture")" != "$expected" ]]; then
        echo "Unexpected Rust bridge builds: $(< "$rust_build_capture")" >&2
        exit 1
    fi
    if [[ -s "$binding_install_capture" ]]; then
        echo "run.sh installed bindings outside the scheme pre-action" >&2
        exit 1
    fi
}

run_rust_fixture
expect_rust_builds "debug"

run_rust_fixture --skip-rust
expect_rust_builds ""

run_rust_fixture --release
expect_rust_builds "release"

(cd "$repo_root/bae-macos/bae" && xcodegen >/dev/null)
if grep -Fq 'Info.plist in Resources' \
    "$repo_root/bae-macos/bae/bae.xcodeproj/project.pbxproj"
then
    echo "The application Info.plist is copied as a resource" >&2
    exit 1
fi

show_build_settings() {
    local configuration="$1"
    shift
    cd "$repo_root/bae-macos/bae"
    xcodebuild -project bae.xcodeproj -scheme bae \
        -configuration "$configuration" \
        -derivedDataPath .build/derivedData \
        -clonedSourcePackagesDirPath .build/SourcePackages \
        -skipPackageUpdates -disableAutomaticPackageResolution \
        "$@" \
        -showBuildSettings 2>&1
}

if ! debug_settings="$(show_build_settings Debug)"; then
    echo "$debug_settings" >&2
    exit 1
fi

if ! run_settings="$(show_build_settings Debug \
    BAE_APP_BUNDLE_IDENTIFIER=fm.bae.desktop \
    BAE_APP_PRODUCT_NAME=bae \
    BAE_APP_GENERATE_INFOPLIST_FILE=NO \
    BAE_APP_INFOPLIST_FILE=bae/Info.plist \
    BAE_APP_LSUIELEMENT=NO)"
then
    echo "$run_settings" >&2
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

if ! grep -Eq \
    '^[[:space:]]*PRODUCT_BUNDLE_IDENTIFIER = fm\.bae\.desktop$' \
    <<< "$run_settings"
then
    echo "The runnable app does not use its installed identity" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*GENERATE_INFOPLIST_FILE = NO$' \
    <<< "$run_settings"
then
    echo "The runnable app generates an Info.plist" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*INFOPLIST_FILE = bae/Info\.plist$' \
    <<< "$run_settings"
then
    echo "The runnable app does not use the application Info.plist" >&2
    exit 1
fi

if ! grep -Eq \
    '^[[:space:]]*INFOPLIST_KEY_LSUIElement = NO$' \
    <<< "$run_settings"
then
    echo "The runnable app is configured as an agent app" >&2
    exit 1
fi
