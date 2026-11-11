#!/usr/bin/env bash
# Capture the Android shots as flat <scene>@android.png files.
#
# Usage: scripts/shots/android.sh <output-dir>
#
# A board's shot (android.*) is the real composable drawn by a Robolectric test with Roborazzi
# (bae-android/app/src/test/java/fm/bae/app/shots): <board>@android.png and its element list
# <board>@android.json. The older scenes below are still @Preview scenes.
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
./scripts/build-layout.sh

source bae-android/toolchain.sh

# The screenshotTest source set compiles against the app's `full` edition, which
# needs the generated uniffi Kotlin bindings and the loc string resources. Both
# are gitignored build products of bae-bridge/build-android.sh (default features
# = oauth-providers → the full edition). Generate them if absent; a checkout that
# already built the Android app has them and skips this.
BINDINGS="$REPO_ROOT/bae-bridge/kotlin-bindings-full/uniffi"
CORE_STRINGS="$REPO_ROOT/bae-android/app/src/main/res/values/core_strings.xml"
if [ ! -d "$BINDINGS" ] || [ ! -f "$CORE_STRINGS" ]; then
    echo "android shots: generating uniffi bindings + loc resources (bae-bridge/build-android.sh)..."
    "$REPO_ROOT/bae-bridge/build-android.sh" --abi "$(android_host_abi)"
fi

cd "$REPO_ROOT/bae-android"

# Wipe any prior rendered references so the per-scene count check below sees only
# this run's output (a removed/renamed scene can't leave a stale PNG behind).
find app/src -type d -path '*screenshotTest*/reference' -prune -exec rm -rf {} + 2>/dev/null || true

echo "android shots: rendering scenes (updateFullDebugScreenshotTest)..."
./gradlew --no-daemon updateFullDebugScreenshotTest

# Scene registry of the old @Preview scenes: <preview-function-name>:<scene-id>. The plugin writes each
# rendered PNG under reference/<package path>/ named "<function>_<hashes>.png",
# so the basename starts with the preview function name. One entry per line,
# sorted by function name, so parallel branches adding scenes merge cleanly.
SCENES=$(cat <<'EOF'
ActivityButtonHeld:activity-button-held
ActivityLaunchProblem:activity-launch-problem
ActivityNeedingAttention:activity-needing-attention
ActivityTransfers:activity-transfers
ActivityTransfersHeld:activity-transfers-held
AlbumDetail:album-detail
AlbumDetailLight:album-detail-light
AlbumPinStates:album-pin-states
AppearanceDarkForest:appearance-dark-forest
AppearanceDarkMidnight:appearance-dark-midnight
AppearanceDarkNeutral:appearance-dark-neutral
AppearanceDarkPlum:appearance-dark-plum
AppearanceDarkSand:appearance-dark-sand
AppearanceDarkSlate:appearance-dark-slate
AppearanceLightForest:appearance-light-forest
AppearanceLightMidnight:appearance-light-midnight
AppearanceLightNeutral:appearance-light-neutral
AppearanceLightPlum:appearance-light-plum
AppearanceLightSand:appearance-light-sand
AppearanceLightSlate:appearance-light-slate
CastPicker:cast-picker
CastPickerLongNames:cast-picker-long-names
CastingReceiverDetails:settings-casting-details
CastingSettingsFound:settings-casting
CastingSettingsLooking:settings-casting-looking
CastingSettingsMenu:settings-casting-menu
DeleteAlbumDialogScene:delete-album-dialog
DownloadEntries:download-entries
FolderChoiceDropbox:folder-choice-dropbox
FolderChoiceOneDrive:folder-choice-onedrive
FullPlayerAllHere:full-player-all-here
FullPlayerPlayingWithAGap:full-player-playing-with-a-gap
FullPlayerWaitingOnStorage:full-player-waiting-on-storage
JoinRequestDialogScene:join-request-dialog
LibraryGrid:library-grid
LibraryGridLight:library-grid-light
LibraryGridScrolled:library-grid-scrolled
LibraryGrouped:library-grouped
LibraryGroupedPinned:library-grouped-pinned
LibraryMoreMenu:library-more-menu
LibrarySettings:settings-library
LibrarySettingsNotSynced:settings-library-not-synced
LibrarySettingsProblem:settings-library-problem
LibraryViewMenu:library-view-menu
MiniPlayerAllHere:mini-player-all-here
MiniPlayerPlayingWithAGap:mini-player-playing-with-a-gap
MiniPlayerWaitingOnStorage:mini-player-waiting-on-storage
NoticeLooks:notice-looks
PeopleAndDevices:people-and-devices
PeopleAndDevicesMember:people-and-devices-member
PeopleAndDevicesPerson:people-and-devices-person
PeopleAndDevicesYours:people-and-devices-yours
PinningList:pinning-list
ResetFailed:reset-failed
ResetUnfinished:reset-unfinished
RestoreCodeMasked:restore-code-masked
RestoreCodeShown:restore-code-shown
S3SetupRegionMismatch:s3-setup-region-mismatch
SettingsAbout:settings-about
SettingsMenu:settings
SettingsMenuOffline:settings-offline
SettingsMenuProblem:settings-problem
SyncStatusFailed:sync-status-failed
SyncStatusOffline:sync-status-offline
SyncStatusSignedOut:sync-status-signed-out
SyncStatusSynced:sync-status-synced
SyncStatusWaiting:sync-status-waiting
Transfers:transfers
TransfersWifiOnly:transfers-wifi-only
EOF
)

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

# The board shots: every board a shot test names must have written its PNG and its element list.
rm -f "$OUT_DIR"/android.*@android.png "$OUT_DIR"/android.*@android.json
echo "android shots: drawing board shots (testFullDebugUnitTest --tests 'fm.bae.app.shots.*')..."
./gradlew --no-daemon :app:testFullDebugUnitTest --tests 'fm.bae.app.shots.*' "-Pbae.shotsDir=$OUT_DIR"
BOARDS=$(grep -rhoE 'shot\("android\.[a-z0-9.-]+"' app/src/test/java/fm/bae/app/shots | sed -E 's/shot\("(.*)"/\1/' | sort)
for board in $BOARDS; do
    for extension in png json; do
        if [ ! -f "$OUT_DIR/$board@android.$extension" ]; then
            echo "android shots: board '$board' wrote no $board@android.$extension" >&2
            exit 1
        fi
    done
done
echo "android shots: wrote $(echo "$BOARDS" | wc -l | tr -d ' ') board shot(s) to $OUT_DIR"
