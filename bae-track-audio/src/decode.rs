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
pub struct StreamDecodeParams {
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
/// `target_sample`, the demuxer start via `decode_start`.
#[derive(Clone)]
pub struct SegmentDecodeParams {
    buffer: SharedSparseBuffer,
    span: bae_records::SegmentSpan,
    start_offset: u64,
    start: SegmentStart,
}

/// Where a segment's decode starts reading its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentStart {
    /// From what the span records: its landing byte at a natural start of a
    /// codec whose frames keep their place after a byte jump, else a seek by
    /// sample to the target.
    Span,
    /// A stored scrub point that plays the target exactly.
    ScrubPoint(bae_records::ScrubPoint),
    /// The stream's first frame: an MP3 target before every stored point's
    /// exact start.
    StreamStart,
}

impl SegmentDecodeParams {
    /// A window with no scrub point to start at: a natural start jumps to the
    /// span's landing byte, a mid-track seek seeks by sample.
    pub fn new(
        buffer: SharedSparseBuffer,
        span: bae_records::SegmentSpan,
        start_offset: u64,
    ) -> Self {
        Self { buffer, span, start_offset, start: SegmentStart::Span }
    }

    /// A window `start_offset` samples into the segment whose decode starts
    /// reading the file at `start`, then decodes forward to the target.
    pub fn with_start(
        buffer: SharedSparseBuffer,
        span: bae_records::SegmentSpan,
        start_offset: u64,
        start: SegmentStart,
    ) -> Self {
        Self { buffer, span, start_offset, start }
    }

    /// First sample to emit: the segment's start plus the seek offset. FFmpeg
    /// trims the lead-in the seek lands before this (a frame may begin before
    /// it), so the first output sample is exact.
    pub(crate) fn target_sample(&self) -> u64 {
        self.span.start_sample + self.start_offset
    }

    /// The byte the decode first reads past the file's head, when known:
    /// a scrub point's frame, or at a natural start the span's landing byte,
    /// which a byte jump lands on and APE's sample seek through its index
    /// lands on too. `None` from the stream's start, which the head is, and
    /// for a sample seek mid-segment, whose landing only the demuxer finds.
    pub(crate) fn first_read_byte(&self) -> Option<u64> {
        match self.start {
            SegmentStart::ScrubPoint(point) => Some(point.byte),
            SegmentStart::StreamStart => None,
            SegmentStart::Span if self.start_offset == 0 => self.span.start_byte,
            SegmentStart::Span => None,
        }
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
    pub fn new(
        segments: Vec<SegmentDecodeParams>,
        byte_seekable: bool,
        leading_silence_frames: u64,
        trailing_silence_frames: u64,
    ) -> Self {
        Self { segments, byte_seekable, leading_silence_frames, trailing_silence_frames }
    }

    /// The byte the first window reading through the buffer `buffer_id`
    /// first reads past its file's head (`SegmentDecodeParams::first_read_byte`),
    /// which a fresh buffer's reader fetches first; `None` when no window
    /// reads through it or that byte isn't known.
    pub fn first_read_byte(&self, buffer_id: u64) -> Option<u64> {
        self.segments
            .iter()
            .find(|segment| segment.buffer.id() == buffer_id)
            .and_then(SegmentDecodeParams::first_read_byte)
    }

    #[cfg(desktop)]
    pub fn leading_silence_frames(&self) -> u64 {
        self.leading_silence_frames
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn segment_buffer_id(&self, index: usize) -> u64 {
        self.segments[index].buffer.id()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn segment_target_sample(&self, index: usize) -> u64 {
        self.segments[index].target_sample()
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn segment_decode_start(&self, index: usize) -> bae_audio::DecodeStart {
        self.segments[index].decode_start(self.byte_seekable)
    }

    /// Block until each segment's file has delivered its first bytes — the
    /// ones the decoder's probe reads first — or return why one can't be read.
    /// A caller that commits to its output before decoding (a transcode writes
    /// its header at once) learns of an unreadable file here rather than
    /// partway through. The bytes land in the segments' own buffers, so the
    /// decode reads them from there.
    pub fn wait_readable(&self) -> Result<(), BufferStop> {
        for segment in &self.segments {
            segment.buffer.new_reader().read(&mut [0u8; 1])?;
        }
        Ok(())
    }

    /// Run the streaming decoder for each segment in turn: FFmpeg seeks (by byte
    /// or by sample), trims lead-in at the segment's target sample, and stops at
    /// the segment's end sample. Shared by the play/seek, preload, and preview
    /// paths so they can't drift on the seek/trim mapping.
    pub fn run_decoder(
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

    /// Decode this window into a [`DecodedSink`](bae_audio::DecodedSink) — the save re-encoder's
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
    pub fn run_to_sink(
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

    /// Decode this window into a verifying [`DecodedSink`](bae_audio::DecodedSink) — import's loudness
    /// pass, which meters the stored audio and checks it decodes whole. Seeks
    /// each segment as [`Self::run_to_sink`] does; invalid packets are
    /// discarded and counted rather than failing the decode, so the sink can
    /// judge from frame coverage whether the track is usable. A failed read of
    /// the source is returned as that read's error. Carries no silence: the
    /// pass measures only stored audio. Blocking; run it off the async runtime.
    /// Each segment's decode announces its stream to `sink` before its packets
    /// (`DecodedSink::on_stream`), so a sink can tell the segments apart.
    #[cfg(desktop)]
    pub fn verify_to_sink(
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

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
