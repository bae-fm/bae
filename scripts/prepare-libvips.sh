#!/usr/bin/env bash
# Points this checkout's bae-libvips at the folder of this machine's libvips
# store that holds the release scripts/bae-libvips-release pins, and builds
# this machine's dist there when it is missing, as scripts/prepare-ffmpeg.sh
# does for FFmpeg. Only a Mac builds a desktop dist here, and other hosts link
# no libvips; the Android dists come from scripts/build-libvips-android.sh into
# the same folder.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(cat scripts/bae-libvips-release)"
folder="${XDG_CACHE_HOME:-$HOME/.cache}/bae-libvips/$version"
mkdir -p "$folder"

if [[ "$(uname -s)" == Darwin ]]; then
    scripts/build-libvips.sh macos "$folder/dist"
fi
if [[ "$(readlink bae-libvips || true)" != "$folder" ]]; then
    ln -sfn "$folder" bae-libvips
fi
