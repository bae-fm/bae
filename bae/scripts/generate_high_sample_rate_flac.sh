#!/bin/bash
# Generate high sample rate (96kHz) FLAC fixture for testing sample rate handling

OUTPUT_DIR="tests/fixtures/flac"
mkdir -p "$OUTPUT_DIR"

SAMPLE_RATE=96000
DURATION=3  # 3 seconds
FREQ=440

if command -v ffmpeg &> /dev/null; then
    echo "Generating 96kHz FLAC fixture..."
    ffmpeg -f lavfi -i "sine=frequency=$FREQ:duration=$DURATION" \
           -ar $SAMPLE_RATE -ac 2 -sample_fmt s16 -f flac \
           "$OUTPUT_DIR/96khz_test.flac" -y -loglevel error
    
    echo "Generated: $OUTPUT_DIR/96khz_test.flac"
    ffprobe -show_streams "$OUTPUT_DIR/96khz_test.flac" 2>&1 | grep sample_rate
else
    echo "Error: ffmpeg not available"
    exit 1
fi
