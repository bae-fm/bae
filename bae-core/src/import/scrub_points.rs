//! Scrub points, picked from the frames the import's verifying decode reads.
//!
//! A FLAC with no usable SEEKTABLE gives FFmpeg nothing to seek a sample by,
//! so a scrub binary-searches the file, one fetch per step over the cloud. The
//! verifying decode reads every frame of a segment from a frame start, so each
//! packet it reads carries its frame's exact byte and first sample. A few of
//! those, stored, let a scrub jump by byte instead.

use crate::audio_codec::{DecodedPacket, DecodedStream, AVIO_BUFFER_SIZE};
use crate::db::ScrubPoint;
use crate::playback::sparse_buffer::FILL_WINDOW_SIZE;
use bae_util::content_type::ContentType;
use tracing::{debug, warn};

/// Most audio between two scrub points.
const SPACING_SECONDS: u64 = 10;

/// Frames FFmpeg's FLAC parser holds before it hands the first of them on:
/// `FLAC_MIN_HEADERS` in `libavcodec/flac_parser.c` (n8.1.2). Reaching a
/// scrub's target frame reads up to this many frames from its start.
const FLAC_PARSER_FRAMES_HELD: u64 = 10;

/// One frame the decode read.
struct ReadFrame {
    start: ScrubPoint,
    size: u64,
}

/// Collects the frames of one segment's decode, then picks its scrub points.
pub(crate) struct ScrubPointTaker {
    sample_rate: u32,
    /// The segment's end: frames from here on belong to what follows it.
    stop_at_sample: Option<u64>,
    frames: Vec<ReadFrame>,
    /// Packets without a byte position or a timestamp, which can't be points.
    unplaced: u32,
    largest_frame: u64,
}

impl ScrubPointTaker {
    /// The taker for a segment's decode of `stream`, when its file needs
    /// scrub points: a FLAC whose header gave FFmpeg no seek index past the
    /// first sample, decoded from a frame start. Every other format bae
    /// decodes seeks by an index of its own or by arithmetic, or (an MP3 with
    /// no table of contents) is left to FFmpeg's search.
    pub(crate) fn for_stream(stream: &DecodedStream) -> Option<Self> {
        if stream.content_type != ContentType::Flac {
            return None;
        }
        if stream.header_index_passes_start {
            debug!("the FLAC's seektable indexes it; no scrub points");
            return None;
        }
        if !stream.starts_at_frame {
            warn!("a FLAC without a usable seektable decoded from a sample seek; no scrub points");
            return None;
        }
        Some(Self {
            sample_rate: stream.sample_rate,
            stop_at_sample: stream.stop_at_sample,
            frames: Vec::new(),
            unplaced: 0,
            largest_frame: 0,
        })
    }

    pub(crate) fn observe(&mut self, packet: DecodedPacket) {
        self.largest_frame = self.largest_frame.max(packet.size);
        let (Some(sample), Some(byte)) = (packet.sample, packet.byte) else {
            self.unplaced += 1;
            return;
        };
        if self.stop_at_sample.is_some_and(|stop| sample >= stop) {
            return;
        }
        self.frames.push(ReadFrame {
            start: ScrubPoint { sample, byte },
            size: packet.size,
        });
    }

    /// The segment's scrub points: its first frame, then the first frame 10
    /// seconds of audio past the last point, or an earlier one that starts too
    /// far past it for one window to reach -- never the segment's last frame.
    pub(crate) fn finish(self) -> Vec<ScrubPoint> {
        if self.unplaced > 0 {
            warn!(
                "{} packets had no byte position or timestamp; scrub points skip them",
                self.unplaced
            );
        }
        let in_order = self.frames.windows(2).all(|pair| {
            pair[1].start.sample > pair[0].start.sample
                && pair[1].start.byte >= pair[0].start.byte + pair[0].size
        });
        if !in_order {
            warn!("the decode's frames are out of order; storing no scrub points");
            return Vec::new();
        }

        // A scrub jumps to its point and fetches one fill window from there.
        // To hand over the target's frame, FFmpeg's FLAC parser reads that
        // frame and the frames it holds after it, through the AVIO buffer. So
        // a point serves the frames that start within one window of it, less
        // those held frames at the segment's largest frame size and one AVIO
        // read. A bigger window spaces the points further apart.
        let held = FLAC_PARSER_FRAMES_HELD * self.largest_frame + AVIO_BUFFER_SIZE as u64;
        let reach = FILL_WINDOW_SIZE.saturating_sub(held);
        let spacing = SPACING_SECONDS * u64::from(self.sample_rate);

        let mut points: Vec<ScrubPoint> = Vec::new();
        for frame in &self.frames {
            let due = match points.last() {
                None => true,
                Some(last) => {
                    frame.start.sample - last.sample >= spacing
                        || frame.start.byte - last.byte > reach
                }
            };
            if due {
                points.push(frame.start);
            }
        }

        // A decode that jumps to a FLAC's last frame can't date it: the parser
        // gives the last frame no timestamp, and nothing before it was read, so
        // the decode can't trim to a target in it. The segment's last frame may
        // be its file's last, so it is never a point; the frame before it, which
        // a jump can date, serves its samples.
        if let [.., before, last] = self.frames.as_slice() {
            if points.last() == Some(&last.start) {
                points.pop();
                if points
                    .last()
                    .is_none_or(|point| point.sample < before.start.sample)
                {
                    points.push(before.start);
                }
            }
        }
        points
    }
}
