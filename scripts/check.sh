#!/usr/bin/env bash
# Runs every non-Windows CI gate serially, for full-system validation; routine
# changes use the pre-commit hook and affected tests. Missing toolchains fail.

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

# The tests load FFmpeg's dylibs from bae-ffmpeg/dist; SIP strips an inherited
# DYLD_* variable, so it is set here.
if [[ -d "$ROOT/bae-ffmpeg/dist/lib" ]]; then
  export DYLD_LIBRARY_PATH="$ROOT/bae-ffmpeg/dist/lib${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}"
fi

# Gradle needs a JDK; Homebrew's keg-only openjdk is found here when JAVA_HOME
# is unset, preferring the LTS releases Gradle 8.10.2 supports.
if [[ -z "${JAVA_HOME:-}" ]] && command -v brew &>/dev/null; then
  for _jdk in openjdk@21 openjdk@17 openjdk; do
    _jdk_home="$(brew --prefix "$_jdk" 2>/dev/null)/libexec/openjdk.jdk/Contents/Home"
    if [[ -d "$_jdk_home" ]]; then
      export JAVA_HOME="$_jdk_home"
      export PATH="$JAVA_HOME/bin:$PATH"
      break
    fi
  done
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

# ── Output helpers ────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; BOLD='\033[1m'; NC='\033[0m'

PASS=0; FAIL=0
FAILURES=()

section() { echo -e "\n${BOLD}── $1 ──────────────────────────────────${NC}"; }

# Runs a command, prints ✓ or ✗ with its output only on failure, and returns
# its exit code.
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
  find "$1" -name "*.swift" ! -name 'bae_bridge_*.swift' -print0 \
    | xargs -0 xcrun swift-format lint -s
}

# The baeium bridge exports none of the feature-gated functions.
_baeium_macos_export_guard() {
  local gated='oauthAuthorize|oauthBegin|oauthCancel|oauthComplete|setOauthClientCreds|signInCloudProvider|useCloudkit'
  if grep -nE "func ($gated)" bae-bridge/swift-bindings-macos/bae_bridge.swift; then
    echo "baeium bridge exported a feature-gated symbol (above): the gate leaked into the S3-only build." >&2
    return 1
  fi
  echo "baeium bridge is clean: no OAuth/CloudKit bindings exported."
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
check "source file layout"          scripts/check-source-file-size.py
check "restricted-path visibility"  scripts/check-rust-visibility.sh
check "owner dependency boundary"   scripts/owner-dependency-boundary.sh
check "UniFFI runtime boundary"      scripts/check-uniffi-runtime-boundary.py
check "macOS native Cargo cache"     scripts/check-macos-build-cache.sh
check "mobile host Cargo cache"      scripts/check-mobile-build-cache.sh
check "clippy (workspace)"          cargo clippy --workspace -- -D warnings
check "clippy (bae-core + test-utils)" \
  cargo clippy -p bae-core --tests --features bae-core/test-utils -- -D warnings
check "clippy (bae-bridge)"         cargo clippy -p bae-bridge -- -D warnings
check "clippy (bae-core --features oauth-providers)" \
  cargo clippy -p bae-core --features oauth-providers -- -D warnings
check "clippy (bae-bridge --features oauth-providers,cloudkit)" \
  cargo clippy -p bae-bridge --features oauth-providers,cloudkit -- -D warnings
# The feature set build-macos.sh ships, which lints the desktop half of the
# bridge.
check "clippy (bae-bridge --features oauth-providers,cloudkit,desktop)" \
  cargo clippy -p bae-bridge --features oauth-providers,cloudkit,desktop -- -D warnings

# bae-core's production code alone, so an item only its tests use fails as
# dead code.
check "dead_code (bae-core lib only)" \
  env RUSTFLAGS="-D warnings" cargo check -p bae-core

# New #[allow(dead_code)] is banned everywhere.
check "no new #[allow(dead_code)]" bash -c '
  offenders=$(grep -rn --include="*.rs" --exclude-dir="target*" \
    "allow(dead_code)" . \
    || true)
  if [ -n "$offenders" ]; then
    echo "New #[allow(dead_code)] is banned (delete the code or #[cfg]-restrict it):"
    echo "$offenders"
    exit 1
  fi
'

check "cargo machete" cargo machete
check "cargo deny" cargo deny check

check "cargo doc" env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# ── Rust tests (loc + theme + bridge + automation) ─────────────────────────────
section "Rust tests"

check "cargo test (bae-loc)"           cargo test -p bae-loc
check "cargo test (bae-theme)"         cargo test -p bae-theme
# Both editions: default features are baeium's, `desktop` the full build's.
check "cargo test (bae-bridge --lib)"  cargo test -p bae-bridge --lib
check "cargo test (bae-bridge --lib, desktop)" \
  cargo test -p bae-bridge --lib --features desktop
check "cargo test (bae-automation)"    cargo test -p bae-automation
check "cargo test (bae-mcp)"           cargo test -p bae-mcp
check "cargo test (bae-subsonic)"      cargo test -p bae-subsonic
check "cargo test (bae-cast)"          cargo test -p bae-cast
check "cargo test (bae-desktop)"       cargo test -p bae-desktop
# Chrome strings no code references; `core.*` keys are covered by the
# bae-bridge test above.
check "loc chrome orphans"             python3 scripts/loc-chrome-orphans.py
check "loc english skeleton"           python3 scripts/loc-english-skeleton.py

# ── Avalonia skeleton ─────────────────────────────────────────────────────────
section "Avalonia skeleton"

# Builds the bridge as a macOS dylib, generates the C# bindings, builds the
# app and smoke-tests it.
check "Avalonia skeleton build + smoke test" bae-avalonia/smoke-test.sh

# ── macOS ──────────────────────────────────────────────────────────────────────
section "macOS"

# The baeium edition first, so the section ends on the full edition a dev's
# next build expects.
check "bridge build (macOS baeium)" \
  env BAE_BRIDGE_FEATURES=desktop ./bae-bridge/build-macos.sh
check "install macOS bridge binding (baeium)" \
  ./bae-bridge/install-swift-bindings.sh macos
check "guard: baeium bridge exports no OAuth/CloudKit binding" \
  _baeium_macos_export_guard
check "xcodegen (macOS baeium)" bash -c 'cd bae-macos/bae && xcodegen'
check "xcodebuild (macOS baeium)" \
  env BAE_BRIDGE_FEATURES=desktop \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -derivedDataPath bae-macos/bae/.build/derivedData \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution build

check "bridge build" ./bae-bridge/build-macos.sh
check "install macOS bridge binding" \
  ./bae-bridge/install-swift-bindings.sh macos
check "xcodegen" bash -c 'cd bae-macos/bae && xcodegen'
check "xcodebuild" \
  env BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,desktop \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -derivedDataPath bae-macos/bae/.build/derivedData \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution build

check "xcodebuild test (baeTests)" \
  env BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,desktop \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -derivedDataPath bae-macos/bae/.build/derivedData \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution test

check "swift-format lint" _swift_format_lint bae-macos/bae/bae
# BaeKit is linted once for both apps.
check "swift-format lint (BaeKit)" _swift_format_lint BaeKit/Sources/BaeKit
check "swift-format lint (Apple host)" \
  _swift_format_lint BaeKit/Sources/AppleHost

check "swiftlint" swiftlint lint --strict --config .swiftlint.yml bae-macos/bae/bae
check "swiftlint (BaeKit)" \
  swiftlint lint --strict --config .swiftlint.yml \
    BaeKit/Sources/BaeKit BaeKit/Sources/AppleHost

check "periphery" bash -c '
  cd bae-macos/bae && periphery scan --strict --skip-build \
    --index-store-path .build/derivedData/Index.noindex/DataStore
'

# ── iOS ────────────────────────────────────────────────────────────────────────
section "iOS"

rustup target add aarch64-apple-ios 2>/dev/null || true

check "clippy (iOS aarch64)"  _ios_clippy

# The baeium edition first, so the section ends on the full edition.
check "bridge build (iOS baeium)" \
  env BAE_BRIDGE_FEATURES=cast ./bae-bridge/build-ios.sh
check "xcodegen (iOS baeium)" bash -c 'cd bae-ios/bae && xcodegen'
check "xcodebuild (iOS baeium, iphonesimulator)" \
  env BAE_BRIDGE_FEATURES=cast \
  xcodebuild -project bae-ios/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -sdk iphonesimulator -arch arm64 \
    -derivedDataPath bae-ios/bae/.build/derivedData build

check "bridge build" ./bae-bridge/build-ios.sh
check "xcodegen" bash -c 'cd bae-ios/bae && xcodegen'
check "xcodebuild (iphonesimulator)" \
  env BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,cast \
  xcodebuild -project bae-ios/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -sdk iphonesimulator -arch arm64 \
    -derivedDataPath bae-ios/bae/.build/derivedData build

# The test run keeps ad-hoc signing: without it the iCloud entitlement is
# stripped and the app traps at launch.
check "xcodebuild test (iOS baeTests)" bash -c '
  DEST=$(xcrun simctl list devices available -j | python3 -c "import json,sys; d=[d for rt in json.load(sys.stdin)[\"devices\"].values() for d in rt if d.get(\"isAvailable\") and d[\"name\"].startswith(\"iPhone\")][-1]; print(d[\"udid\"] + \"\t\" + d[\"name\"])")
  DEST_ID=${DEST%%$'\''\t'\''*}
  DEST_NAME=${DEST#*$'\''\t'\''}
  echo "Testing on simulator: $DEST_NAME ($DEST_ID)"
  BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,cast \
  xcodebuild -project bae-ios/bae/bae.xcodeproj -scheme bae -configuration Debug \
    -destination "platform=iOS Simulator,id=$DEST_ID" \
    -derivedDataPath bae-ios/bae/.build/derivedData test
'

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

check "bridge build (Android full)" env BAE_BRIDGE_FEATURES=oauth-providers,cast ./bae-bridge/build-android.sh
check "Gradle unit tests (Android full)" bash -c \
  'cd bae-android && ./gradlew testFullDebugUnitTest --no-daemon'
check "ktlint" ktlint "bae-android/app/src/**/*.kt"
check "detekt" detekt --input bae-android/app/src/main/java \
  --config bae-android/detekt.yml --build-upon-default-config
check "Android lint (full)" bash -c \
  'cd bae-android && ./gradlew lintFullDebug --no-daemon'
check "assemble debug APK (full)" bash -c \
  'cd bae-android && ./gradlew assembleFullDebug --no-daemon'

check "bridge build (Android baeium)" env BAE_BRIDGE_FEATURES=cast ./bae-bridge/build-android.sh
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
# Suites sharing process state serialize themselves; the CPU test runs alone
# below because it reads process-wide getrusage.
check "cargo test (bae-core)" \
  cargo test -p bae-core --features bae-core/test-utils \
    -- --skip test_playback_cpu
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
