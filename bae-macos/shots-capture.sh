#!/usr/bin/env bash
# Takes the Mac shots: builds the bridge and the Xcode project, then runs the
# baeShots test bundle, whose CaptureSceneTests draws each scene and writes
# <board>@macos.png and <board>@macos.json. builds shots runs this script from
# the repo's top (`.app-build`'s `shots-capture mac` line) with
# BUILDS_SHOTS_OUT, BUILDS_SHOTS_SELECT and BUILDS_SHOTS_WINDOWS, and with a
# TEST_RUNNER_ copy of each, which xcodebuild passes on to the test process
# under the plain name.
#
# bae-bridge/build-macos.sh builds the bridge and installs its Swift bindings,
# so the script runs it once and tells xcodebuild to skip the scheme's
# pre-action (BAE_SKIP_RUST_BRIDGE=YES), which would run it again.
set -euo pipefail

cd "$(dirname "$0")/.."
./scripts/build-layout.sh
./bae-bridge/build-macos.sh
(cd bae-macos/bae && xcodegen)
source scripts/xcode-trees.sh

cd bae-macos/bae
xcodebuild -project bae.xcodeproj -scheme baeShots -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO BAE_SKIP_RUST_BRIDGE=YES \
    "${XCODE_MACOS_SHOTS_ARGS[@]}" "${XCODE_MACOS_PACKAGES_ARGS[@]}" test
