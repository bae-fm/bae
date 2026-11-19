#!/usr/bin/env bash
# Install one dist of the bae-ffmpeg release this tree pins into DIST, unless
# DIST already holds that release.
# Usage: scripts/install-ffmpeg-dist.sh ARCHIVE DIST KIND
#
# ARCHIVE is the release's file, such as ffmpeg-linux-x86_64.tar.gz. KIND says
# how to check the download: macos, linux or android dists hold shared
# libraries, which scripts/check-bundled-libraries.py checks for libraries
# outside the dist or the OS; ios dists hold static archives, which load
# nothing. The release is the one scripts/bae-ffmpeg-release pins.
#
# The download is unpacked beside DIST, checked, stamped with its release in
# VERSION and renamed into place, so no build ever reads a partly written
# dist. A dist that holds the pinned release is never replaced: when another
# install of the same release puts its dist in place first, this one deletes
# its own download. A DIST that holds another release is replaced.
set -euo pipefail

if [[ $# -ne 3 ]]; then
    echo "Usage: $0 ARCHIVE DIST KIND" >&2
    exit 2
fi
archive="$1"
dist="$2"
kind="$3"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
version="$(cat "$script_dir/bae-ffmpeg-release")"

holds_pin() {
    [[ -f "$dist/VERSION" && "$(cat "$dist/VERSION")" == "$version" ]]
}

if holds_pin; then
    exit 0
fi

case "$kind" in
    macos) check=(python3 "$script_dir/check-bundled-libraries.py") ;;
    linux | android) check=(python3 "$script_dir/check-bundled-libraries.py" --platform "$kind") ;;
    ios) check=() ;;
    *) echo "$0: unknown kind $kind" >&2; exit 2 ;;
esac

mkdir -p "$(dirname "$dist")"
staging="$(mktemp -d "$(dirname "$dist")/.ffmpeg-dist.XXXXXX")"
trap 'rm -rf "$staging"' EXIT

echo "Downloading bae-ffmpeg $version ($archive)..."
curl -fL --retry 3 --retry-delay 2 \
    "https://github.com/bae-fm/bae-ffmpeg/releases/download/$version/$archive" \
    | tar xz -C "$staging"
if ! compgen -G "$staging/lib/libavcodec.*" >/dev/null; then
    echo "$archive holds no lib/libavcodec" >&2
    exit 1
fi
if [[ ${#check[@]} -gt 0 ]]; then
    "${check[@]}" "$staging"
fi
printf '%s\n' "$version" > "$staging/VERSION"
chmod 755 "$staging"

if [[ -e "$dist" ]] && ! holds_pin; then
    rm -rf "$dist"
fi
# rename(2) puts the dist in place at once, and fails when another install
# put a dist there after the check above.
if python3 -c 'import os, sys; os.rename(sys.argv[1], sys.argv[2])' "$staging" "$dist" 2>/dev/null; then
    trap - EXIT
    echo "bae-ffmpeg $version installed to: $dist"
elif holds_pin; then
    echo "bae-ffmpeg $version was already installed to: $dist"
else
    echo "$0: could not put $staging in place as $dist" >&2
    exit 1
fi
