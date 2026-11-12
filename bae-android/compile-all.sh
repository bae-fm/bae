#!/usr/bin/env bash
# Compiles the Android app and its unit-test target without installing
# anything: `bae-android/run.sh` builds and installs the app but compiles no
# test target. The `android` step of `.app-build` runs
# this, after the `android-bridge` step has built the bridge.
set -euo pipefail

exec "$(dirname "$0")/gradle.sh" compileFullDebugKotlin compileFullDebugUnitTestKotlin
