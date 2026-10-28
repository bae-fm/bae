#!/usr/bin/env bash
# Fail unless every given FFmpeg dist is the bae-ffmpeg release this tree pins.
# Usage: scripts/check-ffmpeg-dist.sh INSTALLER DIST_DIR...
#
# The installers (scripts/setup-ffmpeg.sh, scripts/build-ffmpeg-ios.sh,
# scripts/build-ffmpeg-android.sh) write the release they installed to
# DIST_DIR/VERSION after checking it. A dist with no VERSION or another release
# is one this tree never checked, so a build must not link it; INSTALLER is the
# command the message tells the developer to run.
set -euo pipefail

if [[ $# -lt 2 ]]; then
    echo "Usage: $0 INSTALLER DIST_DIR..." >&2
    exit 2
fi
installer="$1"
shift

pinned="$(cat "$(dirname "${BASH_SOURCE[0]}")/bae-ffmpeg-release")"
status=0
for dist in "$@"; do
    if [[ ! -f "$dist/VERSION" ]]; then
        echo "$dist holds no recorded bae-ffmpeg release; this tree pins $pinned." >&2
        status=1
    elif [[ "$(cat "$dist/VERSION")" != "$pinned" ]]; then
        echo "$dist holds bae-ffmpeg $(cat "$dist/VERSION"); this tree pins $pinned." >&2
        status=1
    fi
done
if [[ "$status" -ne 0 ]]; then
    echo "Run $installer to install the pinned release." >&2
fi
exit "$status"
