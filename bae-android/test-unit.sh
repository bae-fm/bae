#!/usr/bin/env bash
# Runs the Android app's unit tests (testFullDebugUnitTest) from the output
# `compile-all.sh` built, extra arguments going to Gradle, such as
# `--tests '*SyncScreenTest'`. The `android-tests` step of `.app-build` runs
# this, after the `android` step.
#
# A test that runs past a minute has its test JVM killed by the watchdog in
# build.gradle.kts, which names the test; `--continue` lets the other modules'
# tests run after that module's task fails.
set -euo pipefail

exec "$(dirname "$0")/gradle.sh" --continue testFullDebugUnitTest "$@"
