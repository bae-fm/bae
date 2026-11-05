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
./scripts/build-layout.sh
source scripts/xcode-trees.sh

# The Android SDK, JDK and NDK the Gradle and cargo steps use; a missing SDK
# or NDK fails here, before any check runs.
source bae-android/toolchain.sh
if [[ ! -d "$ANDROID_HOME" ]]; then
  echo "No Android SDK at $ANDROID_HOME; set ANDROID_HOME" >&2
  exit 1
fi
android_rust_env || exit 1

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

_ios_clippy() {
  local ffmpeg_prefix="$ROOT/bae-ffmpeg/ios/aarch64-apple-ios"
  local device_sdk
  device_sdk="$(xcrun --sdk iphoneos --show-sdk-path)"
  IPHONEOS_DEPLOYMENT_TARGET=16.0 \
  FFMPEG_DIR="$ffmpeg_prefix" \
  BINDGEN_EXTRA_CLANG_ARGS="--target=arm64-apple-ios16.0 -isysroot $device_sdk -I$ffmpeg_prefix/include" \
  CARGO_TARGET_DIR=target-ios \
  scripts/cargo.sh clippy --target aarch64-apple-ios -p bae-bridge -- -D warnings
}

# ── Rust ──────────────────────────────────────────────────────────────────────
section "Rust"

check "cargo fmt"                   cargo fmt --all -- --check
check "source file layout"          scripts/check-source-file-size.py
check "restricted-path visibility"  scripts/check-rust-visibility.sh
check "reqwest error causes"       scripts/check-reqwest-error-display.py
check "bundled-library checker"     python3 scripts/tests/check-bundled-libraries-test.py
check "owner dependency boundary"   scripts/owner-dependency-boundary.sh
check "UniFFI runtime boundary"      scripts/check-uniffi-runtime-boundary.py
check "every Rust file compiled"    sh -c 'python3 scripts/check-rust-files-compiled.py && python3 scripts/tests/check-rust-files-compiled-test.py'
check "test keychain"                sh -c 'python3 scripts/check-test-keychain.py && python3 scripts/tests/check-test-keychain-test.py'
check "macOS native Cargo cache"     scripts/check-macos-build-cache.sh
check "mobile host Cargo cache"      scripts/check-mobile-build-cache.sh
check "Xcode build trees"           scripts/check-xcode-trees.py
check "clippy (workspace)"          scripts/cargo.sh clippy --workspace -- -D warnings
check "clippy (bae-library + test-utils)" \
  scripts/cargo.sh clippy -p bae-library --tests --features bae-library/test-utils -- -D warnings
check "clippy (bae-util + test-utils)" \
  scripts/cargo.sh clippy -p bae-util --tests --features bae-util/test-utils -- -D warnings
check "clippy (bae-audio + test features)" \
  scripts/cargo.sh clippy -p bae-audio --tests --features test-utils,test-instrumentation -- -D warnings
check "clippy (bae-types + test-utils)" \
  scripts/cargo.sh clippy -p bae-types --tests --features bae-types/test-utils -- -D warnings
check "clippy (bae-catalogs + test features)" \
  scripts/cargo.sh clippy -p bae-catalogs --tests --features test-utils,test-instrumentation -- -D warnings
check "clippy (bae-diagnostics + test-utils)" \
  scripts/cargo.sh clippy -p bae-diagnostics --tests --features bae-diagnostics/test-utils -- -D warnings
check "clippy (bae-app-dir + test-utils)" \
  scripts/cargo.sh clippy -p bae-app-dir --tests --features bae-app-dir/test-utils -- -D warnings
check "clippy (bae-store + test-utils)" \
  scripts/cargo.sh clippy -p bae-store --tests --features bae-store/test-utils -- -D warnings
check "clippy (bae-db + test-utils)" \
  scripts/cargo.sh clippy -p bae-db --tests --features bae-db/test-utils -- -D warnings
check "clippy (bae-preferences + test-utils)" \
  scripts/cargo.sh clippy -p bae-preferences --tests --features bae-preferences/test-utils -- -D warnings
check "clippy (bae-track-audio + test-utils)" \
  scripts/cargo.sh clippy -p bae-track-audio --tests --features bae-track-audio/test-utils -- -D warnings
check "clippy (bae-transfers + test-utils)" \
  scripts/cargo.sh clippy -p bae-transfers --tests --features bae-transfers/test-utils -- -D warnings
check "clippy (bae-views + test-utils)" \
  scripts/cargo.sh clippy -p bae-views --tests --features bae-views/test-utils -- -D warnings
check "clippy (bae-identify + test-utils)" \
  scripts/cargo.sh clippy -p bae-identify --tests --features bae-identify/test-utils -- -D warnings
check "clippy (bae-playback + test-utils)" \
  scripts/cargo.sh clippy -p bae-playback --tests --features bae-playback/test-utils -- -D warnings
check "clippy (bae-import + test-utils)" \
  scripts/cargo.sh clippy -p bae-import --tests --features bae-import/test-utils -- -D warnings
check "clippy (bae-app + test-utils)" \
  scripts/cargo.sh clippy -p bae-app --tests --features bae-app/test-utils -- -D warnings
check "clippy (bae-bridge)"         scripts/cargo.sh clippy -p bae-bridge -- -D warnings
check "clippy (bae-library --features oauth-providers)" \
  scripts/cargo.sh clippy -p bae-library --features oauth-providers -- -D warnings
check "clippy (bae-app --features oauth-providers)" \
  scripts/cargo.sh clippy -p bae-app --features oauth-providers -- -D warnings
check "clippy (bae-bridge --features oauth-providers,cloudkit)" \
  scripts/cargo.sh clippy -p bae-bridge --features oauth-providers,cloudkit -- -D warnings
# The feature set build-macos.sh ships.
check "clippy (bae-bridge --features oauth-providers,cloudkit,cast)" \
  scripts/cargo.sh clippy -p bae-bridge --features oauth-providers,cloudkit,cast -- -D warnings

# Each crate's production code alone, so an item only its tests use fails as
# dead code. The flag goes to the one crate: RUSTFLAGS would recompile every
# dependency for this one check.
check "dead_code (bae-library lib only)" \
  scripts/cargo.sh rustc -p bae-library --lib --profile check -- -D warnings
check "dead_code (bae-util lib only)" \
  scripts/cargo.sh rustc -p bae-util --lib --profile check -- -D warnings
check "dead_code (bae-audio lib only)" \
  scripts/cargo.sh rustc -p bae-audio --lib --profile check -- -D warnings
check "dead_code (bae-types lib only)" \
  scripts/cargo.sh rustc -p bae-types --lib --profile check -- -D warnings
check "dead_code (bae-catalogs lib only)" \
  scripts/cargo.sh rustc -p bae-catalogs --lib --profile check -- -D warnings
check "dead_code (bae-diagnostics lib only)" \
  scripts/cargo.sh rustc -p bae-diagnostics --lib --profile check -- -D warnings
check "dead_code (bae-app-dir lib only)" \
  scripts/cargo.sh rustc -p bae-app-dir --lib --profile check -- -D warnings
check "dead_code (bae-store lib only)" \
  scripts/cargo.sh rustc -p bae-store --lib --profile check -- -D warnings
check "dead_code (bae-db lib only)" \
  scripts/cargo.sh rustc -p bae-db --lib --profile check -- -D warnings
check "dead_code (bae-preferences lib only)" \
  scripts/cargo.sh rustc -p bae-preferences --lib --profile check -- -D warnings
check "dead_code (bae-track-audio lib only)" \
  scripts/cargo.sh rustc -p bae-track-audio --lib --profile check -- -D warnings
check "dead_code (bae-transfers lib only)" \
  scripts/cargo.sh rustc -p bae-transfers --lib --profile check -- -D warnings
check "dead_code (bae-views lib only)" \
  scripts/cargo.sh rustc -p bae-views --lib --profile check -- -D warnings
check "dead_code (bae-identify lib only)" \
  scripts/cargo.sh rustc -p bae-identify --lib --profile check -- -D warnings
check "dead_code (bae-playback lib only)" \
  scripts/cargo.sh rustc -p bae-playback --lib --profile check -- -D warnings
check "dead_code (bae-import lib only)" \
  scripts/cargo.sh rustc -p bae-import --lib --profile check -- -D warnings
check "dead_code (bae-app lib only)" \
  scripts/cargo.sh rustc -p bae-app --lib --profile check -- -D warnings
check "dead_code (bae-cast lib only)" \
  scripts/cargo.sh rustc -p bae-cast --lib --profile check -- -D warnings

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

check "cargo doc" env RUSTDOCFLAGS="-D warnings" scripts/cargo.sh doc --workspace --no-deps

# ── Rust tests (util + audio + types + catalogs + loc + theme + bridge + automation) ──────────
section "Rust tests"

check "cargo test (bae-util)"          scripts/cargo.sh test -p bae-util
check "cargo test (bae-audio)"         scripts/cargo.sh test -p bae-audio
check "cargo test (bae-types)"         scripts/cargo.sh test -p bae-types
check "cargo test (bae-catalogs)"      scripts/cargo.sh test -p bae-catalogs
check "cargo test (bae-diagnostics)"   scripts/cargo.sh test -p bae-diagnostics
check "cargo test (bae-app-dir)"       scripts/cargo.sh test -p bae-app-dir
check "cargo test (bae-store)"       scripts/cargo.sh test -p bae-store
check "cargo test (bae-db)"       scripts/cargo.sh test -p bae-db
check "cargo test (bae-preferences)" scripts/cargo.sh test -p bae-preferences
check "cargo test (bae-track-audio)"  scripts/cargo.sh test -p bae-track-audio
check "cargo test (bae-transfers)"    scripts/cargo.sh test -p bae-transfers
check "cargo test (bae-views)"      scripts/cargo.sh test -p bae-views
check "cargo test (bae-identify)"     scripts/cargo.sh test -p bae-identify
check "cargo test (bae-playback)"  scripts/cargo.sh test -p bae-playback
check "cargo test (bae-import)"    scripts/cargo.sh test -p bae-import
check "cargo test (bae-app)"     scripts/cargo.sh test -p bae-app --features bae-app/test-utils
check "cargo test (bae-loc)"         scripts/cargo.sh test -p bae-loc
check "cargo test (bae-theme)"         scripts/cargo.sh test -p bae-theme
# Both feature sets: the defaults, and `cast`, which every shipping build turns on.
check "cargo test (bae-bridge --lib)"  scripts/cargo.sh test -p bae-bridge --lib
check "cargo test (bae-bridge --lib, cast)" \
  scripts/cargo.sh test -p bae-bridge --lib --features cast
check "cargo test (bae-automation)"    scripts/cargo.sh test -p bae-automation
check "cargo test (bae-mcp)"           scripts/cargo.sh test -p bae-mcp
check "cargo test (bae-subsonic)"      scripts/cargo.sh test -p bae-subsonic
check "cargo test (bae-cast)"          scripts/cargo.sh test -p bae-cast
check "cargo test (bae-desktop)"       scripts/cargo.sh test -p bae-desktop
# Chrome strings no code references; `core.*` keys are covered by the
# bae-bridge test above.
check "loc chrome orphans"             python3 scripts/loc-chrome-orphans.py
check "loc english skeleton"           python3 scripts/loc-english-skeleton.py
check "loc words"                      python3 scripts/loc-words.py
check "loc words tests"                python3 scripts/tests/loc-words-test.py

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
  env BAE_BRIDGE_FEATURES=cast ./bae-bridge/build-macos.sh
check "install macOS bridge binding (baeium)" \
  ./bae-bridge/install-swift-bindings.sh macos
check "guard: baeium bridge exports no OAuth/CloudKit binding" \
  ./scripts/check-baeium-exports.sh
check "xcodegen (macOS baeium)" bash -c 'cd bae-macos/bae && xcodegen'
check "xcodebuild (macOS baeium)" \
  env BAE_BRIDGE_FEATURES=cast \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    "${XCODE_MACOS_UNSIGNED_ARGS[@]}" "${XCODE_MACOS_PACKAGES_ARGS[@]}" \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution build

check "bridge build" ./bae-bridge/build-macos.sh
check "install macOS bridge binding" \
  ./bae-bridge/install-swift-bindings.sh macos
check "xcodegen" bash -c 'cd bae-macos/bae && xcodegen'
check "xcodebuild" \
  env BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,cast \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    "${XCODE_MACOS_UNSIGNED_ARGS[@]}" "${XCODE_MACOS_PACKAGES_ARGS[@]}" \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution build

check "xcodebuild test (baeTests)" \
  env BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,cast bae-macos/test.sh
check "test-filter checker" python3 scripts/tests/xcresult-tests-ran-test.py

# The Screenshots workflow compiles the scene registry in baeShots, which no
# other macOS gate builds.
check "xcodebuild build-for-testing (baeShots)" \
  env BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,cast \
  xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme baeShots -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    "${XCODE_MACOS_UNSIGNED_ARGS[@]}" "${XCODE_MACOS_PACKAGES_ARGS[@]}" \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution build-for-testing

check "swift-format lint" _swift_format_lint bae-macos/bae/bae
# BaeKit is linted once for both apps.
check "swift-format lint (BaeKit)" _swift_format_lint BaeKit/Sources/BaeKit

check "swiftlint" swiftlint lint --strict --config .swiftlint.yml bae-macos/bae/bae
check "swiftlint (BaeKit)" \
  swiftlint lint --strict --config .swiftlint.yml \
    BaeKit/Sources/BaeKit

check "periphery" bash -c '
  cd bae-macos/bae && periphery scan --strict --skip-build \
    --index-store-path "$1"
' _ "$XCODE_MACOS_UNSIGNED_INDEX"

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
    "${XCODE_IOS_ARGS[@]}" build

check "bridge build" ./bae-bridge/build-ios.sh
check "xcodegen" bash -c 'cd bae-ios/bae && xcodegen'
check "xcodebuild (iphonesimulator)" \
  env BAE_BRIDGE_FEATURES=oauth-providers,cloudkit,cast \
  xcodebuild -project bae-ios/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -sdk iphonesimulator -arch arm64 \
    "${XCODE_IOS_ARGS[@]}" build

check "swift-format lint" _swift_format_lint bae-ios/bae/bae
check "swiftlint" swiftlint lint --strict --config .swiftlint.yml bae-ios/bae/bae
check "periphery" bash -c '
  cd bae-ios/bae && periphery scan --strict --skip-build \
    --index-store-path "$1"
' _ "$XCODE_IOS_INDEX"

# ── Android ────────────────────────────────────────────────────────────────────
section "Android"

check "clippy (Android aarch64)" ./bae-bridge/clippy-android.sh

# This machine's ABI only: CI builds and packages every ABI in bae.abis.
ANDROID_LOCAL_ABI=$(android_host_abi) || exit 1
check "bridge build (Android full)" env BAE_BRIDGE_FEATURES=oauth-providers,cast \
  ./bae-bridge/build-android.sh --abi "$ANDROID_LOCAL_ABI"
check "Gradle unit tests (Android full)" bash -c \
  'cd bae-android && ./gradlew testFullDebugUnitTest --no-daemon'
check "ktlint" ktlint "bae-android/app/src/**/*.kt" "bae-android/fastvalues/src/**/*.kt" "bae-android/screensubscription/src/**/*.kt"
check "detekt" detekt --input bae-android/app/src/main/java,bae-android/fastvalues/src/main/java,bae-android/screensubscription/src/main/java \
  --config bae-android/detekt.yml --build-upon-default-config
check "detekt (type-resolved rules)" detekt --input bae-android/app/src/main/java,bae-android/fastvalues/src/main/java,bae-android/screensubscription/src/main/java \
  --config bae-android/detekt-type-resolved.yml \
  --classpath "$ANDROID_PLATFORM_JAR" --jvm-target "$ANDROID_JAVA_VERSION"
check "Android lint (full)" bash -c \
  'cd bae-android && ./gradlew lintFullDebug --no-daemon'
check "assemble debug APK (full)" bash -c \
  "cd bae-android && ./gradlew assembleFullDebug --no-daemon -Pbae.abi=$ANDROID_LOCAL_ABI"

check "bridge build (Android baeium)" env BAE_BRIDGE_FEATURES=cast \
  ./bae-bridge/build-android.sh --abi "$ANDROID_LOCAL_ABI"
check "Gradle unit tests (Android baeium)" bash -c \
  'cd bae-android && ./gradlew testBaeiumDebugUnitTest --no-daemon'
check "Android lint (baeium)" bash -c \
  'cd bae-android && ./gradlew lintBaeiumDebug --no-daemon'
check "assemble debug APK (baeium)" bash -c \
  "cd bae-android && ./gradlew assembleBaeiumDebug --no-daemon -Pbae.abi=$ANDROID_LOCAL_ABI"

# ── GitHub Actions workflows ───────────────────────────────────────────────────
section "Workflows"

check "actionlint" env SHELLCHECK_OPTS="--severity=error" actionlint

# ── bae-library tests ────────────────────────────────────────────────────────────
section "bae-library tests"
# A test that changes process-wide state runs in a process of its own; the CPU
# test, a binary of its own, runs alone below because it reads process-wide
# getrusage.
check "cargo test (bae-library)" \
  scripts/cargo.sh test -p bae-library --features bae-library/test-utils \
    -- --skip test_playback_cpu
check "cargo test (bae-library playback CPU, release)" \
  scripts/cargo.sh test -p bae-library --release --features bae-library/test-utils \
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
