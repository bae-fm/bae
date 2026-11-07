#!/usr/bin/env bash
# The one definition of where the apps' Xcode builds go. Source it for the
# paths and the xcodebuild arguments that name them; run it with a tree's name
# (`scripts/xcode-trees.sh macos-run`) to print a tree's path, for a script that
# is not shell. No other file passes -derivedDataPath
# (scripts/check-xcode-trees.py).
#
# The macOS app has two trees because its two kinds of build differ in what
# they produce, not only in where:
#   macos-run       bae-macos/run.sh: signed with the person's identity and
#                   provisioning, bundle id fm.bae.desktop, a plist that makes
#                   the app a Dock app. Holds the app the person launches, and
#                   what bae-avalonia/run.sh and `builds test-app` read from
#                   it.
#   macos-unsigned  every other build of the app: scripts/check.sh,
#                   bae-macos/test.sh, the screenshot scenes, and CI and the
#                   release workflow's builds. CODE_SIGNING_ALLOWED=NO, bundle id
#                   fm.bae.desktop.xcode (project.yml's Debug default), a plist
#                   generated as an agent app. A test or a check build would
#                   otherwise replace run.sh's bundle with an unsigned one.
# Swift packages resolve once per checkout into macos-packages, which every
# xcodebuild of the app shares: each tree would otherwise unpack its own copy of
# Sentry's binary frameworks, 2.8 GB.
#
# The builds tool ages each tree on its own (an Xcode tree nothing built into
# for three days is deleted), so a tree added here is aged with no other edit.

_xcode_trees_repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Each tree is named by its real path: <folder> (a `.build` the builds tool
# links to the build volume) resolved through its links, then <leaf>.
# xcodebuild resolves the links in a -derivedDataPath that exists and keeps one
# that doesn't yet as given, so a tree named by its linked path was recorded
# under one path by the build that created it and another by the next; every
# compile command names the tree, so that next build recompiled all of it (889
# Swift files for test.sh's tree). A folder that doesn't exist (CI, before the
# links) is named as given.
_xcode_trees_real() {
    local folder="$1" leaf="$2" real
    if real="$(cd "$folder" 2>/dev/null && pwd -P)"; then
        printf '%s/%s\n' "$real" "$leaf"
    else
        printf '%s/%s\n' "$folder" "$leaf"
    fi
}

# BAE_MACOS_RUN_TREE is overridden by bae-macos/tests/run-isolation.sh, which
# runs run.sh against a stub xcodebuild and must not touch the real tree.
XCODE_MACOS_RUN_TREE="${BAE_MACOS_RUN_TREE:-$(_xcode_trees_real "$_xcode_trees_repo/bae-macos/bae/.build" runApp)}"
XCODE_MACOS_UNSIGNED_TREE="$(_xcode_trees_real "$_xcode_trees_repo/bae-macos/bae/.build" unsigned)"
XCODE_MACOS_PACKAGES="$(_xcode_trees_real "$_xcode_trees_repo/bae-macos/bae/.build" SourcePackages)"
XCODE_IOS_TREE="$(_xcode_trees_real "$_xcode_trees_repo/bae-ios/bae/.build" derivedData)"

XCODE_MACOS_RUN_ARGS=(-derivedDataPath "$XCODE_MACOS_RUN_TREE")
XCODE_MACOS_UNSIGNED_ARGS=(-derivedDataPath "$XCODE_MACOS_UNSIGNED_TREE")
XCODE_MACOS_PACKAGES_ARGS=(-clonedSourcePackagesDirPath "$XCODE_MACOS_PACKAGES")
XCODE_IOS_ARGS=(-derivedDataPath "$XCODE_IOS_TREE")

# Where the build index of each tree lives, for periphery and the environment
# audit.
XCODE_MACOS_UNSIGNED_INDEX="$XCODE_MACOS_UNSIGNED_TREE/Index.noindex/DataStore"
XCODE_IOS_INDEX="$XCODE_IOS_TREE/Index.noindex/DataStore"

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    case "${1:-}" in
        macos-run) printf '%s\n' "$XCODE_MACOS_RUN_TREE" ;;
        macos-unsigned) printf '%s\n' "$XCODE_MACOS_UNSIGNED_TREE" ;;
        macos-packages) printf '%s\n' "$XCODE_MACOS_PACKAGES" ;;
        ios) printf '%s\n' "$XCODE_IOS_TREE" ;;
        *)
            echo "usage: $0 macos-run|macos-unsigned|macos-packages|ios" >&2
            exit 1
            ;;
    esac
fi
