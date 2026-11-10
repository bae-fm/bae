#!/usr/bin/env bash
# Runs the Android app's unit tests (testFullDebugUnitTest) from the output
# `compile-all.sh` built, extra arguments going to Gradle, such as
# `--tests '*SyncScreenTest'`. The `android-tests` step of `.app-build` runs
# this, after the `android` step.
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck source=toolchain.sh
source toolchain.sh
./gradlew testFullDebugUnitTest "$@"
