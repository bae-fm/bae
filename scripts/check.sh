#!/usr/bin/env bash
# Locally reproduce all non-Windows CI checks before pushing.
#
# Usage: scripts/check.sh [--tests] [--no-ios] [--no-android]
#
#   --tests       Run the full bae-core test suite (~5 min; skipped by default).
#   --no-ios      Skip iOS bridge build and Swift checks.
#   --no-android  Skip Android checks (also auto-skipped when ANDROID_NDK_HOME is unset).
#
# Not covered: bae-windows and bae-windows-ffi require the Windows toolchain
# and can only be validated in CI.

set -uo pipefail

# ── Flags ─────────────────────────────────────────────────────────────────────
RUN_TESTS=false
RUN_IOS=true
RUN_ANDROID=true
IOS_SKIP_REASON=""
ANDROID_SKIP_REASON=""

for arg in "$@"; do
  case "$arg" in
    --tests)      RUN_TESTS=true ;;
    --no-ios)     RUN_IOS=false; IOS_SKIP_REASON="--no-ios" ;;
    --no-android) RUN_ANDROID=false; ANDROID_SKIP_REASON="--no-android" ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

# ── Environment ───────────────────────────────────────────────────────────────
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target-macos}"
if command -v brew &>/dev/null; then
  BREW_PREFIX="$(brew --prefix)"
  export LIBRARY_PATH="${BREW_PREFIX}/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
fi

# Auto-disable Android when NDK is unavailable.
if [[ $RUN_ANDROID == true && -z "${ANDROID_NDK_HOME:-}" ]]; then
  RUN_ANDROID=false
  ANDROID_SKIP_REASON="ANDROID_NDK_HOME unset"
fi

# Auto-disable iOS when ffmpeg-ios is absent.
if [[ $RUN_IOS == true && ! -d "third_party/ffmpeg-ios" ]]; then
  RUN_IOS=false
  IOS_SKIP_REASON="third_party/ffmpeg-ios absent — run scripts/build-ffmpeg-ios.sh"
fi

# ── Output helpers ────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BOLD='\033[1m'; NC='\033[0m'

PASS=0; FAIL=0; SKIP=0
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

skip() { echo -e "  ${YELLOW}–${NC} $1 ($2)"; SKIP=$((SKIP+1)); }

# ── Helpers for complex multi-step commands ────────────────────────────────────

_swift_format_lint() {
  find "$1" -name "*.swift" ! -name bae_bridge.swift -print0 \
    | xargs -0 xcrun swift-format lint -s
}

_ios_clippy() {
  local ffmpeg_prefix="$ROOT/third_party/ffmpeg-ios/aarch64-apple-ios"
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
  local ffmpeg_prefix="$ROOT/third_party/ffmpeg-android"
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

if cargo machete --version &>/dev/null; then
  check "cargo machete" cargo machete
else
  skip "cargo machete" "not installed — cargo binstall cargo-machete"
fi

if cargo deny --version &>/dev/null; then
  check "cargo deny" cargo deny check
else
  skip "cargo deny" "not installed — cargo binstall cargo-deny"
fi

check "cargo doc" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# ── Rust tests (loc + bridge) ──────────────────────────────────────────────────
section "Rust tests"

check "cargo test (bae-loc)"           cargo test -p bae-loc
check "cargo test (bae-bridge --lib)"  cargo test -p bae-bridge --lib

# ── macOS ──────────────────────────────────────────────────────────────────────
section "macOS"

MACOS_BUILD_OK=false
if check "bridge build" ./bae-bridge/build-macos.sh; then
  cp bae-bridge/swift-bindings/bae_bridge.swift bae-macos/bae/bae/bae_bridge.swift
  cp bae-bridge/loc/generated/apple/Core.xcstrings bae-macos/bae/bae/Core.xcstrings
  check "xcodegen" bash -c 'cd bae-macos/bae && xcodegen'
  MACOS_XCODE_OK=false
  if check "xcodebuild" \
      xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
        CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
        -derivedDataPath bae-macos/bae/.build/derivedData build; then
    MACOS_BUILD_OK=true
    MACOS_XCODE_OK=true
  fi
else
  skip "xcodegen" "bridge build failed"
  skip "xcodebuild" "bridge build failed"
fi

check "swift-format lint" _swift_format_lint bae-macos/bae/bae

if command -v swiftlint &>/dev/null; then
  check "swiftlint" swiftlint lint --strict --config .swiftlint.yml bae-macos/bae/bae
else
  skip "swiftlint (macOS)" "not installed — brew install swiftlint"
fi

if command -v periphery &>/dev/null; then
  if [[ ${MACOS_XCODE_OK:-false} == true ]]; then
    check "periphery" bash -c '
      cd bae-macos/bae && periphery scan --strict --skip-build \
        --index-store-path .build/derivedData/Index.noindex/DataStore
    '
  else
    skip "periphery (macOS)" "xcodebuild failed"
  fi
else
  skip "periphery (macOS)" "not installed — brew install peripheryapp/periphery/periphery"
fi

# ── iOS ────────────────────────────────────────────────────────────────────────
section "iOS"

if [[ $RUN_IOS == false ]]; then
  skip "iOS checks" "$IOS_SKIP_REASON"
else
  rustup target add aarch64-apple-ios 2>/dev/null || true

  check "clippy (iOS aarch64)"  _ios_clippy

  IOS_XCODE_OK=false
  if check "bridge build" ./bae-bridge/build-ios.sh; then
    check "xcodegen" bash -c 'cd bae-ios/bae && xcodegen'
    if check "xcodebuild (iphonesimulator)" \
        xcodebuild -project bae-ios/bae/bae.xcodeproj -scheme bae -configuration Debug \
          CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
          -sdk iphonesimulator -arch arm64 \
          -derivedDataPath bae-ios/bae/.build/derivedData build; then
      IOS_XCODE_OK=true
    fi
  else
    skip "xcodegen (iOS)" "bridge build failed"
    skip "xcodebuild (iOS)" "bridge build failed"
  fi

  check "swift-format lint" _swift_format_lint bae-ios/bae/bae

  if command -v swiftlint &>/dev/null; then
    check "swiftlint" swiftlint lint --strict --config .swiftlint.yml bae-ios/bae/bae
  else
    skip "swiftlint (iOS)" "not installed — brew install swiftlint"
  fi

  if command -v periphery &>/dev/null; then
    if [[ $IOS_XCODE_OK == true ]]; then
      check "periphery" bash -c '
        cd bae-ios/bae && periphery scan --strict --skip-build \
          --index-store-path .build/derivedData/Index.noindex/DataStore
      '
    else
      skip "periphery (iOS)" "xcodebuild failed"
    fi
  else
    skip "periphery (iOS)" "not installed — brew install peripheryapp/periphery/periphery"
  fi
fi

# ── Android ────────────────────────────────────────────────────────────────────
section "Android"

if [[ $RUN_ANDROID == false ]]; then
  skip "Android checks" "$ANDROID_SKIP_REASON"
else
  rustup target add aarch64-linux-android 2>/dev/null || true

  check "clippy (Android aarch64)" _android_clippy

  ANDROID_BUILD_OK=false
  if check "bridge build" ./bae-bridge/build-android.sh; then
    ANDROID_BUILD_OK=true
    check "Gradle unit tests" bash -c \
      'cd bae-android && ./gradlew testFullDebugUnitTest --no-daemon'
  else
    skip "Gradle unit tests" "bridge build failed"
  fi

  if command -v ktlint &>/dev/null; then
    check "ktlint" ktlint "bae-android/app/src/**/*.kt"
  else
    skip "ktlint" "not installed — brew install ktlint"
  fi

  if command -v detekt &>/dev/null; then
    check "detekt" detekt --input bae-android/app/src/main/java --build-upon-default-config
  else
    skip "detekt" "not installed — brew install detekt"
  fi
fi

# ── GitHub Actions workflows ───────────────────────────────────────────────────
section "Workflows"

if command -v actionlint &>/dev/null; then
  check "actionlint" env SHELLCHECK_OPTS="--severity=error" actionlint
else
  skip "actionlint" "not installed — brew install actionlint"
fi

# ── bae-core tests (opt-in, slow) ─────────────────────────────────────────────
if [[ $RUN_TESTS == true ]]; then
  section "bae-core tests"
  check "cargo test (bae-core)" \
    cargo test -p bae-core --features bae-core/test-utils \
      -- --test-threads=1 --skip test_playback_cpu
  check "cargo test (bae-core playback CPU, release)" \
    cargo test -p bae-core --release --features bae-core/test-utils \
      --test test_playback_cpu
fi

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo -e "${BOLD}────────────────────────────────────────────────────────${NC}"
printf "  ${GREEN}✓${NC} %d passed   ${RED}✗${NC} %d failed   ${YELLOW}–${NC} %d skipped\n" \
  "$PASS" "$FAIL" "$SKIP"

if [[ $FAIL -gt 0 ]]; then
  echo -e "\n  ${RED}Failed:${NC}"
  for f in "${FAILURES[@]}"; do
    echo "    • $f"
  done
  echo ""
  exit 1
fi

echo ""
