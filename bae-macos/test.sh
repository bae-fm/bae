#!/usr/bin/env bash
# Runs the macOS app's unit tests (baeTests), all of them or only the named
# Swift test types (`Type` or `Type/test`), and fails when a name ran no tests:
# xcodebuild passes a filter that matches nothing, such as a file name.
#
# Usage: bae-macos/test.sh [<Type>[/<test>] ...]
set -euo pipefail

cd "$(dirname "$0")/.."

# The project lists the test files that existed when it was last generated, so
# generate it from project.yml now: a test file added since would not be built
# and would never run.
(cd bae-macos/bae && xcodegen)

# The scheme's pre-action rebuilds the bridge with these features; unset, it
# would build its full-feature default anyway, so name it to match run.sh.
export BAE_BRIDGE_FEATURES="${BAE_BRIDGE_FEATURES:-oauth-providers,cloudkit,cast}"

ONLY_TESTING=()
for name in "$@"; do
    ONLY_TESTING+=("-only-testing:baeTests/$name")
done

RESULTS="$(mktemp -d)"
trap 'rm -rf "$RESULTS"' EXIT
BUNDLE="$RESULTS/baeTests.xcresult"

status=0
xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    -derivedDataPath bae-macos/bae/.build/derivedData \
    -clonedSourcePackagesDirPath bae-macos/bae/.build/SourcePackages \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution \
    -resultBundlePath "$BUNDLE" \
    ${ONLY_TESTING[@]+"${ONLY_TESTING[@]}"} test || status=$?

# A build that failed before testing leaves no bundle; its own status says so.
if [[ -d "$BUNDLE" ]]; then
    xcrun xcresulttool get test-results tests --path "$BUNDLE" > "$RESULTS/tests.json"
    python3 scripts/xcresult-tests-ran.py "$RESULTS/tests.json" "$@" || status=1
fi
exit "$status"
