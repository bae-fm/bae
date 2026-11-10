#!/usr/bin/env bash
# Runs the macOS app's unit tests (baeTests), all of them or only the named
# Swift test types (`Type` or `Type/test`), and fails when a name ran no tests:
# xcodebuild passes a filter that matches nothing, such as a file name.
#
# `--build-only` compiles the app and baeTests into the same tree and stops,
# running nothing: the check after a change to a bridge shape or Swift code,
# which `run.sh` alone misses because it never builds the test target.
#
# `--run-only` runs the tests a `--build-only` run already built, without
# compiling anything: the `macos-tests-run` step of `.app-build` follows the
# `macos-tests` step with it.
#
# Usage: bae-macos/test.sh [--build-only | --run-only] [<Type>[/<test>] ...]
set -euo pipefail

ACTION=test
if [[ "${1:-}" == "--build-only" ]]; then
    if [[ $# -ne 1 ]]; then
        echo "--build-only takes no test names" >&2
        exit 2
    fi
    ACTION=build-for-testing
    shift
elif [[ "${1:-}" == "--run-only" ]]; then
    ACTION=test-without-building
    shift
fi

cd "$(dirname "$0")/.."
./scripts/build-layout.sh
source scripts/xcode-trees.sh

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

# The `macos-tests-run` step runs the unit-test target only; the UI tests run in
# CI and in a plain `test.sh` run.
if [[ "$ACTION" == test-without-building && ${#ONLY_TESTING[@]} -eq 0 ]]; then
    ONLY_TESTING=("-only-testing:baeTests")
fi

RESULTS="$(mktemp -d)"
trap 'rm -rf "$RESULTS"' EXIT
BUNDLE="$RESULTS/baeTests.xcresult"

./bae-macos/ensure-bridge.sh Debug

status=0
xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
    CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
    "${XCODE_MACOS_UNSIGNED_ARGS[@]}" "${XCODE_MACOS_PACKAGES_ARGS[@]}" \
    -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
    -disableAutomaticPackageResolution \
    -resultBundlePath "$BUNDLE" \
    ${ONLY_TESTING[@]+"${ONLY_TESTING[@]}"} "$ACTION" || status=$?

# A build that failed before testing leaves no bundle; its own status says so.
if [[ "$ACTION" != build-for-testing && -d "$BUNDLE" ]]; then
    xcrun xcresulttool get test-results tests --path "$BUNDLE" > "$RESULTS/tests.json"
    python3 scripts/xcresult-tests-ran.py "$RESULTS/tests.json" "$@" || status=1
fi
exit "$status"
