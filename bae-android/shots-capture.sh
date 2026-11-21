#!/usr/bin/env bash
# Takes the Android shots: the Robolectric tests under
# bae-android/app/src/test/java/fm/bae/app/shots draw each board's real
# composable and write <board>@android.png and <board>@android.json.
# builds shots runs this script from the repo's top (`.app-build`'s
# `shots-capture android` line) with BUILDS_SHOTS_OUT and BUILDS_SHOTS_SELECT,
# which the script passes to Gradle as bae.shotsDir and bae.shotsSelect.
set -euo pipefail

cd "$(dirname "$0")/.."
: "${BUILDS_SHOTS_OUT:?builds shots runs this script and sets BUILDS_SHOTS_OUT}"

./scripts/build-layout.sh
# The tests compile against the full edition's generated Kotlin bindings and
# the localization resources, both products of the bridge build.
if [[ ! -d bae-bridge/kotlin-bindings-full/uniffi ||
    ! -f bae-android/app/src/main/res/values/core_strings.xml ]]; then
    ./bae-bridge/build-android.sh --abi host
fi

properties=("-Pbae.shotsDir=$BUILDS_SHOTS_OUT")
if [[ -n "${BUILDS_SHOTS_SELECT:-}" ]]; then
    properties+=("-Pbae.shotsSelect=$BUILDS_SHOTS_SELECT")
fi
./bae-android/gradle.sh --no-daemon :app:testFullDebugUnitTest --tests 'fm.bae.app.shots.*' \
    "${properties[@]}"
