#!/usr/bin/env bash
# Runs the macOS app's tests: with no names, the unit tests (baeTests) and the UI
# tests (baeUITests); with names, only the named baeTests Swift test types
# (`Type` or `Type/test`). Fails when a name ran no tests: xcodebuild passes a
# filter that matches nothing, such as a file name.
#
# `--build-only` compiles the app and the test targets into the same tree and
# stops, running nothing: the check after a change to a bridge shape or Swift code,
# which `run.sh` alone misses because it never builds the test target.
#
# `--run-only` runs the tests a `--build-only` run already built, without
# compiling anything: the `macos-tests-run` step of `.app-build` follows the
# `macos-tests` step with it.
#
# Every test has a minute to finish (`-test-timeouts-enabled`): a test that
# runs past it is killed, reported failed by name, and the run goes on with the
# rest. XCTest rounds an allowance up to whole minutes, so 60 is the least there
# is. A test that legitimately needs longer sets `executionTimeAllowance` itself
# (the UI tests would, in `setUp`).
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
# CI and in a plain `test.sh` run (the `macos-tests-all` step).
if [[ "$ACTION" == test-without-building && ${#ONLY_TESTING[@]} -eq 0 ]]; then
    ONLY_TESTING=("-only-testing:baeTests")
fi

RESULTS="$(mktemp -d)"
trap 'rm -rf "$RESULTS"' EXIT
BUNDLE="$RESULTS/baeTests.xcresult"

./bae-macos/ensure-bridge.sh Debug

xcodebuild_tests() {
    xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug \
        CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO \
        "${XCODE_MACOS_UNSIGNED_ARGS[@]}" "${XCODE_MACOS_PACKAGES_ARGS[@]}" \
        -scmProvider system -disablePackageRepositoryCache -skipPackageUpdates \
        -disableAutomaticPackageResolution "$@"
}

# Xcode makes the UI tests' runner by copying its own XCTRunner app, which Apple
# signed, and adding the test bundle, its frameworks and an Info.plist. With
# signing off it never signs the result, so the runner's signature no longer
# matches its contents. Files built on the person's Mac carry macOS's
# provenance mark, so Gatekeeper checks the runner as it launches, finds an
# Apple-signed app whose contents changed, and holds it at a prompt saying it is
# damaged: the runner never gets past dyld, and xcodebuild reports it "hung
# before establishing connection". Signed ad hoc, as the linker signs bae.app,
# the runner launches.
#
# Once an Apple-signed runner has been launched, macOS refuses codesign's writes
# into it ("System Policy: codesign deny file-write-create" in the unified log),
# so the runner is deleted before the build and Xcode's fresh copy is signed.
UI_TEST_RUNNER="$XCODE_MACOS_UNSIGNED_TREE/Build/Products/Debug/baeUITests-Runner.app"

status=0
if [[ "$ACTION" != test-without-building ]]; then
    rm -rf "$UI_TEST_RUNNER"
    xcodebuild_tests build-for-testing
    codesign --force --sign - "$UI_TEST_RUNNER"
fi
if [[ "$ACTION" != build-for-testing ]]; then
    xcodebuild_tests -resultBundlePath "$BUNDLE" \
        -test-timeouts-enabled YES -default-test-execution-time-allowance 60 \
        ${ONLY_TESTING[@]+"${ONLY_TESTING[@]}"} test-without-building || status=$?
fi

# A build that failed before testing leaves no bundle; its own status says so.
if [[ "$ACTION" != build-for-testing && -d "$BUNDLE" ]]; then
    xcrun xcresulttool get test-results tests --path "$BUNDLE" > "$RESULTS/tests.json"
    python3 scripts/xcresult-tests-ran.py "$RESULTS/tests.json" "$@" || status=1
fi
exit "$status"
