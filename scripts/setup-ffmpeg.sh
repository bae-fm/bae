#!/bin/bash
set -euo pipefail

# Install the bae-ffmpeg fork's prebuilt FFmpeg for this host.
# Usage: ./scripts/setup-ffmpeg.sh [DIST_DIR]
#
# DIST_DIR defaults to bae-ffmpeg/dist, where .cargo/config.toml points
# FFMPEG_DIR; CI passes its own location. The release is the one
# scripts/bae-ffmpeg-release pins. The download is checked for libraries
# outside the dist or the OS before it replaces DIST_DIR, and DIST_DIR/VERSION
# records the release, which scripts/check-ffmpeg-dist.sh compares to the pin
# before a build links it.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
VERSION="$(cat "$SCRIPT_DIR/bae-ffmpeg-release")"
DIST="${1:-$PROJECT_ROOT/bae-ffmpeg/dist}"

ARCH=$(uname -m)
case "$(uname -s)" in
    Darwin) ARCHIVE="ffmpeg-macos-$ARCH.tar.gz"; CHECK_FLAGS=() ;;
    Linux) ARCHIVE="ffmpeg-linux-$ARCH.tar.gz"; CHECK_FLAGS=(--platform linux) ;;
    *) echo "Unsupported OS: $(uname -s)" >&2; exit 1 ;;
esac

mkdir -p "$(dirname "$DIST")"
STAGING="$(mktemp -d "$(dirname "$DIST")/.ffmpeg-dist.XXXXXX")"
trap 'rm -rf "$STAGING"' EXIT

echo "Downloading bae-ffmpeg $VERSION ($ARCHIVE)..."
curl -fL --retry 3 --retry-delay 2 \
    "https://github.com/bae-fm/bae-ffmpeg/releases/download/$VERSION/$ARCHIVE" \
    | tar xz -C "$STAGING"
python3 "$SCRIPT_DIR/check-bundled-libraries.py" ${CHECK_FLAGS[@]+"${CHECK_FLAGS[@]}"} "$STAGING"
printf '%s\n' "$VERSION" > "$STAGING/VERSION"

rm -rf "$DIST"
mv "$STAGING" "$DIST"
chmod 755 "$DIST"
trap - EXIT

echo "bae-ffmpeg $VERSION installed to: $DIST"
