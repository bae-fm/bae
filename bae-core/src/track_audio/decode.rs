//! A track's decode window over its sources' buffers: the one shape playback,
//! a save's re-encode, import's loudness pass and a transcode decode by.

use bae_audio::sparse_buffer::{BufferStop, SharedSparseBuffer};
use bae_audio::track_stream::TrackSink;
use bae_audio::DecodeError;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// One track's decode window — the one shape both playback and the save
/// re-encoder run: its segments in play order, preceded by
/// `leading_silence_frames` of generated silence (a CUE `PREGAP` directive) and
/// followed by `trailing_silence_frames` (a save relocating the next track's
/// generated pregap; always 0 for playback).
#[derive(Clone)]
pub(crate) struct StreamDecodeParams {
    segments: Vec<SegmentDecodeParams>,
    /// Whether the demuxer may jump by byte to a segment's recorded start_byte:
    /// whether the codec's frames keep their place after a byte jump
    /// (`bae_audio::byte_jump_keeps_place`). The rest seek by sample.
    byte_seekable: bool,
    leading_silence_frames: u64,
    trailing_silence_frames: u64,
}

/// One decoder window: the segment's span within its backing file, the buffer
/// its bytes stream through, the sample offset into the segment where output
/// starts (a mid-track seek; 0 for a natural start), and where the decode
/// starts reading. The lead-in trim target derives from these via
/// [`Self::target_sample`], the demuxer start via [`Self::decode_start`].
#[derive(Clone)]
pub(crate) struct SegmentDecodeParams {
    buffer: SharedSparseBuffer,
    span: bae_db::SegmentSpan,
    start_offset: u64,
    start: SegmentStart,
}

/// Where a segment's decode starts reading its file.
#[derive(Clone, Copy)]
enum SegmentStart {
    /// From what the span records: its landing byte at a natural start of a
    /// codec whose frames keep their place after a byte jump, else a seek by
    /// sample to the target.
    Span,
    /// A stored scrub point that plays the target exactly.
    ScrubPoint(bae_db::ScrubPoint),
    /// The stream's first frame: an MP3 target before every stored point's
    /// exact start.
    StreamStart,
}

impl SegmentDecodeParams {
    /// A window with no scrub point to start at: a natural start jumps to the
    /// span's landing byte, a mid-track seek seeks by sample.
    pub(crate) fn new(
        buffer: SharedSparseBuffer,
        span: bae_db::SegmentSpan,
        start_offset: u64,
    ) -> Self {
        Self { buffer, span, start_offset, start: SegmentStart::Span }
    }

    /// A start `start_offset` samples into the segment that decodes from
    /// `point`, a stored frame that plays the target exactly, forward to the
    /// target.
    pub(crate) fn from_scrub_point(
        buffer: SharedSparseBuffer,
        span: bae_db::SegmentSpan,
        start_offset: u64,
        point: bae_db::ScrubPoint,
    ) -> Self {
        Self { buffer, span, start_offset, start: SegmentStart::ScrubPoint(point) }
    }

    /// A start `start_offset` samples into the segment that decodes from the
    /// stream's first frame forward to the target.
    pub(crate) fn from_stream_start(
        buffer: SharedSparseBuffer,
        span: bae_db::SegmentSpan,
        start_offset: u64,
    ) -> Self {
        Self { buffer, span, start_offset, start: SegmentStart::StreamStart }
    }

    /// First sample to emit: the segment's start plus the seek offset. FFmpeg
    /// trims the lead-in the seek lands before this (a frame may begin before
    /// it), so the first output sample is exact.
    pub(crate) fn target_sample(&self) -> u64 {
        self.span.start_sample + self.start_offset
    }

    /// Where the demuxer starts: a scrub point's frame, the stream's start,
    /// or the recorded landing byte at a natural start (`start_offset == 0`)
    /// of a byte-seekable codec. Any other start, and APE, sample-seeks to
    /// [`Self::target_sample`] instead.
    pub(crate) fn decode_start(&self, byte_seekable: bool) -> bae_audio::DecodeStart {
        use bae_audio::DecodeStart;
        match self.start {
            SegmentStart::ScrubPoint(point) => {
                DecodeStart::Frame { byte: point.byte, sample: point.sample }
            }
            SegmentStart::StreamStart => DecodeStart::StreamStart,
            SegmentStart::Span => match self.span.start_byte {
                Some(byte) if self.start_offset == 0 && byte_seekable => DecodeStart::Byte(byte),
                _ => DecodeStart::Sample(self.target_sample()),
            },
        }
    }
}

impl StreamDecodeParams {
    pub(crate) fn new(
        segments: Vec<SegmentDecodeParams>,
        byte_seekable: bool,
        leading_silence_frames: u64,
        trailing_silence_frames: u64,
    ) -> Self {
        Self { segments, byte_seekable, leading_silence_frames, trailing_silence_frames }
    }

    #[cfg(desktop)]
    pub(crate) fn leading_silence_frames(&self) -> u64 {
        self.leading_silence_frames
    }

    #[cfg(test)]
    pub(crate) fn segment_count(&self) -> usize {
        self.segments.len()
    }

    #[cfg(test)]
    pub(crate) fn segment_buffer_id(&self, index: usize) -> u64 {
        self.segments[index].buffer.id()
    }

    #[cfg(test)]
    pub(crate) fn segment_target_sample(&self, index: usize) -> u64 {
        self.segments[index].target_sample()
    }

    #[cfg(test)]
    pub(crate) fn segment_decode_start(&self, index: usize) -> bae_audio::DecodeStart {
        self.segments[index].decode_start(self.byte_seekable)
    }

    /// Block until each segment's file has delivered its first bytes — the
    /// ones the decoder's probe reads first — or return why one can't be read.
    /// A caller that commits to its output before decoding (a transcode writes
    /// its header at once) learns of an unreadable file here rather than
    /// partway through. The bytes land in the segments' own buffers, so the
    /// decode reads them from there.
    pub(crate) fn wait_readable(&self) -> Result<(), BufferStop> {
        for segment in &self.segments {
            segment.buffer.new_reader().read(&mut [0u8; 1])?;
        }
        Ok(())
    }

    /// Run the streaming decoder for each segment in turn: FFmpeg seeks (by byte
    /// or by sample), trims lead-in at the segment's target sample, and stops at
    /// the segment's end sample. Shared by the play/seek, preload, and preview
    /// paths so they can't drift on the seek/trim mapping.
    pub(crate) fn run_decoder(
        &self,
        sink: &mut TrackSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), DecodeError> {
        if self.leading_silence_frames > 0 {
            sink.push_silence_frames_blocking(self.leading_silence_frames);
            if cancel.load(Ordering::Relaxed) || sink.is_cancelled() {
                return Err(DecodeError::InputCancelled);
            }
        }

        let mut decode_errors = 0u32;
        for segment in &self.segments {
            decode_errors += bae_audio::decode_audio_streaming(
                segment.buffer.clone(),
                sink,
                segment.decode_start(self.byte_seekable),
                Some(segment.target_sample()),
                segment.span.end_sample,
                segment.span.end_byte,
                cancel.clone(),
            )?;
            if cancel.load(Ordering::Relaxed) || sink.is_cancelled() {
                return Err(DecodeError::InputCancelled);
            }
        }

        if self.trailing_silence_frames > 0 {
            sink.push_silence_frames_blocking(self.trailing_silence_frames);
            if cancel.load(Ordering::Relaxed) || sink.is_cancelled() {
                return Err(DecodeError::InputCancelled);
            }
        }

        // The track is every segment plus its silence; only now has the sink
        // received all of it. Marking it finished after a single segment would
        // end the track at the pregap/main boundary.
        sink.set_decode_error_count(decode_errors);
        sink.mark_finished();
        Ok(())
    }

    /// Decode this window into a [`DecodedSink`] — the save re-encoder's
    /// driver, sharing the per-segment seek derivation with [`Self::run_decoder`]:
    /// announce the stored format, push the leading silence, decode each
    /// segment, push the trailing silence. Blocking; run it off the async
    /// runtime.
    ///
    /// `sample_rate`/`channels` come from the stored audio format; each
    /// segment's decode re-announces the probed values, so a sink that checks
    /// (the encoder) turns a stored-vs-probed mismatch into a loud failure.
    /// A read that panicked comes back as [`DecodeError::ReadPanicked`], for
    /// the caller to report as the bug it is.
    // The Subsonic transcode drives it on every platform; save and export on
    // desktop.
    pub(crate) fn run_to_sink(
        &self,
        sample_rate: u32,
        channels: u32,
        sink: &mut dyn bae_audio::DecodedSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), DecodeError> {
        sink.on_format(sample_rate, channels);
        push_silence_to_sink(sink, self.leading_silence_frames, channels, &cancel)?;
        for segment in &self.segments {
            bae_audio::decode_audio_to_sink_with_seek(
                segment.buffer.clone(),
                segment.decode_start(self.byte_seekable),
                Some(segment.target_sample()),
                segment.span.end_sample,
                sink,
                cancel.clone(),
            )?;
        }
        push_silence_to_sink(sink, self.trailing_silence_frames, channels, &cancel)?;
        Ok(())
    }

    /// Decode this window into a verifying [`DecodedSink`] — import's loudness
    /// pass, which meters the stored audio and checks it decodes whole. Seeks
    /// each segment as [`Self::run_to_sink`] does; invalid packets are
    /// discarded and counted rather than failing the decode, so the sink can
    /// judge from frame coverage whether the track is usable. A failed read of
    /// the source is returned as that read's error. Carries no silence: the
    /// pass measures only stored audio. Blocking; run it off the async runtime.
    /// Each segment's decode announces its stream to `sink` before its packets
    /// (`DecodedSink::on_stream`), so a sink can tell the segments apart.
    #[cfg(desktop)]
    pub(crate) fn verify_to_sink(
        &self,
        sink: &mut dyn bae_audio::DecodedSink,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), DecodeError> {
        for segment in &self.segments {
            bae_audio::decode_audio_to_verifying_sink(
                segment.buffer.clone(),
                segment.decode_start(self.byte_seekable),
                Some(segment.target_sample()),
                segment.span.end_sample,
                sink,
                cancel.clone(),
            )?;
        }
        Ok(())
    }
}

/// Push `frames` frames of silence into the sink in chunks, checking `cancel`
/// between chunks so an aborted save stops promptly.
fn push_silence_to_sink(
    sink: &mut dyn bae_audio::DecodedSink,
    frames: u64,
    channels: u32,
    cancel: &AtomicBool,
) -> Result<(), DecodeError> {
    if frames == 0 {
        return Ok(());
    }
    const CHUNK_FRAMES: u64 = 4096;
    let zeros = vec![0i32; (CHUNK_FRAMES * u64::from(channels)) as usize];
    let mut remaining = frames;
    while remaining > 0 {
        if cancel.load(Ordering::Relaxed) {
            return Err(DecodeError::InputCancelled);
        }
        let chunk = remaining.min(CHUNK_FRAMES);
        sink.on_samples(&zeros[..(chunk * u64::from(channels)) as usize]);
        remaining -= chunk;
    }
    Ok(())
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use super::*;
    use bae_audio::sparse_buffer::create_sparse_buffer;

    /// Counts the frames a decode hands it.
    #[derive(Default)]
    struct FrameCount {
        channels: u32,
        frames: u64,
    }

    impl bae_audio::DecodedSink for FrameCount {
        fn on_format(&mut self, _sample_rate: u32, channels: u32) {
            self.channels = channels;
        }

        fn on_samples(&mut self, samples: &[i32]) {
            self.frames += (samples.len() / self.channels.max(1) as usize) as u64;
        }
    }

    /// The byte offsets a verifying decode of one second from 20s into the
    /// CUE fixture image read, with the window's recorded landing byte, and
    /// the frames it decoded.
    fn verified_window_reads() -> (Vec<u64>, u64, u64) {
        bae_audio::init();
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/cue_flac/Test Album.flac"
        ));
        let bytes = std::fs::read(path).unwrap();
        let start_sample = 20 * 44_100;
        let landing =
            bae_audio::seek_landing_bytes(path, &[start_sample]).expect("landing byte")[0];
        let buffer = create_sparse_buffer(bytes.len() as u64);
        buffer.append_at(0, &bytes);
        let decode = StreamDecodeParams::new(
            vec![SegmentDecodeParams::new(
                buffer.clone(),
                bae_db::SegmentSpan {
                    start_sample,
                    end_sample: Some(start_sample + 44_100),
                    start_byte: Some(landing),
                    end_byte: None,
                },
                0,
            )],
            true,
            0,
            0,
        );
        let mut sink = FrameCount::default();
        decode
            .verify_to_sink(&mut sink, Arc::new(AtomicBool::new(false)))
            .expect("the window decodes");
        (buffer.read_log(), landing, sink.frames)
    }

    /// A verifying decode jumps straight to a segment's recorded landing byte
    /// rather than searching the file for its start sample: past the header,
    /// it reads nothing before the landing.
    #[test]
    fn verifying_decode_jumps_to_the_recorded_landing_byte() {
        let (reads, landing, frames) = verified_window_reads();
        assert_eq!(frames, 44_100);
        let header_end = reads[0] + 8 * 1024;
        assert!(
            reads.iter().all(|&offset| offset < header_end || offset >= landing),
            "reads {reads:?} around landing {landing}"
        );

        assert!(
            reads.contains(&landing),
            "the decode reads from the landing: {reads:?} vs {landing}"
        );
    }

    /// A track made of several segments (a CUE pregap segment followed by its
    /// main segment) finishes once, after the last segment. Finishing the sink
    /// after the first segment makes the player treat the pregap as the whole
    /// track and advance a few seconds in.
    #[test]
    fn multi_segment_track_finishes_only_after_last_segment() {
        use bae_audio::track_stream::create_track_stream_pair_with_capacity;
        use bae_audio::{encode_i32, EncodeFormat};

        let sample_rate = 44_100u32;
        let total_frames = 2 * sample_rate as usize;
        let original: Vec<i32> = (0..total_frames)
            .map(|i| {
                let t = i as f64 / sample_rate as f64;
                (0.5 * i32::MAX as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
            })
            .collect();
        let flac =
            encode_i32(EncodeFormat::Flac { bits_per_sample: 16 }, &original, sample_rate, 1)
                .unwrap();
        let buffer = create_sparse_buffer(flac.len() as u64);
        buffer.append_at(0, &flac);

        let split = sample_rate as u64;
        let span = |start: u64, end: u64| bae_db::SegmentSpan {
            start_sample: start,
            end_sample: Some(end),
            start_byte: None,
            end_byte: None,
        };
        let decode = StreamDecodeParams::new(
            vec![
                SegmentDecodeParams::new(buffer.clone(), span(0, split), 0),
                SegmentDecodeParams::new(buffer, span(split, total_frames as u64), 0),
            ],
            false,
            0,
            0,
        );

        // A ring far smaller than a segment, so the decoder is pushing
        // samples -- not finished -- for most of the run, and a sink finished
        // at the segment boundary is observed before the second segment's
        // samples arrive.
        let (mut sink, mut stream, _ready) =
            create_track_stream_pair_with_capacity(sample_rate, 1, 4_096);
        let decoder = std::thread::spawn(move || {
            decode.run_decoder(&mut sink, Arc::new(AtomicBool::new(false)))
        });

        let mut pulled = 0usize;
        let mut buf = [0.0f32; 1_024];
        loop {
            pulled += stream.pull_samples(&mut buf);
            if stream.producer_finished() {
                assert_eq!(
                    stream.samples_decoded() as usize,
                    total_frames,
                    "sink reported finished before every segment was decoded"
                );
                if stream.is_finished() {
                    break;
                }
            }
        }
        decoder.join().unwrap().expect("two-segment decode succeeds");
        assert_eq!(pulled, total_frames);
    }
}
