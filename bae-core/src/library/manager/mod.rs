//! Orchestrator layer: the one place where raw DB aggregates meet util
//! formatters and filesystem paths to produce resolved types.
//!
//! `LibraryManager` owns the database handle. Its surface is split into
//! per-entity modules — `release`, `album`, `track`, `artist`, `identity`,
//! `image`, `import`, `export`, `storage` — each holding that entity's I/O
//! operations, reading `&Database` / `&CovenHandle` for the covers, pin state,
//! and joins a resolved shape needs. The pure projection from a `Db*` aggregate
//! to its resolved counterpart lives on the produced type, as a `from_raw`
//! constructor in `crate::album_detail`. Public methods return the resolved
//! shapes — `AlbumSummary`, `ReleaseStorageSummary`, `SearchResults`,
//! `AlbumDetail`, `ReleaseDetail` — never the raw `Db*` aggregates.
//!
//! Rule for additions: all DB-backed data flows through this layer. A new
//! resolved shape means a raw type in `crate::db::models`, a resolved type in
//! `crate::album_detail` (or a sibling like `crate::queue`) with its `from_raw`
//! constructor, and the I/O method in the matching entity module here.

use std::collections::HashMap;
use std::future::Future;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;
use tracing::{debug, warn};

use crate::album_detail::{
    image_refs, AlbumDetail, AlbumSummary, ArtistDetail, ArtistSearchResult, ArtistSummary,
    ComposerDetail, ComposerSummary, ComposerWorkGroup, GallerySource, ImageRef, ReleaseDetail,
    ReleaseResolveCtx, ReleaseStorageAction, ReleaseStorageState, ReleaseStorageSummary,
    SearchResults, StoragePage, StorageRow, WorkDetail, WorkReleaseSummary, WorkSummary,
};
use crate::config::ConfigHandle;
use crate::db::{
    Database, DbAlbum, DbArtist, DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, DbFile,
    DbLibraryImage, DbRelease, DbTrack, LibraryImageType,
};
use crate::diagnostics::{Diagnostics, SyncOperation, TelemetryEvent};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::library::save::SaveService;
use crate::library::sync_controller::SyncController;
use crate::sync::S3ConfigData;
use coven::ClockRef;
use coven::IdSourceRef;

mod service;
pub use service::LibraryOpening;

mod abandoned;
pub use abandoned::AbandonedItem;
mod album;
mod artist;
mod collections;
mod composer;
mod config;
mod coven_files;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod discogs;
mod error_category;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
mod fixture;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub use fixture::{
    FixtureAlbum, FixtureCandidate, FixtureCandidateState, FixturePlaying, FixtureWatchedFolder,
    LibraryFixture, LibraryFixtureError, FIXTURE_IMPORT_FAILURE, FIXTURE_LOOKUP_FAILURE,
};
/// Desktop-only, under the same predicate as the rest of the export surface (the
/// queue field below, and `library::export`). Exporting writes a directory tree
/// next to the user's chosen folder — a hidden staging sibling, a marker file, a
/// rename into place — which a sandboxed iOS/Android document URL does not offer,
/// and the preset half of it needs the desktop-only `library::export` encoder.
/// Compiling the producers out with the worker is what makes a mobile export call
/// a compile error rather than a release that sits `Queued` with nothing to run it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod export;
mod history;
mod image;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod import;
mod lifecycle;
mod local_files;
mod locality;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod output;
mod people;
mod pin;
mod playback_state;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod records;
mod release;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use release::stored_track_lengths;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod release_edit;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use release_edit::ReleaseEditSession;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod save;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use save::SaveWindow;
mod originals;
mod pending_item;
mod storage;
mod sync;
mod sync_status;
pub(crate) use error_category::{bootstrap_category, creation_category, deletion_category};
use error_category::{
    cloud_home_category, cloud_setup_category, cloud_unlock_category, file_read_category,
    key_category, sync_category,
};
mod track;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use discogs::DiscogsCheck;
pub use pending_item::{PendingItem, PendingSubjectName, PendingWait};
use sync_status::SyncStatusState;
pub use sync_status::{pending_action, PendingAction, SyncRetryOutcome};

/// Outcome of `resolve_records_target_album` — where a release should
/// land after a `set_records` call. `new_album` carries the album row
/// to insert when the target is brand-new; otherwise the target is an
/// existing album and `new_album` is `None`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct RecordsTargetAlbum {
    album_id: String,
    new_album: Option<DbAlbum>,
}

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] crate::db::DbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// An import step the library ran failed — reading a catalog release
    /// against a release's tracks, fetching a cover, a candidate's paths —
    /// as the import error itself, so every surface words the refusal it is
    /// (another number of tracks, discs that fit alike) rather than a
    /// generic line.
    #[error(transparent)]
    Import(Box<crate::import::ImportError>),
    /// An operation on a release, an album, an artist or an import
    /// candidate could not go ahead as asked — what it names is not in the
    /// library, the candidate moved under it, an input is blank — with why
    /// as text. Shown as the generic import line.
    #[error("{0}")]
    Refused(String),
    #[error("Import error: {0}")]
    ArtistIdentityConflict(#[source] Box<crate::import::ArtistIdentityConflict>),
    /// A verbatim release export (reproducing the imported file set) failed.
    #[error("Export error: {0}")]
    Export(String),
    /// A save (rendered output — track or release) failed.
    #[error("Save error: {0}")]
    Save(String),
    /// A user-submitted release metadata edit failed its invariants (blank album
    /// title, no album artist). The editor's typed error, so every surface reports
    /// the same rule rather than its own hand-written sentence.
    #[error(transparent)]
    Edit(#[from] crate::import::EditValidationError),
    #[error("Release editor cannot load: {0}")]
    ReleaseEditLoad(#[from] crate::album_detail::ReleaseEditLoadError),
    /// A release edit session's save found the release changed since the
    /// session opened on it, and wrote nothing.
    #[error("Release '{release_id}' changed since its edit session opened")]
    ReleaseEditStale { release_id: String },
    #[error("Track mapping error: {0}")]
    TrackMapping(String),
    #[error("Encryption error: {0}")]
    Encryption(#[from] coven::CryptoError),
    #[error("Storage error: {0}")]
    Storage(String),
    /// Reading, pinning, or unpinning a file's bytes through coven failed.
    #[error("{operation}: {error}")]
    FileRead {
        operation: String,
        #[source]
        error: Box<coven::FileReadError>,
    },
    #[error("Playback error: {0}")]
    Playback(String),
    /// Work asked of a library after it closed; nothing of it started.
    #[error("{0} cannot start: the library is closed")]
    Closed(&'static str),
    /// An internal invariant the caller can't act on (a missing platform driver,
    /// a provider used through the wrong entry point).
    #[error("{0}")]
    Internal(String),
    /// The OS keyring couldn't be read or written (encryption key, cloud-home
    /// credentials, Discogs/MCP tokens).
    #[error("Keyring error: {0}")]
    Keyring(#[from] coven::KeyError),
    /// Reading or writing the on-disk library config failed.
    #[error("Config error: {0}")]
    Config(#[from] crate::config::ConfigError),
    /// Building or probing a cloud home failed — bad credentials/config
    /// (`Configuration`) or an unreachable backend (`Transport`).
    #[error("Cloud home error: {0}")]
    CloudHome(#[from] coven::StorageError),
    #[error("Cloud setup error: {0}")]
    CloudSetup(#[from] coven::StorageSetupError),
    #[error("Cloud unlock error: {0}")]
    CloudUnlock(#[from] coven::StoreKeyUnlockError),
    /// A sync-manager connection or membership operation failed.
    #[error("Sync error: {0}")]
    Sync(#[from] coven::SyncError),
    /// An invite could no longer be cancelled: approval of its person has
    /// begun, and stopping would leave them half joined, or it has ended.
    #[error("the invite can no longer be cancelled: {0}")]
    InviteJoining(#[source] coven::SyncError),
    /// Handing one blocked sync operation back to the sync loop failed — most
    /// often because it is no longer blocked, so there is nothing to retry.
    #[error("Retry blocked operation error: {0}")]
    RetryBlockedOperation(#[source] Box<coven::OperationError>),
    /// A bae-level input validation failed (an empty library name). bae's own
    /// validation, not coven's — the detail is log-only, so the user sees the
    /// generic settings line.
    #[error("{0}")]
    Validation(String),
    #[error("{operation}; cleanup also failed: {cleanup}")]
    Cleanup {
        operation: Box<LibraryError>,
        cleanup: Box<LibraryError>,
    },
}

impl LibraryError {
    pub(crate) fn file_read(operation: impl Into<String>, error: coven::FileReadError) -> Self {
        Self::FileRead {
            operation: operation.into(),
            error: Box::new(error),
        }
    }
}

impl From<coven::DbError> for LibraryError {
    fn from(error: coven::DbError) -> Self {
        Self::Database(error.into())
    }
}

impl From<coven::CovenError> for LibraryError {
    fn from(error: coven::CovenError) -> Self {
        Self::Database(error.into())
    }
}

impl From<crate::db::ArtistWriteError> for LibraryError {
    fn from(error: crate::db::ArtistWriteError) -> Self {
        match error {
            crate::db::ArtistWriteError::IdentityConflict(conflict) => {
                Self::ArtistIdentityConflict(conflict)
            }
            crate::db::ArtistWriteError::Db(error) => Self::Database(error),
            crate::db::ArtistWriteError::FilesAlreadyImported { release_id } => {
                Self::Import(Box::new(crate::import::ImportError::FilesAlreadyImported {
                    release_id,
                }))
            }
            other @ (crate::db::ArtistWriteError::Unresolvable(_)
            | crate::db::ArtistWriteError::LibraryChanged) => Self::Refused(other.to_string()),
        }
    }
}

impl From<crate::import::ArtistIdentityConflict> for LibraryError {
    fn from(error: crate::import::ArtistIdentityConflict) -> Self {
        Self::ArtistIdentityConflict(Box::new(error))
    }
}

#[derive(Debug)]
pub struct UploadReceipt {
    pub outbox_revision: u64,
    pub release_ids: Vec<String>,
}

#[derive(Debug)]
pub struct UploadBatchFailure {
    pub release_ids: Vec<String>,
    pub error: crate::ui::UiError,
}

#[derive(Debug)]
pub enum UploadReleasesOutcome {
    Complete {
        receipt: UploadReceipt,
    },
    Partial {
        receipt: Option<UploadReceipt>,
        failure: UploadBatchFailure,
    },
}

/// A stored fragment we refuse to join onto a local path surfaces as an import
/// failure: the row it came from is unusable, and the copy it was for does not run.
impl From<crate::storage::path_fragment::PathFragmentError> for LibraryError {
    fn from(error: crate::storage::path_fragment::PathFragmentError) -> Self {
        LibraryError::Refused(error.to_string())
    }
}

impl LibraryError {
    /// The user-facing diagnostic class the bridge renders. Membership/setup
    /// failures carry the distinctions the cloud-setup and sharing flows show as
    /// different messages (bad credentials vs unreachable backend vs keyring vs
    /// membership); everything else maps to its general class.
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        use crate::ui::UiErrorCategory as C;
        match self {
            LibraryError::Database(_) => C::Database,
            LibraryError::Config(_) | LibraryError::Validation(_) => C::Config,
            LibraryError::Keyring(error) => key_category(error),
            LibraryError::CloudHome(e) => cloud_home_category(e),
            LibraryError::CloudSetup(error) => cloud_setup_category(error),
            LibraryError::CloudUnlock(error) => cloud_unlock_category(error),
            LibraryError::Sync(e) => sync_category(e),
            LibraryError::InviteJoining(_) => C::InviteJoining,
            LibraryError::RetryBlockedOperation(_) => C::Membership,
            LibraryError::Import(error) => error.category(),
            LibraryError::Refused(_) | LibraryError::Edit(_) => C::Import,
            LibraryError::ReleaseEditStale { .. } => C::ReleaseChangedSinceOpened,
            LibraryError::ArtistIdentityConflict(_) => C::Import,
            LibraryError::Export(_) => C::Export,
            LibraryError::Save(_) => C::Save,
            LibraryError::FileRead { error, .. } => file_read_category(error),
            LibraryError::Cleanup { operation, .. } => operation.category(),
            LibraryError::Io(_)
            | LibraryError::TrackMapping(_)
            | LibraryError::ReleaseEditLoad(_)
            | LibraryError::Encryption(_)
            | LibraryError::Storage(_)
            | LibraryError::Playback(_)
            | LibraryError::Closed(_)
            | LibraryError::Internal(_) => C::Internal,
        }
    }
}

/// A write the import handle ran to completion on its own task did not report
/// back: the task panicked, or the runtime is shutting down under it.
impl From<tokio::task::JoinError> for LibraryError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Internal(format!("a candidate write did not complete: {error}"))
    }
}

/// Library operations that run import steps (a release editor's catalog
/// apply, a cover download) report the import error itself. One that is a
/// library error already (`ImportError::Db`) is that library error again, so
/// the two never wrap each other.
impl From<crate::import::ImportError> for LibraryError {
    fn from(error: crate::import::ImportError) -> Self {
        match error {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            crate::import::ImportError::Db(error) => error,
            other => LibraryError::Import(Box::new(other)),
        }
    }
}

/// Sync activity and the things coven is waiting on that need attention.
/// Each list is `None` until first read.
#[derive(Debug, Clone)]
pub struct SyncStatusSnapshot {
    pub status: coven::SyncStatus,
    pub pending: Option<Result<Vec<PendingItem>, crate::ui::UiError>>,
    /// The files this device attached that no row will name, until the
    /// person dismisses them; `None` until first read.
    pub abandoned: Option<Result<Vec<AbandonedItem>, crate::ui::UiError>>,
    pub error: Option<crate::ui::UiError>,
}

/// The settings status line. Platforms localize these cases and the item count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncIndicator {
    Unconnected,
    /// Before the pending list's first read.
    Loading,
    /// Storage is set up, but the sync loop isn't running.
    Stopped,
    Syncing,
    Synced,
    Offline,
    /// The failure is the snapshot's `failure()`.
    Failed,
    /// The pending list couldn't be read; its error is the list's.
    Unreadable,
    Waiting {
        items: usize,
    },
}

/// All DB data needed to play or serve a track: the internal aggregate behind
/// `resolve_track_audio`, also carried inside `SaveTrackPlan` for the export
/// decoder. Callers that only need resolved playback data use
/// `ResolvedTrackAudio`; the export path still needs the raw rows (a segment's
/// byte range, its CUE sample bounds) for whole-file decode, so this raw shape
/// stays `pub(crate)`.
#[derive(Clone)]
pub(crate) struct TrackAudioMeta {
    pub track: DbTrack,
    pub release: DbRelease,
    pub audio_format: DbAudioFormat,
    pub audio_segments: Vec<DbAudioSegment>,
    /// The scrub points of every segment that has any.
    pub scrub_points: Vec<crate::db::DbScrubPoints>,
    pub audio_files: Vec<DbFile>,
}

impl TrackAudioMeta {
    pub(crate) async fn resolve(database: &Database, track_id: &str) -> Result<Self, LibraryError> {
        let track = database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;

        let audio_format = database
            .find_audio_format_by_track_id(track_id)
            .await?
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!("No audio format for track: {}", track_id))
            })?;

        let release = database.get_release_for_track(&track).await?;

        let audio_segments = database
            .get_audio_segments_for_format(&audio_format.id)
            .await?;
        if audio_segments.is_empty() {
            return Err(LibraryError::TrackMapping(format!(
                "No audio segments for track: {}",
                track_id
            )));
        }

        let scrub_points = database
            .get_scrub_points_for_format(&audio_format.id)
            .await?;

        let mut audio_files = Vec::new();
        for segment in &audio_segments {
            let audio_file = database
                .find_file_by_id(&segment.file_id)
                .await?
                .ok_or_else(|| {
                    LibraryError::TrackMapping(format!("Audio file not found: {}", segment.file_id))
                })?;
            if !audio_files
                .iter()
                .any(|existing: &DbFile| existing.id == audio_file.id)
            {
                audio_files.push(audio_file);
            }
        }

        Ok(Self {
            track,
            release,
            audio_format,
            audio_segments,
            scrub_points,
            audio_files,
        })
    }
}

pub struct ResolvedTrackAudioSegment {
    pub role: DbAudioSegmentRole,
    pub file_id: String,
    pub file_size: u64,
    /// Where this segment sits inside its backing file, in samples and bytes.
    pub span: crate::db::SegmentSpan,
    /// The frames a scrub inside this segment jumps to by byte.
    pub scrub_points: crate::playback::ScrubPoints,
}

/// All resolved data needed to set up a playback reader for a track.
///
/// Returned by `LibraryManager::resolve_track_audio` — no raw `Db*` types
/// exposed. The track's sample window is resolved from the stored bounds, so the
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
    /// Carry each segment's file and its size with the playback parameters.
    pub(crate) fn from_meta(meta: &TrackAudioMeta) -> Self {
        let segments = meta
            .audio_segments
            .iter()
            .map(|segment| {
                let audio_file = meta
                    .audio_files
                    .iter()
                    .find(|file| file.id == segment.file_id)
                    .expect("TrackAudioMeta resolves every segment file");
                ResolvedTrackAudioSegment {
                    role: segment.role.clone(),
                    file_id: segment.file_id.clone(),
                    file_size: audio_file.file_size as u64,
                    span: segment.span(),
                    scrub_points: crate::playback::ScrubPoints::new(
                        meta.scrub_points
                            .iter()
                            .find(|stored| stored.audio_segment_id == segment.id)
                            .map_or_else(Vec::new, |stored| stored.points.clone()),
                    ),
                }
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
            bits_per_sample: meta
                .audio_format
                .bits_per_sample
                .map(std::num::NonZeroU32::get),
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

    /// The WAV the track's stream is served as uncompressed: PCM at the depth
    /// it was stored at (16, 24 or 32 bits), or 16 for a lossy codec, which
    /// has no stored depth and decodes to nothing finer.
    pub fn wav_stream_format(&self) -> bae_audio::StreamEncodeFormat {
        let bits_per_sample = match self.bits_per_sample {
            Some(17..=24) => 24,
            Some(25..) => 32,
            Some(_) | None => 16,
        };
        bae_audio::StreamEncodeFormat::PcmWav { bits_per_sample }
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
        let rate = u64::from(self.sample_rate.get());
        let nanos = (frames % rate) * 1_000_000_000 / rate;
        std::time::Duration::new(
            frames / rate,
            u32::try_from(nanos).expect("a fraction of a second is under 10^9 ns"),
        )
    }

    /// Linear playback gain for this track under `mode`. `1.0` = no change.
    ///
    /// The gain is a view of (stored measurements, mode, target) — never stored.
    /// `Off` plays at unity. `Track`/`Album` pick that level's `(loudness, peak)`,
    /// falling back to the other level when the preferred one wasn't measured,
    /// and to unity when neither was (NULL measurements, or a silent track). For
    /// the chosen level the gain brings the measured loudness to the target, then
    /// is capped at `1.0/peak` so a boosted track can't clip.
    pub fn replay_gain_linear(&self, mode: crate::config::ReplayGainMode) -> f32 {
        use crate::config::ReplayGainMode;

        let track = self.track_loudness_lufs.zip(self.track_peak_linear);
        let album = self.album_loudness_lufs.zip(self.album_peak_linear);

        let chosen = match mode {
            ReplayGainMode::Off => None,
            ReplayGainMode::Track => track.or(album),
            ReplayGainMode::Album => album.or(track),
        };

        let Some((loudness_lufs, peak_linear)) = chosen else {
            return 1.0;
        };

        let gain = 10f64.powf((REPLAY_GAIN_TARGET_LUFS - loudness_lufs) / 20.0);
        // Cap the gain so the loudest true-peak sample can't exceed full scale.
        // A non-positive peak (no usable peak) imposes no cap.
        let max_safe = if peak_linear > 0.0 {
            1.0 / peak_linear
        } else {
            f64::INFINITY
        };
        gain.min(max_safe) as f32
    }
}

/// Target playback loudness the replay gain aims each track/album at, in LUFS.
/// -18 LUFS is a common reference for quiet-listening normalization.
const REPLAY_GAIN_TARGET_LUFS: f64 = -18.0;

/// Tag fields to embed on an exported track file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct SaveTags {
    pub title: String,
    /// The track's line: its own credits, else the album's (ARTIST).
    pub artist: String,
    /// The album's line (ALBUMARTIST).
    pub album_artist: String,
    pub album: String,
    pub year: Option<i32>,
    /// The track's medium among its release's, for a release of several
    /// mediums. `None` for a release of one, which writes no disc tag.
    pub disc: Option<u32>,
}

/// Tag data resolved for one track — everything a filename template or the
/// tag writer needs, before applying the user's metadata selection. Resolved
/// from the database alone: no audio or cover read, so the filename-suggestion
/// path can build it without touching a whole file or the cloud.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct ResolvedSaveTags {
    pub tags: SaveTags,
    /// The track's number on its side.
    pub track_number: u32,
    pub total_tracks: usize,
}

/// Which image to use when changing an album's cover art.
pub enum CoverSelection {
    /// Use an image file already in the library.
    ReleaseImage { file_id: String },
    /// Download from a remote URL.
    RemoteCover {
        url: String,
        source: bae_catalogs::catalog::Catalog,
    },
}

/// Verb for a storage transition, used to name the operation in the "ended
/// without completion" guard.
fn verb(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "Pin",
        ReleaseStorageAction::Unpin => "Unpin",
        ReleaseStorageAction::Upload => "Upload",
        ReleaseStorageAction::DetachAudio => "Detach Audio",
    }
}

/// Persistence and queries for albums, tracks, and files: import state
/// transitions, library browsing, and deletion with cloud-storage cleanup.
#[derive(Clone)]
pub struct LibraryManager {
    database: Database,
    config_handle: Arc<ConfigHandle>,
    /// The app directory this library is registered under: where renaming
    /// another library finds it, where the active-library pointer is written,
    /// and where forgetting this library removes it from.
    app_dir: crate::config::AppDir,
    clock: ClockRef,
    ids: IdSourceRef,
    /// Typed telemetry sink, injected at bootstrap alongside the clock/id
    /// sources. The sync-failure path emits through it; the playback and import
    /// services read it back off this manager for their own events.
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    /// The manager's own background tasks (queue workers, the sync status and
    /// outbox subscriptions, startup sync), which each hold a clone of it and
    /// so of the store. [`LibraryManager::close`] ends them all.
    tasks: service::BackgroundTasks,
    /// The readers of every deletion of tracks, each hearing all of them in
    /// order, as the ids deleted: the player drops them from its queue.
    track_deletions: Arc<std::sync::Mutex<Vec<tokio::sync::mpsc::UnboundedSender<Vec<String>>>>>,
    /// The cloud-sync responsibility: the outbox projection over the live
    /// upload state, provider connection, and membership.
    sync: SyncController,
    /// The sync banner's state and the stream the front-ends read it from.
    sync_status: SyncStatusState,
    /// Every storage transition in flight, and the stream the storage rows
    /// read them from.
    transitions: crate::library::storage_transitions::StorageTransitions,
    /// The "Pin for offline" queue and the Downloads pane's stream. A single
    /// serial worker drains it one release at a time. Shared across manager
    /// clones; transient (empty after a restart — a release that wasn't fully
    /// pinned stays cloud-only).
    downloads: crate::library::Downloads,
    /// The "Export…" and save queue and the stream the inspector reads. A single
    /// serial worker drains it one release at a time. Shared across manager
    /// clones; transient. An output changes no release state — it only reads,
    /// and writes to a user directory. Desktop-only: see the `output` module.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    outputs: crate::library::Outputs,
    /// Session cache of provider image responses, keyed by URL. Candidate
    /// preparation and library images own their bytes independently.
    remote_images: bae_catalogs::remote_image::RemoteImageCache,
    /// The services releases are looked up in, built once by the app.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    providers: crate::providers::Providers,
    /// The Discogs client for the key this library stores, built on first use
    /// and dropped when the key is set or cleared, so every Discogs call
    /// shares one key read and one client. Held under one lock with the key's
    /// own writes, so no client outlives the key it was built with.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    discogs_client:
        Arc<std::sync::Mutex<Option<Arc<bae_catalogs::discogs::client::DiscogsClient>>>>,
    /// The one writer of import candidates' stored state.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    preparations: crate::import::CandidatePreparations,
    /// Held by every edit session's save and every field edit of a release,
    /// so a save compares the release with what its session opened on and
    /// writes with no other of them on this device between the two.
    release_writes: Arc<tokio::sync::Mutex<()>>,
    /// Where a test holds every look at this device's originals.
    #[cfg(test)]
    disk_hold: Arc<originals::DiskHold>,
}

pub fn generate_mcp_token() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

impl std::fmt::Debug for LibraryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryManager")
            .field("database", &self.database)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
