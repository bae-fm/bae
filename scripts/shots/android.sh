#!/usr/bin/env bash
# Capture the Android screenshot scenes as flat <scene>@android.png files.
#
# Usage: scripts/shots/android.sh <output-dir>
#
# Renders the @Preview scenes in bae-android/app/src/screenshotTest via the
# Compose Preview Screenshot Testing plugin (layoutlib on the JVM — no emulator
# or device). Each scene function renders a session-free composition from the
# production UI; this script runs the plugin's update task, then copies each
# rendered PNG to the contract name. Any scene that fails to render, or renders
# to anything other than exactly one PNG, fails the whole script.
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <output-dir>" >&2
    exit 2
fi

# Absolute output dir (resolved before we cd into the repo).
OUT_DIR=$(mkdir -p "$1" && cd "$1" && pwd)

cd "$(dirname "$0")/../.."
REPO_ROOT=$(pwd)

export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
export JAVA_HOME="${JAVA_HOME:-/Applications/Android Studio.app/Contents/jbr/Contents/Home}"

# The screenshotTest source set compiles against the app's `full` edition, which
# needs the generated uniffi Kotlin bindings and the loc string resources. Both
# are gitignored build products of bae-bridge/build-android.sh (default features
# = oauth-providers → the full edition). Generate them if absent; a checkout that
# already built the Android app has them and skips this.
BINDINGS="$REPO_ROOT/bae-bridge/kotlin-bindings-full/uniffi"
CORE_STRINGS="$REPO_ROOT/bae-android/app/src/main/res/values/core_strings.xml"
if [ ! -d "$BINDINGS" ] || [ ! -f "$CORE_STRINGS" ]; then
    echo "android shots: generating uniffi bindings + loc resources (bae-bridge/build-android.sh)..."
    "$REPO_ROOT/bae-bridge/build-android.sh"
fi

cd "$REPO_ROOT/bae-android"

# Wipe any prior rendered references so the per-scene count check below sees only
# this run's output (a removed/renamed scene can't leave a stale PNG behind).
find app/src -type d -path '*screenshotTest*/reference' -prune -exec rm -rf {} + 2>/dev/null || true

echo "android shots: rendering scenes (updateFullDebugScreenshotTest)..."
./gradlew --no-daemon updateFullDebugScreenshotTest

# Scene registry: <preview-function-name>:<scene-id>. The plugin writes each
# rendered PNG under reference/<package path>/ named "<function>_<hashes>.png",
# so the basename starts with the preview function name.
SCENES="Welcome:welcome LibraryGrid:library-grid AlbumDetail:album-detail"

for entry in $SCENES; do
    func="${entry%%:*}"
    scene="${entry##*:}"
    matches=$(find app/src -type f -path '*screenshotTest*/reference/*' -name "${func}_*.png")
    count=$(printf '%s' "$matches" | grep -c . || true)
    if [ "$count" -ne 1 ]; then
        echo "android shots: scene '$scene' (function $func) rendered $count PNG(s), expected 1" >&2
        [ -n "$matches" ] && printf '  %s\n' $matches >&2
        exit 1
    fi
    cp "$matches" "$OUT_DIR/${scene}@android.png"
    echo "android shots: $scene -> ${scene}@android.png"
done

echo "android shots: wrote $(echo "$SCENES" | wc -w | tr -d ' ') scene(s) to $OUT_DIR"
