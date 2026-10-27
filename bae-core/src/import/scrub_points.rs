//! Scrub points, picked from the frames the import's verifying decode reads.
//!
//! A FLAC with no usable SEEKTABLE gives FFmpeg nothing to seek a sample by,
//! so a scrub binary-searches the file, one fetch per step over the cloud. An
//! MP3's frames state no position at all: FFmpeg places a seek into one from
//! its bitrate or its Xing table of contents, an estimate. The verifying
//! decode reads every frame of a FLAC segment from a frame start, and every
//! frame of an MP3 from the stream's first, so each packet it reads carries
//! its frame's exact byte and first sample. A few of those, stored, let a
//! scrub start its decode at a known frame instead.

use crate::db::ScrubPoint;
use crate::playback::data_source::FILL_WINDOW_SIZE;
use bae_audio::{DecodeStart, DecodedPacket, DecodedStream, AVIO_BUFFER_SIZE};
use bae_util::content_type::ContentType;
use tracing::{debug, warn};

/// Most audio between two scrub points.
const SPACING_SECONDS: u64 = 10;

/// Frames FFmpeg's FLAC parser holds before it hands the first of them on:
/// `FLAC_MIN_HEADERS` in `libavcodec/flac_parser.c` (n8.1.2). Reaching a
/// scrub's target frame reads up to this many frames from its start.
const FLAC_PARSER_FRAMES_HELD: u64 = 10;

/// How far back in the frames before it an MP3 frame's main data can begin,
/// in bytes of their main data: `main_data_begin`, 9 bits of an MPEG-1
/// frame's side information (8 of an MPEG-2 frame's). This is the bit
/// reservoir.
const MP3_RESERVOIR_BYTES: u64 = 511;

/// The most bytes of an MP3 frame that hold no main data: the 4-byte header,
/// a 2-byte CRC, and 32 bytes of side information (MPEG-1 stereo's, the most
/// of any layout).
const MP3_FRAME_OVERHEAD: u64 = 38;

/// How the frames of a codec bae stores scrub points for relate to each
/// other.
enum Codec {
    /// Each frame states its own first sample and decodes alone.
    Flac,
    /// No frame states its position, and each leans on the frames before it.
    Mp3,
}

/// One frame the decode read.
struct ReadFrame {
    /// Its first sample, on the stream's own count: an MP3's first frame
    /// begins before sample 0, by the encoder delay a decode drops.
    sample: i64,
    byte: u64,
    size: u64,
}

impl ReadFrame {
    /// The frame as a point a decode from which plays exactly from
    /// `exact_from`; `None` for a frame that begins before sample 0, which a
    /// point can't be.
    fn point(&self, exact_from: i64) -> Option<ScrubPoint> {
        Some(ScrubPoint {
            sample: u64::try_from(self.sample).ok()?,
            byte: self.byte,
            exact_from: u64::try_from(exact_from).ok()?,
        })
    }
}

/// Collects the frames of one segment's decode, then picks its scrub points.
pub(crate) struct ScrubPointTaker {
    codec: Codec,
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
    /// first sample, decoded from a frame start; or an MP3, decoded from the
    /// stream's start. Every other format bae decodes seeks by an index of its
    /// own or by arithmetic.
    pub(crate) fn for_stream(stream: &DecodedStream) -> Option<Self> {
        let codec = match stream.content_type {
            ContentType::Flac => {
                if stream.header_index_passes_start {
                    debug!("the FLAC's seektable indexes it; no scrub points");
                    return None;
                }
                // After a jump into the middle of a frame the first packets
                // carry wrong positions.
                let starts_at_frame = match stream.start {
                    DecodeStart::StreamStart | DecodeStart::Byte(_) | DecodeStart::Frame { .. } => {
                        true
                    }
                    DecodeStart::Sample(sample) => sample == 0,
                };
                if !starts_at_frame {
                    warn!(
                        "a FLAC without a usable seektable decoded from a sample seek; no scrub points"
                    );
                    return None;
                }
                Codec::Flac
            }
            ContentType::Mp3 => {
                // Which samples a frame holds, and what it leans on, show
                // only in a read from the stream's first frame.
                if stream.start != DecodeStart::StreamStart {
                    warn!(
                        "an MP3 decoded from {:?}, not its start; no scrub points",
                        stream.start
                    );
                    return None;
                }
                Codec::Mp3
            }
            _ => return None,
        };
        Some(Self {
            codec,
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
        if self
            .stop_at_sample
            .is_some_and(|stop| u64::try_from(sample).is_ok_and(|sample| sample >= stop))
        {
            return;
        }
        self.frames.push(ReadFrame {
            sample,
            byte,
            size: packet.size,
        });
    }

    /// The segment's scrub points, in sample order.
    pub(crate) fn finish(self) -> Vec<ScrubPoint> {
        if self.unplaced > 0 {
            warn!(
                "{} packets had no byte position or timestamp; scrub points skip them",
                self.unplaced
            );
        }
        let in_order = self.frames.windows(2).all(|pair| {
            pair[1].sample > pair[0].sample && pair[1].byte >= pair[0].byte + pair[0].size
        });
        if !in_order {
            warn!("the decode's frames are out of order; storing no scrub points");
            return Vec::new();
        }
        match self.codec {
            Codec::Flac => self.flac_points(),
            Codec::Mp3 => self.mp3_points(),
        }
    }

    /// A FLAC segment's points: its first frame, then the first frame 10
    /// seconds of audio past the last point, or an earlier one that starts too
    /// far past it for one window to reach -- never the segment's last frame.
    /// A FLAC frame decodes alone, so a decode from a point plays exactly from
    /// the point's own sample.
    fn flac_points(self) -> Vec<ScrubPoint> {
        // A scrub jumps to its point and fetches one fill window from there.
        // To hand over the target's frame, FFmpeg's FLAC parser reads that
        // frame and the frames it holds after it, through the AVIO buffer. So
        // a point serves the frames that start within one window of it, less
        // those held frames at the segment's largest frame size and one AVIO
        // read. A bigger window spaces the points further apart.
        let held = FLAC_PARSER_FRAMES_HELD * self.largest_frame + AVIO_BUFFER_SIZE as u64;
        let reach = FILL_WINDOW_SIZE.saturating_sub(held);
        let spacing = SPACING_SECONDS * u64::from(self.sample_rate);

        let mut points: Vec<&ReadFrame> = Vec::new();
        for frame in &self.frames {
            let due = match points.last() {
                None => true,
                Some(last) => {
                    frame.sample.abs_diff(last.sample) >= spacing || frame.byte - last.byte > reach
                }
            };
            if due {
                points.push(frame);
            }
        }

        // A decode that jumps to a FLAC's last frame can't date it: the parser
        // gives the last frame no timestamp, and nothing before it was read, so
        // the decode can't trim to a target in it. The segment's last frame may
        // be its file's last, so it is never a point; the frame before it, which
        // a jump can date, serves its samples.
        if let [.., before, last] = self.frames.as_slice() {
            if points
                .last()
                .is_some_and(|point| point.sample == last.sample)
            {
                points.pop();
                if points
                    .last()
                    .is_none_or(|point| point.sample < before.sample)
                {
                    points.push(before);
                }
            }
        }
        points
            .into_iter()
            .filter_map(|frame| frame.point(frame.sample))
            .collect()
    }

    /// An MP3's points: for the stream's first frame, then the first frame
    /// each 10 seconds of audio on, the latest frame a decode can start at and
    /// play it exactly, with the first sample that decode plays exactly. A
    /// frame only the stream's own start plays exactly (the first of them)
    /// gets none: a scrub there decodes from the start. bae binds no CUE sheet
    /// to lossy audio, so an MP3 segment is its whole file, and the decode
    /// read it from its first frame. 10 seconds of the highest bitrate an MP3
    /// has, 320 kbps, are 400 KB, a tenth of the window a scrub fetches, so
    /// time alone spaces the points.
    fn mp3_points(self) -> Vec<ScrubPoint> {
        let frames = &self.frames;

        // A frame decodes as it does from the stream's start once the decoder
        // has its main data and the frame before it whole: the earlier
        // frame's output is what the frame overlaps, and its main data can
        // begin up to `MP3_RESERVOIR_BYTES` back in the main data of the
        // frames before it. A decode that starts at frame j has the main data
        // of frame j on, so it plays frame k exactly when frames j to k - 2
        // hold that much main data, counted as each frame's size less the most
        // a frame holds besides. `held[i]` is the main data of the frames
        // before frame i.
        let mut held = Vec::with_capacity(frames.len() + 1);
        held.push(0u64);
        for frame in frames {
            let before = *held.last().expect("held starts with one entry");
            held.push(before + frame.size.saturating_sub(MP3_FRAME_OVERHEAD));
        }
        // For each frame, the latest frame past the stream's first that a
        // decode can start at and play it exactly; `None` when only the
        // stream's own start does. It never moves back from one frame to the
        // next: a later frame's frames before it hold more.
        let mut start = 0;
        let latest: Vec<Option<usize>> = (0..frames.len())
            .map(|frame| {
                let overlapped = frame.checked_sub(1)?;
                while start + 1 < overlapped
                    && held[overlapped] - held[start + 1] >= MP3_RESERVOIR_BYTES
                {
                    start += 1;
                }
                (start >= 1 && held[overlapped] - held[start] >= MP3_RESERVOIR_BYTES)
                    .then_some(start)
            })
            .collect();

        let spacing = SPACING_SECONDS * u64::from(self.sample_rate);
        let mut served: Vec<usize> = Vec::new();
        for (index, frame) in frames.iter().enumerate() {
            let due = served
                .last()
                .is_none_or(|&last| frame.sample.abs_diff(frames[last].sample) >= spacing);
            if due {
                served.push(index);
            }
        }

        let mut points: Vec<ScrubPoint> = Vec::new();
        for target in served {
            let Some(jump) = latest[target] else {
                debug!(
                    sample = frames[target].sample,
                    "only a decode from the MP3's start plays this frame exactly; no point for it"
                );
                continue;
            };
            // The first frame a decode from `jump` plays exactly: the first
            // whose latest start is `jump` or later.
            let exact = latest.partition_point(|latest| latest.is_none_or(|latest| latest < jump));
            let Some(point) = frames[jump].point(frames[exact].sample) else {
                debug!(
                    sample = frames[target].sample,
                    "the frame a decode would start at begins before sample 0; no point for it"
                );
                continue;
            };
            if points.last() != Some(&point) {
                points.push(point);
            }
        }
        points
    }
}
