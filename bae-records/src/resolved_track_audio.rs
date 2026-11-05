//! A track's stored audio, read from its rows: its format, its segments and
//! the files they name, and the loudness its release and it were measured at.

#[cfg(desktop)]
use super::DbFile;
use super::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, DbRelease, DbTrack, Records};
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
    #[error("No audio segments for track: {0}")]
    NoSegments(String),
    #[error(transparent)]
    Database(#[from] DbError),
}

/// All DB data needed to play or serve a track: the internal aggregate behind
/// [`ResolvedTrackAudio::resolve`], also carried inside `SaveTrackPlan` for the export
/// decoder. Callers that only need resolved playback data use
/// `ResolvedTrackAudio`; the export path still needs the raw rows (a segment's
/// byte range, its CUE sample bounds) for whole-file decode, so this raw shape
/// stays `pub(crate)`.
#[derive(Clone)]
pub struct TrackAudioMeta {
    pub track: DbTrack,
    pub release: DbRelease,
    pub audio_format: DbAudioFormat,
    /// The format's segments in segment order, each with its file's size and
    /// its scrub points.
    pub audio_segments: Vec<TrackAudioSegment>,
    /// The release files the segments name, each once, which a save opens.
    #[cfg(desktop)]
    pub audio_files: Vec<DbFile>,
}

/// One segment of a track's audio, with the size of the file it names and
/// the frames a scrub inside it can start at.
#[derive(Clone)]
pub struct TrackAudioSegment {
    pub segment: DbAudioSegment,
    pub file_size: u64,
    pub scrub_points: Vec<crate::ScrubPoint>,
}

impl TrackAudioMeta {
    pub async fn resolve(records: &Records, track_id: &str) -> Result<Self, TrackAudioError> {
        let track = records
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| TrackAudioError::TrackNotFound(track_id.to_string()))?;

        let audio_format = records
            .find_audio_format_by_track_id(track_id)
            .await?
            .ok_or_else(|| TrackAudioError::NoAudioFormat(track_id.to_string()))?;

        let release = records.get_release_for_track(&track).await?;

        let audio_segments = records
            .get_track_audio_segments(&audio_format.id)
            .await?
            .into_iter()
            .map(|row| {
                let file_size = row
                    .file_size
                    .ok_or_else(|| TrackAudioError::FileNotFound(row.segment.file_id.clone()))?;
                Ok(TrackAudioSegment {
                    file_size,
                    segment: row.segment,
                    scrub_points: row.scrub_points,
                })
            })
            .collect::<Result<Vec<_>, TrackAudioError>>()?;
        if audio_segments.is_empty() {
            return Err(TrackAudioError::NoSegments(track_id.to_string()));
        }
        #[cfg(desktop)]
        let audio_files = records.get_audio_files_for_format(&audio_format.id).await?;

        Ok(Self {
            track,
            release,
            audio_format,
            audio_segments,
            #[cfg(desktop)]
            audio_files,
        })
    }
}

pub struct ResolvedTrackAudioSegment {
    pub role: DbAudioSegmentRole,
    pub file_id: String,
    pub file_size: u64,
    /// Where this segment sits inside its backing file, in samples and bytes.
    pub span: crate::SegmentSpan,
    /// The frames a scrub inside this segment jumps to by byte.
    pub scrub_points: crate::ScrubPoints,
}

/// All resolved data needed to set up a playback reader for a track.
///
/// Resolved from a track's rows by [`ResolvedTrackAudio::resolve`] — no raw
/// `Db*` types exposed. The track's sample window is resolved from the stored bounds, so the
/// playback service never reads raw audio format fields.
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
    pub bits_per_sample: Option<u32>,
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
        Ok(Self::from_meta(&TrackAudioMeta::resolve(records, track_id).await?))
    }

    /// Carry each segment's file and its size with the playback parameters.
    fn from_meta(meta: &TrackAudioMeta) -> Self {
        let segments = meta
            .audio_segments
            .iter()
            .map(|entry| ResolvedTrackAudioSegment {
                role: entry.segment.role.clone(),
                file_id: entry.segment.file_id.clone(),
                file_size: entry.file_size,
                span: entry.segment.span(),
                scrub_points: crate::ScrubPoints::new(entry.scrub_points.clone()),
            })
            .collect();
        Self {
            track_id: meta.track.id.clone(),
            release_id: meta.track.release_id.clone(),
            segments,
            duration_ms: meta.track.duration_ms,
            pregap_samples: meta.audio_format.pregap_samples,
            generated_pregap_samples: meta.audio_format.generated_pregap_samples,
            sample_rate: meta.audio_format.sample_rate,
            channels: meta.audio_format.channels,
            bits_per_sample: meta.audio_format.bits_per_sample.map(std::num::NonZeroU32::get),
            content_type: meta.audio_format.content_type.clone(),
            track_loudness_lufs: meta.audio_format.track_loudness_lufs,
            track_peak_linear: meta.audio_format.track_peak_linear,
            album_loudness_lufs: meta.release.album_loudness_lufs,
            album_peak_linear: meta.release.album_peak_linear,
        }
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
