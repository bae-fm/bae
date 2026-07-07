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
# The generated bindings file ends without a trailing newline, so emit an
# explicit blank line before #endif — otherwise it lands inside the final `//`
# comment and never closes the #if.
{
    echo "#if $OS_COND"
    cat "$SRC"
    echo ""
    echo "#endif"
} > "$DEST"
