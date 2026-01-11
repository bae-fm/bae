#!/bin/bash
# Generate a realistic CUE/FLAC test fixture with a seektable
#
# Creates a ~30 second FLAC file with:
# - Seektable entries (essential for testing byte range calculation)
# - Multiple "tracks" identifiable by frequency
# - Stereo audio at CD quality (44100Hz, 16-bit)

set -e

OUTPUT_DIR="tests/fixtures/cue_flac"
mkdir -p "$OUTPUT_DIR"

SAMPLE_RATE=44100
CHANNELS=2

# Track timings (matching the CUE file we'll generate)
# Track 1: 0:00 - 0:10 (10 seconds)
#   - 0-7.5s: silence (compresses to tiny frames → small min_frame_size)
#   - 7.5-8s: pink noise burst (ensures frame boundary before pregap has different audio)
#   - 8-10s: silence (pregap region for track 2)
# Track 2: 0:10 - 0:20 (10 seconds, with 2-second pregap at 0:08) - white noise
# Track 3: 0:20 - 0:30 (10 seconds) - brown noise
#
# The silence gives min_frame_size ~14 bytes, exposing the CRC validation bug.
# The noise burst at 7.5-8s ensures the auto-advance test case (pregap at 8s) has
# a frame boundary in different audio than the target position.

FLAC_FILE="$OUTPUT_DIR/Test Album.flac"
CUE_FILE="$OUTPUT_DIR/Test Album.cue"
TEMP_WAV="/tmp/cue_flac_test_$$.wav"

if ! command -v ffmpeg &> /dev/null; then
    echo "Error: ffmpeg is required"
    exit 1
fi

echo "Generating 30-second stereo WAV with distinct frequencies per track..."

# Create segments with specific audio for each time region:
# [0] 0-7.5s: silence (for small min_frame_size)
# [1] 7.5-8s: pink noise burst (frame boundary lands here for pregap at 8s)
# [2] 8-10s: silence (pregap region - different from frame boundary audio)
# [3] 10-20s: white noise (Track 2 content)
# [4] 20-30s: brown noise (Track 3 content)
ffmpeg -y -loglevel error \
    -f lavfi -i "anullsrc=r=$SAMPLE_RATE:cl=stereo:d=7.5" \
    -f lavfi -i "anoisesrc=d=0.5:c=pink:r=$SAMPLE_RATE:a=0.5:s=1" \
    -f lavfi -i "anullsrc=r=$SAMPLE_RATE:cl=stereo:d=2" \
    -f lavfi -i "anoisesrc=d=10:c=white:r=$SAMPLE_RATE:a=0.5:s=2" \
    -f lavfi -i "anoisesrc=d=10:c=brown:r=$SAMPLE_RATE:a=0.5:s=3" \
    -filter_complex "[0][1][2][3][4]concat=n=5:v=0:a=1[out];[out]aformat=sample_fmts=s16:channel_layouts=stereo[stereo]" \
    -map "[stereo]" \
    -ar $SAMPLE_RATE \
    "$TEMP_WAV"

echo "Encoding to FLAC with seektable..."

# Encode with flac if available (better seektable control), otherwise ffmpeg
if command -v flac &> /dev/null; then
    # -S 5s = seektable point every 5 seconds (sparse enough to catch byte range bugs)
    # With entries at 0s, 5s, 10s, 15s, 20s, 25s, 30s - track boundaries at 8s and 20s
    # will not align with seektable entries, exposing the "at or before" bug
    flac -f -S 5s -o "$FLAC_FILE" "$TEMP_WAV"
else
    # ffmpeg FLAC encoder also creates seektable for files this size
    ffmpeg -y -loglevel error -i "$TEMP_WAV" -c:a flac "$FLAC_FILE"
fi

rm -f "$TEMP_WAV"

echo "Generating CUE file with pregap on track 2..."

cat > "$CUE_FILE" << 'EOF'
REM Test CUE/FLAC fixture with pregap
PERFORMER "Test Artist"
TITLE "Test Album"
FILE "Test Album.flac" WAVE
  TRACK 01 AUDIO
    TITLE "Track One (Silence)"
    PERFORMER "Test Artist"
    INDEX 01 00:00:00
  TRACK 02 AUDIO
    TITLE "Track Two (White Noise)"
    PERFORMER "Test Artist"
    INDEX 00 00:08:00
    INDEX 01 00:10:00
  TRACK 03 AUDIO
    TITLE "Track Three (Brown Noise)"
    PERFORMER "Test Artist"
    INDEX 01 00:20:00
EOF

echo ""
echo "Generated fixture files:"
ls -lh "$OUTPUT_DIR"

echo ""
echo "FLAC metadata:"
if command -v metaflac &> /dev/null; then
    metaflac --list "$FLAC_FILE" | grep -A 100 "SEEKTABLE" | head -20
else
    echo "(install metaflac to inspect seektable)"
fi

echo ""
echo "CUE contents:"
cat "$CUE_FILE"

