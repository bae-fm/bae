#!/usr/bin/env bash
# Points this checkout's bae-libvips at the folder of this machine's libvips
# store that holds the release scripts/bae-libvips-release pins, and builds
# this machine's dist there when it is missing, as scripts/prepare-ffmpeg.sh
# does for FFmpeg. A Mac and a Linux machine build their desktop dist here; CI
# builds the Windows and Linux dists through .github/actions/setup-libvips. The
# Android dists come from scripts/build-libvips-android.sh into the same
# folder, which calls this with --folder-only to get the folder and build no
# desktop dist.
set -euo pipefail
cd "$(dirname "$0")/.."

version="$(cat scripts/bae-libvips-release)"
folder="${XDG_CACHE_HOME:-$HOME/.cache}/bae-libvips/$version"
mkdir -p "$folder"

if [[ "${1:-}" != --folder-only ]]; then
    case "$(uname -s)" in
        Darwin) scripts/build-libvips.sh macos "$folder/dist" ;;
        Linux) scripts/build-libvips.sh linux "$folder/dist" ;;
    esac
fi
if [[ "$(readlink bae-libvips || true)" != "$folder" ]]; then
    ln -sfn "$folder" bae-libvips
fi
