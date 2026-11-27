//! A track's stored audio: its format, its segments with the size and
//! container of the files they name, and the loudness its release and it were
//! measured at, as playback, a save and a server of the track read it.

use super::{DbAudioSegmentRole, Records};
use bae_store::DbError;

/// Why a track's audio could not be read.
#[derive(Debug, thiserror::Error)]
pub enum TrackAudioError {
    /// The library holds no track by this id.
    #[error("Track not found: {0}")]
    TrackNotFound(String),
    #[error("No audio format for track: {0}")]
    NoAudioFormat(String),
    /// A segment names a release file the library doesn't hold.
    #[error("Audio file not found: {0}")]
    FileNotFound(String),
    /// A segment names a release file that states no audio facts.
    #[error("Audio segment names a file that is not audio: {0}")]
    NotAudio(String),
    #[error("No audio segments for track: {0}")]
    NoSegments(String),
    #[error(transparent)]
    Database(#[from] DbError),
}

#[derive(Clone, Debug)]
pub struct ResolvedTrackAudioSegment {
    pub role: DbAudioSegmentRole,
    pub file_id: String,
    pub file_size: u64,
    /// The container its backing file's bytes are in: what a serve of the
    /// whole file names as its type.
    pub container: bae_util::audio_container::AudioContainer,
    /// Where this segment sits inside its backing file, in samples and bytes.
    pub span: crate::SegmentSpan,
    /// The frames a scrub inside this segment jumps to by byte.
    pub scrub_points: crate::ScrubPoints,
}

/// All resolved data needed to play, save or serve a track.
///
/// Read from a track's rows by [`ResolvedTrackAudio::resolve`], or beside its
/// tags by a save's reads.
#[derive(Clone, Debug)]
pub struct ResolvedTrackAudio {
    pub track_id: String,
    pub release_id: String,
    pub segments: Vec<ResolvedTrackAudioSegment>,
    pub duration_ms: u64,
    pub pregap_samples: Option<u64>,
    pub generated_pregap_samples: Option<u64>,
    pub sample_rate: std::num::NonZeroU32,
    pub channels: std::num::NonZeroU32,
    /// Bits per sample as stored at import (16, 24, …). `None` for lossy codecs,
    /// where the source has no fixed sample depth. Carried so callers that
    /// describe the audio to a client (the Subsonic server's `bitDepth`) don't
    /// re-fetch the raw audio-format row.
    pub bits_per_sample: Option<std::num::NonZeroU32>,
    /// This track's audio codec, as stored at import. Playback dispatches the
    /// track-start seek on it: FLAC/lossless byte-seek to `start_byte`; APE
    /// sample-seeks its mandatory index and also prefetches the file's end (its
    /// demuxer reads the tail on open).
    pub content_type: bae_util::content_type::ContentType,
    /// Raw loudness/peak measurements (LUFS + linear peak) for this track and its
    /// album, as stored at import. `None` = not measured. Playback derives the
    /// replay gain from these against a constant target; nothing here is a gain.
    pub track_loudness_lufs: Option<f64>,
    pub track_peak_linear: Option<f64>,
    pub album_loudness_lufs: Option<f64>,
    pub album_peak_linear: Option<f64>,
}

impl ResolvedTrackAudio {
    /// `track_id`'s audio as the library stores it now.
    pub async fn resolve(records: &Records, track_id: &str) -> Result<Self, TrackAudioError> {
        records.track_audio(track_id).await
    }

    /// The stored file that is the track's stream, when one is: a single main
    /// segment from the file's first sample to its end, with no generated
    /// pregap. Only then do the file's bytes, served as they are, hold the
    /// track's stream; a CUE track's file holds the whole image, or its pregap
    /// lives in another file, or its pregap is silence bae generates. Every
    /// other track is served decoded from its own segments.
    pub fn whole_file(&self) -> Option<&ResolvedTrackAudioSegment> {
        let [segment] = self.segments.as_slice() else {
            return None;
        };
        (segment.role == DbAudioSegmentRole::Main
            && segment.span.start_sample == 0
            && segment.span.end_sample.is_none()
            && self.generated_pregap_frames() == 0)
            .then_some(segment)
    }

    /// The silent frames bae generates before the track's first stored sample,
    /// from a CUE `PREGAP` directive; zero without one. The track's stream
    /// starts with them wherever it is decoded from its start — playback, and
    /// the stream served to a remote renderer.
    pub fn generated_pregap_frames(&self) -> u64 {
        self.generated_pregap_samples.unwrap_or(0)
    }

    /// How long the track's stream runs before INDEX 01: its stored pregap and
    /// the silence bae generates, counted in samples and timed by the sample
    /// rate. Zero without either.
    pub fn pregap(&self) -> std::time::Duration {
        let frames = self.pregap_samples.unwrap_or(0) + self.generated_pregap_frames();
        bae_util::sample_time::frame_start(frames, self.sample_rate.get())
    }
}
