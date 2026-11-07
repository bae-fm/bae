#!/usr/bin/env bash
# Compiles the Android app and its unit-test and screenshot-test targets
# without installing anything: `bae-android/run.sh` builds and installs the app
# but compiles neither test target. The `android` step of `.app-build` runs
# this, after the `android-bridge` step has built the bridge.
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck source=toolchain.sh
source toolchain.sh
./gradlew compileFullDebugKotlin compileFullDebugUnitTestKotlin compileFullDebugScreenshotTestKotlin
