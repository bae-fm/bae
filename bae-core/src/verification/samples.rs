//! The rip's audio as one sample stream, which is what every checksum reads.
//!
//! AccurateRip's pressing offsets slide a track's window into its neighbour, so
//! the tracks are decoded into one buffer laid end to end rather than measured
//! file by file. That buffer is the disc's audio at its stored size — four
//! bytes per stereo frame, the same as the CD carries — so a full-length disc
//! costs a little under a gigabyte while it is being verified.

use super::{TrackEdges, VerificationError};
use crate::audio_codec::DecodedSink;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// What a CD holds, and the only thing these checksums describe.
const CD_SAMPLE_RATE: u32 = 44100;
const CD_CHANNELS: u32 = 2;
const CD_BITS_PER_SAMPLE: u32 = 16;

/// A rip's tracks decoded end to end, in disc order.
pub struct DiscSamples {
    /// One entry per stereo frame, `(right << 16) | left` — the 32-bit word
    /// AccurateRip and CTDB both checksum.
    frames: Vec<u32>,
    /// Where each track begins, with the disc's end appended: track `n` spans
    /// `bounds[n]..bounds[n + 1]`.
    bounds: Vec<usize>,
}

impl DiscSamples {
    /// Decode a rip's audio files, in track order, into one disc-long stream.
    ///
    /// Every file must be 44.1 kHz 16-bit stereo: anything else was not ripped
    /// from a CD, and checksumming it would produce a number that can only ever
    /// fail to match.
    pub fn read(files: &[PathBuf]) -> Result<Self, VerificationError> {
        if files.is_empty() {
            return Err(VerificationError::NoAudio);
        }
        let mut frames = Vec::new();
        let mut bounds = Vec::with_capacity(files.len() + 1);
        for path in files {
            bounds.push(frames.len());
            read_track(path, &mut frames)?;
        }
        bounds.push(frames.len());
        Ok(Self { frames, bounds })
    }

    /// The whole disc, which is what the kernels take so a slid window can
    /// cross a track boundary.
    pub fn frames(&self) -> &[u32] {
        &self.frames
    }

    pub fn track_count(&self) -> usize {
        self.bounds.len() - 1
    }

    /// Where track `index` (0-based) sits in the disc's frames.
    pub fn track(&self, index: usize) -> Option<Range<usize>> {
        Some(*self.bounds.get(index)?..*self.bounds.get(index + 1)?)
    }

    /// Which end skips track `index` (0-based) is subject to.
    pub fn edges(&self, index: usize) -> TrackEdges {
        TrackEdges {
            first: index == 0,
            last: index + 1 == self.track_count(),
        }
    }
}

/// Decode one file onto the end of the disc's frames.
fn read_track(path: &Path, frames: &mut Vec<u32>) -> Result<(), VerificationError> {
    let path_str = path.to_str().ok_or_else(|| VerificationError::Read {
        path: path.display().to_string(),
        detail: "path is not UTF-8".to_string(),
    })?;
    let probe = crate::audio_codec::probe_audio_from_path(path_str).ok_or_else(|| {
        VerificationError::Read {
            path: path_str.to_string(),
            detail: "no readable audio stream".to_string(),
        }
    })?;
    if probe.sample_rate != CD_SAMPLE_RATE
        || probe.channels != CD_CHANNELS
        || probe.bits_per_sample != Some(CD_BITS_PER_SAMPLE)
    {
        return Err(VerificationError::NotCdAudio {
            path: path_str.to_string(),
            detail: format!(
                "{} Hz, {} channel(s), {} — not 44100 Hz 16-bit stereo",
                probe.sample_rate,
                probe.channels,
                probe
                    .bits_per_sample
                    .map_or_else(|| "lossy".to_string(), |bits| format!("{bits}-bit")),
            ),
        });
    }

    // The whole compressed file goes into the buffer up front: the decode is a
    // blocking pass with nothing to stream against, and one track's compressed
    // bytes are a rounding error beside the disc's decoded PCM.
    let bytes = std::fs::read(path).map_err(|e| VerificationError::Read {
        path: path_str.to_string(),
        detail: e.to_string(),
    })?;
    let buffer = crate::playback::sparse_buffer::create_sparse_buffer(bytes.len() as u64);
    buffer.append_at(0, &bytes);

    let mut sink = FrameSink {
        frames,
        format: None,
        misaligned: false,
    };
    let never_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    crate::audio_codec::decode_audio_to_sink(buffer, None, None, &mut sink, never_cancelled)
        .map_err(|detail| VerificationError::Decode {
            path: path_str.to_string(),
            detail,
        })?;

    if sink.misaligned {
        return Err(VerificationError::Decode {
            path: path_str.to_string(),
            detail: "decode produced a partial stereo frame".to_string(),
        });
    }
    match sink.format {
        Some((CD_SAMPLE_RATE, CD_CHANNELS)) => Ok(()),
        Some((sample_rate, channels)) => Err(VerificationError::NotCdAudio {
            path: path_str.to_string(),
            detail: format!("decoded as {sample_rate} Hz, {channels} channel(s)"),
        }),
        None => Err(VerificationError::Decode {
            path: path_str.to_string(),
            detail: "decode reported no format".to_string(),
        }),
    }
}

/// Packs the decode's interleaved samples into the disc's 32-bit frames.
///
/// FFmpeg hands every decode out as full-range i32; a 16-bit source is shifted
/// left by 16 to get there, so shifting back recovers the stored sample exactly.
struct FrameSink<'a> {
    frames: &'a mut Vec<u32>,
    format: Option<(u32, u32)>,
    misaligned: bool,
}

impl DecodedSink for FrameSink<'_> {
    fn on_format(&mut self, sample_rate: u32, channels: u32) {
        self.format = Some((sample_rate, channels));
    }

    fn on_samples(&mut self, samples: &[i32]) {
        if !samples.len().is_multiple_of(CD_CHANNELS as usize) {
            self.misaligned = true;
        }
        for pair in samples.chunks_exact(CD_CHANNELS as usize) {
            let left = u32::from((pair[0] >> 16) as i16 as u16);
            let right = u32::from((pair[1] >> 16) as i16 as u16);
            self.frames.push((right << 16) | left);
        }
    }
}

#[cfg(test)]
#[path = "samples_tests.rs"]
mod tests;
