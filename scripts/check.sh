#!/usr/bin/env bash
# Locally reproduce all non-Windows CI checks before pushing.
#
# Usage: scripts/check.sh
#
# Runs the same non-Windows gates as CI. Missing platform toolchains or lint
# tools are failures.
#
# Only Windows is excluded: bae-windows requires the Windows toolchain and can
# only be validated in CI.

set -uo pipefail

if [[ $# -gt 0 ]]; then
  echo "Usage: scripts/check.sh" >&2
  exit 1
fi

# ── Environment ───────────────────────────────────────────────────────────────
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-macos}"
if command -v brew &>/dev/null; then
  BREW_PREFIX="$(brew --prefix)"
  export LIBRARY_PATH="${BREW_PREFIX}/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
fi

export NDK_VERSION="${NDK_VERSION:-29.0.14206865}"

if [[ -z "${ANDROID_HOME:-}" ]]; then
  if [[ -n "${ANDROID_SDK_ROOT:-}" ]]; then
    export ANDROID_HOME="$ANDROID_SDK_ROOT"
  elif [[ -d "$HOME/Library/Android/sdk" ]]; then
    export ANDROID_HOME="$HOME/Library/Android/sdk"
  fi
fi

if [[ -z "${ANDROID_HOME:-}" || ! -d "$ANDROID_HOME" ]]; then
  echo "ANDROID_HOME is unset and no Android SDK was found at ~/Library/Android/sdk" >&2
  exit 1
fi

if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  if [[ -d "$ANDROID_HOME/ndk/$NDK_VERSION" ]]; then
    export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/$NDK_VERSION"
  else
    echo "ANDROID_NDK_HOME is unset and $ANDROID_HOME/ndk/$NDK_VERSION does not exist" >&2
    exit 1
  fi
fi

if [[ ! -d "$ANDROID_NDK_HOME" ]]; then
  echo "ANDROID_NDK_HOME does not exist: $ANDROID_NDK_HOME" >&2
  exit 1
fi

if [[ ! -d "bae-ffmpeg/ios" ]]; then
  echo "bae-ffmpeg/ios is absent; run scripts/build-ffmpeg-ios.sh" >&2
  exit 1
fi

require_free_kib() {
  local path="$1"
  local required_kib="$2"
  local available_kib
  available_kib="$(df -Pk "$path" | awk 'NR == 2 { print $4 }')"
  if [[ -z "$available_kib" || "$available_kib" -lt "$required_kib" ]]; then
    echo "$path has ${available_kib:-0} KiB free; scripts/check.sh requires ${required_kib} KiB" >&2
    exit 1
  fi
}

REQUIRED_CHECK_SPACE_KIB=$((40 * 1024 * 1024))
require_free_kib "$ROOT" "$REQUIRED_CHECK_SPACE_KIB"
require_free_kib "${TMPDIR:-/tmp}" "$REQUIRED_CHECK_SPACE_KIB"

# ── Output helpers ────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; BOLD='\033[1m'; NC='\033[0m'

PASS=0; FAIL=0
FAILURES=()

section() { echo -e "\n${BOLD}── $1 ──────────────────────────────────${NC}"; }

# Run a command, capture its output, and print ✓ / ✗. Output is shown only on
# failure. Returns the command's exit code without aborting the script.
check() {
  local label="$1"; shift
  local tmpout t0 dt
  tmpout=$(mktemp)
  t0=$SECONDS
  if "$@" >"$tmpout" 2>&1; then
    dt=$((SECONDS - t0))
    [[ $dt -ge 3 ]] \
      && echo -e "  ${GREEN}✓${NC} $label (${dt}s)" \
      || echo -e "  ${GREEN}✓${NC} $label"
    rm -f "$tmpout"
    PASS=$((PASS+1))
    return 0
  else
    dt=$((SECONDS - t0))
    [[ $dt -ge 3 ]] \
      && echo -e "  ${RED}✗${NC} $label (${dt}s)" \
      || echo -e "  ${RED}✗${NC} $label"
    sed 's/^/    /' "$tmpout"
    rm -f "$tmpout"
    FAIL=$((FAIL+1))
    FAILURES+=("$label")
    return 1
  fi
}

# ── Helpers for complex multi-step commands ────────────────────────────────────

_swift_format_lint() {
  find "$1" -name "*.swift" ! -name bae_bridge.swift -print0 \
    | xargs -0 xcrun swift-format lint -s
}

_ios_clippy() {
  local ffmpeg_prefix="$ROOT/bae-ffmpeg/ios/aarch64-apple-ios"
  local device_sdk
  device_sdk="$(xcrun --sdk iphoneos --show-sdk-path)"
  IPHONEOS_DEPLOYMENT_TARGET=16.0 \
  FFMPEG_DIR="$ffmpeg_prefix" \
  BINDGEN_EXTRA_CLANG_ARGS="--target=arm64-apple-ios16.0 -isysroot $device_sdk -I$ffmpeg_prefix/include" \
  CARGO_TARGET_DIR=target-ios \
  cargo clippy --target aarch64-apple-ios -p bae-bridge -- -D warnings
}

_android_clippy() {
  local ndk_home="${ANDROID_NDK_HOME}"
  local toolchain="$ndk_home/toolchains/llvm/prebuilt/darwin-x86_64"
  local ffmpeg_prefix="$ROOT/bae-ffmpeg/android"
  CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$toolchain/bin/aarch64-linux-android35-clang" \
  CC_aarch64_linux_android="$toolchain/bin/aarch64-linux-android35-clang" \
  AR_aarch64_linux_android="$toolchain/bin/llvm-ar" \
  BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--target=aarch64-linux-android35 --sysroot=$toolchain/sysroot -I$ffmpeg_prefix/aarch64/include" \
  FFMPEG_DIR="$ffmpeg_prefix/aarch64" \
  CARGO_TARGET_DIR=target-android \
  cargo clippy --target aarch64-linux-android -p bae-bridge -- -D warnings
}

# ── Rust ──────────────────────────────────────────────────────────────────────
section "Rust"

check "cargo fmt"                   cargo fmt --all -- --check
check "clippy (workspace)"          cargo clippy --workspace -- -D warnings
check "clippy (bae-core + test-utils)" \
  cargo clippy -p bae-core --tests --features bae-core/test-utils -- -D warnings
check "clippy (bae-bridge)"         cargo clippy -p bae-bridge -- -D warnings
check "clippy (bae-core --features oauth-providers)" \
  cargo clippy -p bae-core --features oauth-providers -- -D warnings
check "clippy (bae-bridge --features oauth-providers,cloudkit)" \
  cargo clippy -p bae-bridge --features oauth-providers,cloudkit -- -D warnings

check "cargo machete" cargo machete
check "cargo deny" cargo deny check

check "cargo doc" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# ── Rust tests (loc + bridge + automation) ─────────────────────────────────────
section "Rust tests"

check "cargo test (bae-loc)"           cargo test -p bae-loc
check "cargo test (bae-bridge --lib)"  cargo test -p bae-bridge --lib
check "cargo test (bae-automation)"    cargo test -p bae-automation
check "cargo test (bae-mcp)"           cargo test -p bae-mcp
check "cargo test (bae-cli)"           cargo test -p bae-cli
# Chrome-string orphan gate (the `core.*` keys are gated by loc_key_coverage in
# the bae-bridge test above). Mirrors CI's "localization + bridge tests"; fails
# on unreferenced, un-allowlisted catalog keys so dead/renamed keys are caught
# locally, not only in CI.
check "loc chrome orphans"             python3 scripts/loc-chrome-orphans.py

# ── macOS ──────────────────────────────────────────────────────────────────────
section "macOS"

check "bridge build" ./bae-bridge/build-macos.sh
check "copy macOS bridge binding" \
  cp bae-bridge/swift-bindings-macos/bae_bridge.swift bae-macos/bae/bae/bae_bridge.swift
check "xcodegen" bash -c 'cd bae-macos/bae && xcodegen'
check "xcodebuild" \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -derivedDataPath bae-macos/bae/.build/derivedData \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution build

check "xcodebuild test (baeTests)" \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -derivedDataPath bae-macos/bae/.build/derivedData \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution test

check "swift-format lint" _swift_format_lint bae-macos/bae/bae

check "swiftlint" swiftlint lint --strict --config .swiftlint.yml bae-macos/bae/bae

check "periphery" bash -c '
  cd bae-macos/bae && periphery scan --strict --skip-build \
    --index-store-path .build/derivedData/Index.noindex/DataStore
'

# ── iOS ────────────────────────────────────────────────────────────────────────
section "iOS"

rustup target add aarch64-apple-ios 2>/dev/null || true

check "clippy (iOS aarch64)"  _ios_clippy

check "bridge build" ./bae-bridge/build-ios.sh
check "xcodegen" bash -c 'cd bae-ios/bae && xcodegen'
check "xcodebuild (iphonesimulator)" \
  xcodebuild -project bae-ios/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -sdk iphonesimulator -arch arm64 \
    -derivedDataPath bae-ios/bae/.build/derivedData build

check "swift-format lint" _swift_format_lint bae-ios/bae/bae
check "swiftlint" swiftlint lint --strict --config .swiftlint.yml bae-ios/bae/bae
check "periphery" bash -c '
  cd bae-ios/bae && periphery scan --strict --skip-build \
    --index-store-path .build/derivedData/Index.noindex/DataStore
'

# ── Android ────────────────────────────────────────────────────────────────────
section "Android"

rustup target add aarch64-linux-android 2>/dev/null || true

check "clippy (Android aarch64)" _android_clippy

check "bridge build (Android full)" env BAE_BRIDGE_FEATURES=oauth-providers ./bae-bridge/build-android.sh
check "Gradle unit tests (Android full)" bash -c \
  'cd bae-android && ./gradlew testFullDebugUnitTest --no-daemon'
check "ktlint" ktlint "bae-android/app/src/**/*.kt"
check "detekt" detekt --input bae-android/app/src/main/java \
  --config bae-android/detekt.yml --build-upon-default-config
check "Android lint (full)" bash -c \
  'cd bae-android && ./gradlew lintFullDebug --no-daemon'
check "assemble debug APK (full)" bash -c \
  'cd bae-android && ./gradlew assembleFullDebug --no-daemon'

check "bridge build (Android baeium)" env BAE_BRIDGE_FEATURES= ./bae-bridge/build-android.sh
check "Gradle unit tests (Android baeium)" bash -c \
  'cd bae-android && ./gradlew testBaeiumDebugUnitTest --no-daemon'
check "Android lint (baeium)" bash -c \
  'cd bae-android && ./gradlew lintBaeiumDebug --no-daemon'
check "assemble debug APK (baeium)" bash -c \
  'cd bae-android && ./gradlew assembleBaeiumDebug --no-daemon'

# ── GitHub Actions workflows ───────────────────────────────────────────────────
section "Workflows"

check "actionlint" env SHELLCHECK_OPTS="--severity=error" actionlint

# ── bae-core tests ────────────────────────────────────────────────────────────
section "bae-core tests"
check "cargo test (bae-core)" \
  cargo test -p bae-core --features bae-core/test-utils \
    -- --test-threads=1 --skip test_playback_cpu
check "cargo test (bae-core playback CPU, release)" \
  cargo test -p bae-core --release --features bae-core/test-utils \
    --test test_playback_cpu

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}────────────────────────────────────────────────────────${NC}"
printf "  ${GREEN}✓${NC} %d passed   ${RED}✗${NC} %d failed\n" \
  "$PASS" "$FAIL"

if [[ $FAIL -gt 0 ]]; then
  echo -e "\n  ${RED}Failed:${NC}"
  for f in "${FAILURES[@]}"; do
    echo "    • $f"
  done
  echo ""
  exit 1
fi

echo ""
