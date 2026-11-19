#!/usr/bin/env bash
# Points this checkout's bae-ffmpeg at the folder of this machine's FFmpeg
# store that holds the release scripts/bae-ffmpeg-release pins, and installs
# this machine's desktop dist there when it is missing. builds runs it (the
# `prepare` line of .build-dirs) when it sets up the checkout and before cargo
# on whichever machine runs cargo. Each release has its own folder in the
# store, so checkouts on different pins never read or replace each other's
# dists. The Android and iOS bridge builds install their dists into the same
# folder through the link (scripts/build-ffmpeg-android.sh,
# scripts/build-ffmpeg-ios.sh), and builds deletes each folder that no
# checkout's pin names (the `installed` line of .build-dirs).
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(cat scripts/bae-ffmpeg-release)"
folder="${XDG_CACHE_HOME:-$HOME/.cache}/bae-ffmpeg/$version"

if [[ -d bae-ffmpeg && ! -L bae-ffmpeg ]]; then
    # A checkout whose bae-ffmpeg is a folder of its own keeps it, and only
    # checks that it holds the pinned release.
    exec scripts/check-ffmpeg-dist.sh scripts/setup-ffmpeg.sh bae-ffmpeg/dist
fi

scripts/setup-ffmpeg.sh "$folder/dist"
if [[ "$(readlink bae-ffmpeg || true)" != "$folder" ]]; then
    ln -sfn "$folder" bae-ffmpeg
fi
