#!/bin/bash
set -euo pipefail

# Install one platform's generated uniffi bindings into the BaeBridge target.
#
# The bindings are per-platform flavors: the macOS build exports the desktop
# bridge functions the iOS build omits. Each flavor is written to its own
# os()-gated file (bae_bridge_<flavor>.swift wrapped in #if os(macOS)/os(iOS)),
# so one worktree can hold both at once and each platform build compiles only
# its own. Compiling for a platform whose flavor file is absent fails at build
# with missing generated types — the same failure mode as a missing-bindings
# build, and only the flows that never built that platform's bridge hit it.

cd "$(dirname "$0")/.."

FLAVOR="${1:-}"
case "$FLAVOR" in
    macos) OS_COND="os(macOS)" ;;
    ios) OS_COND="os(iOS)" ;;
    *)
        echo "Usage: $0 <macos|ios>" >&2
        exit 1
        ;;
esac

SRC="bae-bridge/swift-bindings-$FLAVOR/bae_bridge.swift"
DEST="BaeKit/Sources/BaeBridge/bae_bridge_$FLAVOR.swift"

if [ ! -f "$SRC" ]; then
    echo "Bindings not found: $SRC (run bae-bridge/build-$FLAVOR.sh first)" >&2
    exit 1
fi

mkdir -p "$(dirname "$DEST")"
TMP="${DEST}.tmp.$$"
trap 'rm -f "$TMP"' EXIT

# A test fake or a preview builds an object with `init(noHandle:)`, whose
# handle is 0. uniffi's clone reads a Rust object at that handle when a
# method the fake doesn't override reaches core, and segfaults inside Rust,
# leaving a crash report that reads as an app crash. Each object's
# `uniffiCloneHandle` stops first instead, naming the class and the method
# that reached it (`#function` as a default argument is the caller's). The
# count check fails the install when uniffi's template no longer has the
# line this rewrites.
CLONES=$(grep -c '^    public func uniffiCloneHandle() -> UInt64 {$' "$SRC" || true)
OBJECTS=$(grep -c '^    public init(noHandle: NoHandle) {$' "$SRC" || true)
if [ "$CLONES" -eq 0 ] || [ "$CLONES" -ne "$OBJECTS" ]; then
    echo "install-swift-bindings.sh: expected one uniffiCloneHandle per object in $SRC, found $CLONES for $OBJECTS objects" >&2
    exit 1
fi

# The generated bindings file ends without a trailing newline, so emit an
# explicit blank line before #endif — otherwise it lands inside the final `//`
# comment and never closes the #if.
{
    echo "#if $OS_COND"
    sed 's/^    public func uniffiCloneHandle() -> UInt64 {$/    public func uniffiCloneHandle(_ caller: StaticString = #function) -> UInt64 {\
        precondition(handle != 0, "a handle-less \\(type(of: self)) reached core from \\(caller), which it does not override")/' "$SRC"
    echo ""
    echo "#endif"
} > "$TMP"
mv "$TMP" "$DEST"
trap - EXIT
