#!/usr/bin/env bash
# Runs ./gradlew in bae-android with the toolchain toolchain.sh finds, so the
# lookup of the SDK and JDK is in one place: `bae-android/gradle.sh <gradle
# arguments>`.
set -euo pipefail

cd "$(dirname "$0")"
# shellcheck source=toolchain.sh
source toolchain.sh
exec ./gradlew "$@"
