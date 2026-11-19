#!/bin/bash
set -euo pipefail

# Install the bae-ffmpeg fork's prebuilt FFmpeg for this host, unless the dist
# already holds the pinned release.
# Usage: ./scripts/setup-ffmpeg.sh [DIST_DIR]
#
# DIST_DIR defaults to bae-ffmpeg/dist, where .cargo/config.toml points
# FFMPEG_DIR; CI passes its own location. The release is the one
# scripts/bae-ffmpeg-release pins, and scripts/install-ffmpeg-dist.sh checks
# the download and records the release in DIST_DIR/VERSION, which
# scripts/check-ffmpeg-dist.sh compares to the pin before a build links it.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DIST="${1:-$PROJECT_ROOT/bae-ffmpeg/dist}"

ARCH=$(uname -m)
case "$(uname -s)" in
    Darwin) ARCHIVE="ffmpeg-macos-$ARCH.tar.gz"; KIND=macos ;;
    Linux) ARCHIVE="ffmpeg-linux-$ARCH.tar.gz"; KIND=linux ;;
    *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

exec "$SCRIPT_DIR/install-ffmpeg-dist.sh" "$ARCHIVE" "$DIST" "$KIND"
