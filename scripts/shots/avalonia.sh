#!/usr/bin/env bash
# Capture every enabled Avalonia scene and verify each PNG was written.

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "Usage: scripts/shots/avalonia.sh <output-dir>" >&2
    exit 1
fi

ROOT="$(git rev-parse --show-toplevel)"
OUT_DIR="$1"
mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

cd "$ROOT"
dotnet run \
    --project bae-avalonia/bae-avalonia.csproj \
    --framework net8.0 \
    --configuration Debug \
    -- \
    --capture-shots "$OUT_DIR" || {
        capture_exit=$?
        if [[ -f "$OUT_DIR/capture.log" ]]; then
            cat "$OUT_DIR/capture.log" >&2
        fi
        exit "$capture_exit"
    }

platform="macos"
case "$(uname -s)" in
    Linux) platform="linux" ;;
    MINGW*|MSYS*|CYGWIN*) platform="windows" ;;
esac

for scene in \
    story-1-first-run \
    story-3-empty-library \
    import-release-queue \
    import-release-ambiguity-narrow \
    import-release-queue-collapsed \
    import-release-scanning-refresh \
    import-release-resolved-reversal
do
    png="$OUT_DIR/${scene}@${platform}.png"
    if [[ ! -s "$png" ]]; then
        echo "Missing capture: $png" >&2
        exit 1
    fi
done
