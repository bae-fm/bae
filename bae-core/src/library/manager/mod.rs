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
use std::sync::{Arc, Mutex};

use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::album_detail::{
    join_artist_names, AlbumDetail, AlbumSummary, ArtistDetail, ArtistSummary, ComposerDetail,
    ComposerSummary, ComposerWorkGroup, GallerySource, ImageRef, ReleaseDetail, ReleaseResolveCtx,
    ReleaseStorageAction, ReleaseStorageState, ReleaseStorageSummary, SearchResults, StoragePage,
    StorageRow, WorkDetail, WorkReleaseSummary, WorkSummary,
};
#[cfg(feature = "oauth-providers")]
use crate::config::CloudProvider;
use crate::config::ConfigHandle;
use crate::db::{
    Database, DbAlbum, DbArtist, DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, DbFile,
    DbLibraryImage, DbRelease, DbTrack, DeleteCleanupPlan, InFlightMakeRemoteBlobCleanup,
    LibraryImageType, Pressing,
};
use crate::diagnostics::{Diagnostics, SyncOperation, TelemetryEvent};
use crate::keys::BaeStoreKeysExt;
use crate::keys::StoreKeys;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::library::save::SaveService;
use crate::library::sync_controller::SyncController;
use crate::playback::QueueEntry;
use crate::queue::QueueItem;
use crate::sync::S3ConfigData;
use coven::ClockRef;
#[cfg(any(test, feature = "test-utils"))]
use coven::CloudHome;
use coven::CovenHandle;
use coven::IdRef;
use coven::SyncLoopStatus;

/// Library events can burst during imports and sync catch-up; lag is recoverable
/// through UI invalidations, and this bound keeps ordinary bursts in order.
const LIBRARY_EVENT_CHANNEL_CAPACITY: usize = 1024;

mod album;
mod artist;
mod composer;
mod config;
mod coven_blobs;
/// Desktop-only, under the same predicate as the rest of the export surface (the
/// queue field below, and `library::export`). Exporting writes a directory tree
/// next to the user's chosen folder — a hidden staging sibling, a marker file, a
/// rename into place — which a sandboxed iOS/Android document URL does not offer,
/// and the preset half of it needs the desktop-only `library::export` encoder.
/// Compiling the producers out with the worker is what makes a mobile export call
/// a compile error rather than a release that sits `Queued` with nothing to run it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod export;
mod identity;
mod image;
mod import;
mod lifecycle;
mod locality;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod output;
mod playback_state;
mod release;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod save;
mod storage;
mod sync;
mod track;

pub(crate) use release::find_release_detail_with;

/// Outcome of `resolve_identity_target_album` — where a release should
/// land after a `set_identity` call. `new_album` carries the album row
/// to insert when the target is brand-new; otherwise the target is an
/// existing album and `new_album` is `None`.
struct IdentityTargetAlbum {
    album_id: String,
    new_album: Option<DbAlbum>,
}

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] coven::DbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Import error: {0}")]
    Import(String),
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
    #[error("Track mapping error: {0}")]
    TrackMapping(String),
    #[error("Encryption error: {0}")]
    Encryption(#[from] coven::EncryptionError),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Playback error: {0}")]
    Playback(String),
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
    Config(#[from] coven::ConfigError),
    /// Building or probing a cloud home failed — bad credentials/config
    /// (`Configuration`) or an unreachable backend (`Transport`).
    #[error("Cloud home error: {0}")]
    CloudHome(#[from] coven::CloudHomeError),
    /// Consumer-cloud OAuth sign-in failed. Carries coven's setup-error
    /// message as a string: coven's `SetupError` type is not part of its
    /// curated public API, so the sign-in call sites map it here by display.
    #[error("Cloud setup error: {0}")]
    CloudSetup(String),
    /// A sync-manager connection or membership operation failed.
    #[error("Sync error: {0}")]
    Sync(#[from] coven::SyncError),
    /// A bae-level input validation failed (an empty library name). bae's own
    /// validation, not coven's — the detail is log-only, so the user sees the
    /// generic settings line.
    #[error("{0}")]
    Validation(String),
    /// Establishing this store's master key (or finding one already
    /// established when none was expected) failed.
    #[error("Master key error: {0}")]
    MasterKey(#[from] coven::MasterKeyError),
}

/// A stored fragment we refuse to join onto a local path surfaces as an import
/// failure: the row it came from is unusable, and the copy it was for does not run.
impl From<crate::storage::path_fragment::PathFragmentError> for LibraryError {
    fn from(error: crate::storage::path_fragment::PathFragmentError) -> Self {
        LibraryError::Import(error.to_string())
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
            LibraryError::Keyring(_) => C::Keyring,
            LibraryError::CloudHome(e) => cloud_home_category(e),
            LibraryError::CloudSetup(_) => C::Credentials,
            LibraryError::Sync(e) => sync_category(e),
            LibraryError::Import(_) | LibraryError::Edit(_) => C::Import,
            LibraryError::Export(_) => C::Export,
            LibraryError::Save(_) => C::Save,
            LibraryError::MasterKey(_) => C::Keyring,
            LibraryError::Io(_)
            | LibraryError::TrackMapping(_)
            | LibraryError::Encryption(_)
            | LibraryError::Storage(_)
            | LibraryError::Playback(_)
            | LibraryError::Internal(_) => C::Internal,
        }
    }
}

/// A cloud-home failure the user must fix (bad credentials, missing bucket) vs a
/// transient one to retry (unreachable backend, local I/O).
fn cloud_home_category(error: &coven::CloudHomeError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    if error.is_retryable() {
        C::Network
    } else {
        C::Credentials
    }
}

/// Classify a coven sync/membership failure into a user-facing class: keyring vs
/// cloud credentials/network vs the membership chain itself.
fn sync_category(error: &coven::SyncError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    use coven::SyncError;
    match error {
        SyncError::Key(_) => C::Keyring,
        SyncError::CloudHome(e) => cloud_home_category(e),
        SyncError::Setup(_) => C::Credentials,
        SyncError::Membership(_) => C::Membership,
        SyncError::StorageSetup(_) => C::Network,
        SyncError::NotConfigured
        | SyncError::LoopNotRunning
        | SyncError::NotEncryptedHome
        | SyncError::MasterKeyNotEstablished
        | SyncError::Init(_)
        | SyncError::Database(_)
        | SyncError::BlobUpload(_)
        | SyncError::Loop(_) => C::Internal,
    }
}

impl From<crate::import::ImportError> for LibraryError {
    /// The re-identify / reset-from-source paths run import mappers but report
    /// through `LibraryError`; a mapper failure becomes an `Import` error with
    /// the typed error's Display as the message. (`ImportError` carries
    /// `Db(#[from] LibraryError)` for the other direction — the two are
    /// distinct conversions.)
    fn from(value: crate::import::ImportError) -> Self {
        LibraryError::Import(value.to_string())
    }
}

/// Current sync status for query callers. Event subscribers receive the same
/// fields as transition events; this snapshot is the current value.
#[derive(Debug, Clone)]
pub struct SyncStatusSnapshot {
    pub error: Option<crate::ui::UiError>,
    pub last_sync_time: Option<i64>,
    pub syncing: bool,
    pub sync_ready: bool,
}

/// What the sync indicator shows, in precedence order. One decision, so the four
/// front-ends stop each writing their own — and so `Synced` can only ever carry a
/// time when the loop is actually running, which is the bug it replaces (a stale
/// timestamp read as "Synced" on a loop that never came up).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncIndicator {
    /// A sync error is present. The banner shows its line; the badge shows the
    /// error state.
    Error,
    /// A sync cycle is actively running.
    Syncing,
    /// The loop is running and idle. Carries the last successful sync time, the
    /// only place it is exposed — so it cannot be shown for a stopped loop.
    Synced { last_sync_time: Option<i64> },
    /// A cloud home is connected but its loop is not running, with nothing to
    /// report. Neither an error nor "synced".
    Idle,
}

impl SyncIndicator {
    /// The one place the indicator's precedence lives. Error over an active cycle
    /// over a running-and-idle loop over nothing. `last_sync_time` reaches the
    /// result only through `Synced`, so it cannot surface for a stopped loop.
    pub fn resolve(
        has_error: bool,
        syncing: bool,
        sync_ready: bool,
        last_sync_time: Option<i64>,
    ) -> SyncIndicator {
        if has_error {
            SyncIndicator::Error
        } else if syncing {
            SyncIndicator::Syncing
        } else if sync_ready {
            SyncIndicator::Synced { last_sync_time }
        } else {
            SyncIndicator::Idle
        }
    }
}

impl SyncStatusSnapshot {
    /// The indicator this status resolves to.
    pub fn indicator(&self) -> SyncIndicator {
        SyncIndicator::resolve(
            self.error.is_some(),
            self.syncing,
            self.sync_ready,
            self.last_sync_time,
        )
    }
}

#[cfg(test)]
mod sync_indicator_tests {
    use super::*;

    fn snapshot(
        sync_ready: bool,
        syncing: bool,
        error: bool,
        time: Option<i64>,
    ) -> SyncStatusSnapshot {
        SyncStatusSnapshot {
            error: error.then(|| crate::ui::UiError::Diagnostic {
                category: crate::ui::UiErrorCategory::Network,
                detail: "boom".to_string(),
            }),
            last_sync_time: time,
            syncing,
            sync_ready,
        }
    }

    /// An error wins over everything, including a ready loop with a recent sync.
    #[test]
    fn error_wins() {
        assert_eq!(
            snapshot(true, false, true, Some(100)).indicator(),
            SyncIndicator::Error
        );
    }

    /// An active cycle shows as syncing even once the loop is ready.
    #[test]
    fn an_active_cycle_shows_syncing() {
        assert_eq!(
            snapshot(true, true, false, Some(100)).indicator(),
            SyncIndicator::Syncing
        );
    }

    /// A ready, idle loop is synced and carries its time.
    #[test]
    fn a_ready_idle_loop_is_synced_with_its_time() {
        assert_eq!(
            snapshot(true, false, false, Some(100)).indicator(),
            SyncIndicator::Synced {
                last_sync_time: Some(100)
            }
        );
    }

    /// The bug this replaces: a loop that never came up is Idle, not Synced —
    /// even with a stale timestamp from a previous session.
    #[test]
    fn a_stopped_loop_with_a_stale_time_is_idle_not_synced() {
        assert_eq!(
            snapshot(false, false, false, Some(100)).indicator(),
            SyncIndicator::Idle
        );
    }
}

#[derive(Debug, Clone)]
struct SyncStatusState {
    error: Option<String>,
    last_sync_time_raw: Option<String>,
    last_sync_time: Option<i64>,
    syncing: bool,
}

impl SyncStatusState {
    fn initial(handle: &CovenHandle) -> Self {
        Self {
            error: None,
            last_sync_time_raw: None,
            last_sync_time: None,
            syncing: handle.is_syncing(),
        }
    }
}

struct ReleaseDeletePlan {
    db_cleanup: DeleteCleanupPlan,
    evict_blobs: Vec<coven::BlobRef>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) struct ImportReplacementPlan {
    pub(crate) db_delete: crate::db::ImportReplacementDelete,
    pub(crate) evict_blobs: Vec<coven::BlobRef>,
    pub(crate) track_ids: Vec<String>,
}

/// All DB data needed to play or serve a track: the internal aggregate behind
/// `resolve_track_audio`, also carried inside `SaveTrackPlan` for the export
/// decoder. Callers that only need resolved playback data use
/// `ResolvedTrackAudio`; the export path still needs the raw rows (a segment's
/// byte range, its CUE sample bounds) for whole-file decode, so this raw shape
/// stays `pub(crate)`.
pub(crate) struct TrackAudioMeta {
    pub track: DbTrack,
    pub release: DbRelease,
    pub audio_format: DbAudioFormat,
    pub audio_segments: Vec<DbAudioSegment>,
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
            audio_files,
        })
    }
}

pub struct ResolvedTrackAudioSegment {
    pub role: DbAudioSegmentRole,
    pub file_id: String,
    pub cloud_path: Option<String>,
    pub file_size: u64,
    pub start_sample: u64,
    pub end_sample: Option<u64>,
    pub start_byte: Option<u64>,
    pub end_byte: Option<u64>,
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
    pub duration_ms: Option<i64>,
    pub pregap_ms: Option<i64>,
    pub generated_pregap_ms: Option<i64>,
    pub pregap_samples: Option<i64>,
    pub generated_pregap_samples: Option<i64>,
    pub sample_rate: u32,
    pub channels: u32,
    /// This track's audio codec, as stored at import. Playback dispatches the
    /// track-start seek on it: FLAC/lossless byte-seek to `start_byte`; APE
    /// sample-seeks its mandatory index and also prefetches the file's end (its
    /// demuxer reads the tail on open).
    pub content_type: crate::util::content_type::ContentType,
    /// Raw loudness/peak measurements (LUFS + linear peak) for this track and its
    /// album, as stored at import. `None` = not measured. Playback derives the
    /// replay gain from these against a constant target; nothing here is a gain.
    pub track_loudness_lufs: Option<f64>,
    pub track_peak_linear: Option<f64>,
    pub album_loudness_lufs: Option<f64>,
    pub album_peak_linear: Option<f64>,
}

impl ResolvedTrackAudio {
    /// Build a resolved view of a track's audio from raw DB records. coven owns the
    /// locality resolution at read time (external ref / local store / cache /
    /// cloud), so this carries only the blob's identity (`file_id` + `cloud_path`)
    /// and the playback parameters — not a resolved read source.
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
                    cloud_path: audio_file.cloud_path.clone(),
                    file_size: audio_file.file_size as u64,
                    start_sample: segment.start_sample as u64,
                    end_sample: segment.end_sample.map(|sample| sample as u64),
                    start_byte: segment.start_byte.map(|byte| byte as u64),
                    end_byte: segment.end_byte.map(|byte| byte as u64),
                }
            })
            .collect();
        Self {
            track_id: meta.track.id.clone(),
            release_id: meta.track.release_id.clone(),
            segments,
            duration_ms: meta.track.duration_ms,
            pregap_ms: meta.audio_format.pregap_ms,
            generated_pregap_ms: meta.audio_format.generated_pregap_ms,
            pregap_samples: meta.audio_format.pregap_samples,
            generated_pregap_samples: meta.audio_format.generated_pregap_samples,
            sample_rate: meta.audio_format.sample_rate as u32,
            channels: meta.audio_format.channels as u32,
            content_type: meta.audio_format.content_type.clone(),
            track_loudness_lufs: meta.audio_format.track_loudness_lufs,
            track_peak_linear: meta.audio_format.track_peak_linear,
            album_loudness_lufs: meta.release.album_loudness_lufs,
            album_peak_linear: meta.release.album_peak_linear,
        }
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

/// The release a directly-selected track plays from: its full track order (by
/// `side, track_number, id`) and the selected track's index into it. Everything
/// the playback service needs to seed a context, so it never chases back into the
/// library for neighbouring track IDs.
pub struct PlayContext {
    pub release_id: String,
    pub track_ids: Vec<String>,
    pub index: usize,
}

/// Tag fields to embed on an exported track file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct SaveTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    /// Disc number for multi-disc releases. `None` when the release is
    /// single-disc — we don't write a disc tag in that case.
    pub disc: Option<i32>,
}

/// Tag data resolved for one track — everything a filename template or the
/// tag writer needs, before applying the user's metadata selection. Resolved
/// from the database alone: no audio or cover read, so the filename-suggestion
/// path can build it without touching a whole file or the cloud.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct ResolvedSaveTags {
    pub tags: SaveTags,
    pub track_number: Option<i32>,
    pub total_tracks: usize,
    pub is_digital: bool,
}

/// Pre-assembled data for exporting a single track. Everything
/// `SaveService::save_track` needs comes out of a single
/// `LibraryManager::get_save_track_plan` call; the export service
/// never chases back into the database to resolve tags, paths, or
/// neighbours.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct SaveTrackPlan {
    /// Source audio files read at plan time, keyed by release file id.
    pub(crate) audio_bytes: Vec<SaveAudioBytes>,
    /// The track's tag data, exactly as the filename template and the tag writer
    /// take it — embedded rather than copied field by field, so the plan and the
    /// template always describe the same track.
    pub resolved: ResolvedSaveTags,
    /// The cover image bytes to embed, read through coven at plan time, or `None`
    /// when the album has no primary release with a cover.
    pub cover_image_bytes: Option<Vec<u8>>,
    /// The source/silence window to encode for this export. Playback uses the
    /// stored audio format directly; export can exclude or move CUE pregaps.
    pub(crate) audio_window: SaveAudioWindow,
    /// Raw audio-format aggregate. Held internally so `SaveService::save_track`
    /// can decode CUE-split byte ranges / APE sample bounds without re-resolving.
    pub(crate) audio_meta: TrackAudioMeta,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) struct SaveAudioBytes {
    pub file_id: String,
    pub bytes: Vec<u8>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub(crate) struct SaveAudioWindow {
    pub segments: Vec<SaveAudioSegmentWindow>,
    pub leading_silence_samples: u64,
    pub trailing_silence_samples: u64,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone)]
pub(crate) struct SaveAudioSegmentWindow {
    pub file_id: String,
    pub source_start_sample: u64,
    pub source_end_sample: Option<u64>,
}

/// Which image to use when changing an album's cover art.
pub enum CoverSelection {
    /// Use an image file already in the library.
    ReleaseImage { file_id: String },
    /// Download from a remote URL.
    RemoteCover {
        url: String,
        source: crate::import::MetadataSource,
    },
}

/// Verb for a storage transition, used to name the operation in the "ended
/// without completion" guard.
fn verb(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "Pin",
        ReleaseStorageAction::Unpin => "Unpin",
        ReleaseStorageAction::MakeRemote => "Manage",
        ReleaseStorageAction::MakeLocal => "Unmanage",
    }
}

/// Emits `ReleaseTransferEnded` on drop unless defused. `drive_transfer` holds
/// one so an aborted transfer future (its bridge wrapper is dropped mid-flight)
/// still clears the UI's transfer indicator; the normal exit defuses it after
/// emitting the event itself. The broadcast send is synchronous, so `Drop` can
/// fire it directly.
struct TransferEndedGuard {
    event_tx: broadcast::Sender<LibraryEvent>,
    transfer_actions: Arc<Mutex<HashMap<String, ReleaseStorageAction>>>,
    release_id: String,
    armed: bool,
}

impl TransferEndedGuard {
    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl Drop for TransferEndedGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.transfer_actions
            .lock()
            .unwrap()
            .remove(&self.release_id);
        if let Err(err) = self.event_tx.send(LibraryEvent::ReleaseTransferEnded {
            release_id: std::mem::take(&mut self.release_id),
        }) {
            warn!("ReleaseTransferEnded on abort had no subscribers: {err}");
        }
    }
}

/// Removes a release's transfer cancellation token from the registry when the
/// transfer ends — whether it completes normally or its future is dropped (a
/// view dismiss), so a dropped transfer never leaves a stale token behind.
struct TransferCancelGuard {
    registry: Arc<Mutex<HashMap<String, crate::library::CancellationToken>>>,
    release_id: String,
}

impl Drop for TransferCancelGuard {
    fn drop(&mut self) {
        self.registry.lock().unwrap().remove(&self.release_id);
    }
}

/// Emitted by `LibraryManager` when data changes.
///
/// Album-level and release-level events are mutually exclusive for the same album
/// in the same mutation: each mutation site emits exactly one event per affected
/// album. `AlbumAdded` carries the first release in its payload, so `ReleaseAdded`
/// only fires when the album already exists.
#[derive(Clone, Debug)]
pub enum LibraryEvent {
    // ── Album-level: carry the full album payload ─────────────────
    AlbumAdded {
        album: AlbumDetail,
    },
    AlbumUpdated {
        album: AlbumDetail,
    },
    AlbumRemoved {
        album_id: String,
        /// Ids of the album's child releases to drop alongside it.
        release_ids: Vec<String>,
    },

    // ── Release-level (carry the release + parent album) ─────────
    ReleaseAdded {
        album: AlbumSummary,
        release: ReleaseDetail,
    },
    ReleaseUpdated {
        album_id: String,
        release: ReleaseDetail,
    },
    ReleaseRemoved {
        album_id: String,
        release_id: String,
        /// Parent album's post-removal summary, so the reducer interns it
        /// instead of patching its `release_ids` by reading the old list.
        /// `None` when the album itself was removed with its last release.
        album: Option<AlbumSummary>,
    },

    // ── Retained (not about library data shape) ──────────────────
    TracksDeleted {
        track_ids: Vec<String>,
    },
    /// Sync loop's latest error state. `None` clears a prior failure (sync
    /// recovered). Emitted on transitions so the UI banner appears and
    /// disappears in step with sync health. When set, it's a
    /// `UiError::Diagnostic` whose category keys the generic line and whose
    /// detail is the opaque, log-only error chain.
    SyncError {
        error: Option<crate::ui::UiError>,
    },
    /// Wall-clock time the last sync cycle completed successfully, as Unix
    /// epoch milliseconds. Emitted on transitions so the sidebar's "Last
    /// synced …" subtitle updates after every cycle and stays stable in
    /// between.
    SyncTimeChanged {
        time: Option<i64>,
    },
    /// Whether the sync loop is currently mid-cycle. Emitted on transitions so
    /// the sidebar can show a spinner over the active library row from when a
    /// "Sync Now" kicks the loop to when it idles again.
    SyncingChanged {
        syncing: bool,
    },
    /// The cloud outbox changed — carries the full processing snapshot so the
    /// Storage Manager re-renders its queue panel.
    OutboxChanged {
        snapshot: crate::library::OutboxSnapshot,
    },
    /// A pin/unpin/manage/unmanage transition started. The UI shows an in-flight
    /// indicator until the matching `ReleaseTransferEnded` arrives.
    ReleaseTransferProgress {
        release_id: String,
        action: ReleaseStorageAction,
    },
    /// A transition finished (success OR failure) — the UI clears its transfer
    /// indicator. On failure the user-facing reason still arrives via the
    /// thrown error from the transfer call.
    ReleaseTransferEnded {
        release_id: String,
    },
    /// The in-memory download (pin) queue changed — carries the full snapshot
    /// so the Storage Manager re-renders its Downloads pane and storage rows
    /// re-read their per-release "Downloading" badge.
    DownloadQueueChanged {
        snapshot: crate::library::DownloadSnapshot,
    },
    /// The in-memory export queue changed — carries the full snapshot so the
    /// Storage Manager re-renders its Exporting pane.
    OutputQueueChanged {
        snapshot: crate::library::OutputSnapshot,
    },
}
/// Persistence and queries for albums, tracks, and files: import state
/// transitions, library browsing, and deletion with cloud-storage cleanup.
#[derive(Clone)]
pub struct LibraryManager {
    database: Database,
    config_handle: Arc<ConfigHandle>,
    key_service: StoreKeys,
    clock: ClockRef,
    ids: IdRef,
    /// Typed telemetry sink, injected at bootstrap alongside the clock/id
    /// sources. The sync-failure path emits through it; the playback and import
    /// services read it back off this manager for their own events.
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    /// The one coven data handle: it owns the database connection, the library
    /// directory, the keys, the cloud storage, and — once a provider is connected
    /// — the sync manager. Every blob read/write, every locality transition, and
    /// the whole sync lifecycle route through it; bae passes only descriptors
    /// (a `BlobRef`, a root id) and never assembles coven's internals by hand.
    handle: CovenHandle,
    event_tx: broadcast::Sender<LibraryEvent>,
    /// The cloud-sync responsibility: the upload pipeline (outbox in-flight,
    /// throughput, pause), provider connection, membership, and the coven
    /// make-Remote/make-Local primitives.
    sync: SyncController,
    sync_status: Arc<Mutex<SyncStatusState>>,
    /// Cancellation tokens for in-progress foreground transfers (unmanage),
    /// keyed by release id. `cancel_release_transition` fires the token; the
    /// transfer observes it between files, deletes the partial copies it wrote,
    /// and leaves the release remote (no orphans). Registered for the transfer's
    /// duration; transient.
    transfer_cancels: Arc<Mutex<HashMap<String, crate::library::CancellationToken>>>,
    transfer_actions: Arc<Mutex<HashMap<String, ReleaseStorageAction>>>,
    /// In-memory queue for "Pin for offline". A single serial worker drains it
    /// one release at a time. Shared across manager clones; transient (empty
    /// after a restart — a release that wasn't fully pinned stays cloud-only).
    download_queue: Arc<crate::library::DownloadQueue>,
    /// In-memory queue for "Export…" (copy a release's files out to a folder). A
    /// single serial worker drains it one release at a time. Shared across
    /// manager clones; transient (empty after a restart). Export changes no
    /// release state — it only reads and writes to a user directory. Desktop-only:
    /// see the `export` module above.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    output_queue: Arc<crate::library::OutputQueue>,
    /// The upload observer coven reports blob transitions to. coven holds only a
    /// `Weak` to it (through `WeakUploadObserver`), so this strong `Arc` is its
    /// sole owner and its lifetime is the manager's: when the last manager clone
    /// drops, the observer drops, releasing the `CovenHandle` it holds and coven's
    /// store-open lock with it. Registering the observer strongly in coven would
    /// instead close a cycle (observer → handle → coven → observer) that pins the
    /// lock forever. `None` only for the database-only [`Self::new`] test
    /// constructor, which installs no observer. Held for its lifetime, never read
    /// (coven reaches it through the `Weak`), so it carries the leading underscore.
    _upload_observer: Option<Arc<crate::sync::upload_observer::ReleaseUploadObserver>>,
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

impl LibraryManager {
    /// Open coven through the top-level builder and create the library manager
    /// over the resulting handle.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        config_handle: Arc<ConfigHandle>,
        key_service: StoreKeys,
        clock: ClockRef,
        ids: IdRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    ) -> Result<Self, coven::DbError> {
        let (event_tx, _) = broadcast::channel(LIBRARY_EVENT_CHANNEL_CAPACITY);
        let outbox_in_flight = Arc::new(Mutex::new(HashMap::new()));
        let upload_sessions = Arc::new(crate::library::UploadSessions::new());
        let upload_throughput = Arc::new(crate::library::UploadThroughput::new());
        let sync_paused = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let observer = Arc::new(crate::sync::upload_observer::ReleaseUploadObserver::new(
            outbox_in_flight.clone(),
            upload_sessions.clone(),
            upload_throughput.clone(),
            sync_paused.clone(),
            event_tx.clone(),
        ));
        // coven holds only a `Weak` to the observer (via `WeakUploadObserver`);
        // the `LibraryManager` below owns the strong `Arc`. Registering the
        // observer strongly here would close a cycle through the `CovenHandle` it
        // holds back, pinning coven's store-open lock past the manager's life.
        let weak_observer = Arc::new(crate::sync::upload_observer::WeakUploadObserver::new(
            Arc::downgrade(&observer),
        ));
        let (max_uploads, max_downloads) = {
            let config = config_handle.config();
            (
                crate::config::usize_bound(config.max_concurrent_uploads),
                crate::config::usize_bound(config.max_concurrent_downloads),
            )
        };
        let ch = Arc::clone(&config_handle);
        let config_provider = move || ch.config().to_coven();
        let handle = coven::Coven::builder(config_provider)
            .synced_tables(crate::sync::synced_tables())
            .clock(clock.clone())
            .key_service(key_service.clone())
            .apply_cloudkit_ops(cloudkit_ops.clone())
            .observer(weak_observer as Arc<dyn coven::BlobTransitionObserver>)
            .migrations(crate::migrations::all())
            .max_concurrent_uploads(max_uploads)
            .max_concurrent_downloads(max_downloads)
            .open()
            .map_err(|e| match e {
                coven::CovenError::Database(error) => error,
                other => coven::DbError(other.to_string()),
            })?;
        let database = Database::from_handle(handle.clone(), clock.clone(), ids.clone());
        observer.set_database(Arc::new(database.clone()));
        observer.set_handle(handle.clone());
        let sync_status = Arc::new(Mutex::new(SyncStatusState::initial(&handle)));

        let sync = SyncController::new(
            handle.clone(),
            config_handle.clone(),
            key_service.clone(),
            event_tx.clone(),
            database.clone(),
            outbox_in_flight,
            upload_sessions,
            upload_throughput,
            sync_paused,
            cloudkit_ops,
            diagnostics.clone(),
        );

        let manager = LibraryManager {
            database,
            config_handle,
            key_service,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            handle,
            event_tx,
            sync,
            sync_status,
            transfer_cancels: Arc::new(Mutex::new(HashMap::new())),
            transfer_actions: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(crate::library::DownloadQueue::new()),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            output_queue: Arc::new(crate::library::OutputQueue::new()),
            _upload_observer: Some(observer),
        };
        manager.start_queue_workers();
        Ok(manager)
    }

    /// Create a library manager over an already-open database handle. Production
    /// uses [`Self::open`] so the upload observer is installed into coven before
    /// sync starts; this constructor remains for tests that exercise database-only
    /// manager behavior.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        database: Database,
        config_handle: Arc<ConfigHandle>,
        key_service: StoreKeys,
        clock: ClockRef,
        ids: IdRef,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(LIBRARY_EVENT_CHANNEL_CAPACITY);
        let handle = database.handle().clone();

        let sync = SyncController::new(
            handle.clone(),
            config_handle.clone(),
            key_service.clone(),
            event_tx.clone(),
            database.clone(),
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(crate::library::UploadSessions::new()),
            Arc::new(crate::library::UploadThroughput::new()),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            None,
            diagnostics.clone(),
        );
        let sync_status = Arc::new(Mutex::new(SyncStatusState::initial(&handle)));

        let manager = LibraryManager {
            database,
            config_handle,
            key_service,
            clock,
            ids,
            diagnostics,
            runtime_handle,
            handle,
            event_tx,
            sync,
            sync_status,
            transfer_cancels: Arc::new(Mutex::new(HashMap::new())),
            transfer_actions: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(crate::library::DownloadQueue::new()),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            output_queue: Arc::new(crate::library::OutputQueue::new()),
            _upload_observer: None,
        };
        manager.start_queue_workers();
        manager
    }

    fn start_queue_workers(&self) {
        self.spawn_queue_worker(|manager| async move {
            manager.run_download_worker().await;
        });
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        self.spawn_queue_worker(|manager| async move {
            manager.run_output_worker().await;
        });
    }

    fn spawn_queue_worker<F, Fut>(&self, worker: F)
    where
        F: FnOnce(Self) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let manager = self.clone();
        self.runtime_handle.spawn(worker(manager));
    }

    pub(crate) fn current_transfer_action(&self, release_id: &str) -> Option<ReleaseStorageAction> {
        self.transfer_actions
            .lock()
            .unwrap()
            .get(release_id)
            .copied()
    }

    /// Connect a real `SyncManager` over an injected cloud home for tests, so the
    /// handle's make-Remote / make-Local / upload-drain / read paths all run
    /// against a mock cloud with no live provider — the test counterpart of
    /// `attach_and_start_sync`. `cipher` is the home's at-rest protection:
    /// `Plaintext` for a browsable mock, `Encrypted(service)` for an opaque one.
    /// After this, `has_cloud_home` and `is_sync_ready` resolve off the
    /// connected manager, no override needed.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn connect_test_cloud_home(
        &self,
        cloud_home: Arc<dyn CloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), LibraryError> {
        self.sync.connect_test_cloud_home(cloud_home, cipher).await
    }

    /// Set the cloud home's storage mode in config, so a test can exercise the
    /// browsable read/write paths against an injected cloud home (production sets
    /// this through the cloud-setup wizard). The connected home's cipher must match
    /// — `Plaintext` for browsable, `Encrypted` for opaque.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_home_storage(&self, storage: crate::config::HomeStorage) {
        self.config_handle
            .update(|c| c.cloud_home.storage = storage)
            .expect("set test home storage mode");
    }

    /// The cloud object key the read path resolves for a remote file: the row's
    /// stored `cloud_path` on a browsable home, the hashed-by-id key on an opaque
    /// one. Exposed so a test can assert the read key matches the stored upload key
    /// without setting up full playback.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn resolve_track_cloud_key_for_test(&self, file_id: &str) -> String {
        let file = self
            .database
            .find_file_by_id(file_id)
            .await
            .expect("file lookup")
            .expect("file exists");
        self.release_file_cloud_key(&file).expect("cloud key")
    }

    /// The injected wall clock. The import layer and the mappers read "now"
    /// through this so the whole import shares one clock under test.
    pub(crate) fn clock(&self) -> &ClockRef {
        &self.clock
    }

    /// The injected telemetry sink. The playback and import services read it
    /// off the manager they already hold rather than each taking a separate
    /// handle, so there is one source threaded from bootstrap.
    pub(crate) fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// Ship one `audio_format_orphaned` anomaly per orphan the release-detail
    /// projection reported (a pure layer that can only detect and log them). A
    /// zero count ships nothing.
    fn report_audio_format_orphans(&self, count: u32) {
        for _ in 0..count {
            self.diagnostics.event(TelemetryEvent::Anomaly {
                kind: crate::diagnostics::AnomalyKind::AudioFormatOrphaned,
            });
        }
    }

    /// The injected id source. The import layer and the mappers mint row ids
    /// through this so tests get a deterministic-but-unique sequence; the
    /// playback queue mints per-instance `QueueEntryId`s through it on every
    /// platform.
    pub(crate) fn ids(&self) -> &IdRef {
        &self.ids
    }

    /// The library's on-disk directory. The import layer reads/writes sibling
    /// appdata files (e.g. the watched-folder registry) under it.
    /// Desktop-only: the import module that uses it is gated off iOS/Android,
    /// and playback reads blobs through coven's handle rather than this path.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn library_dir(&self) -> coven::StoreDir {
        self.config_handle.config().store_dir.clone()
    }

    /// Subscribe to the sync loop's status and turn it into library events: the
    /// banner state, and granular entity events for the rows an applied changeset
    /// touched. Call once after construction, with a tokio runtime available.
    pub fn start(&self) {
        let mut rx = self.handle.subscribe_sync_status();
        let lm = self.clone();
        self.runtime_handle.spawn(async move {
            while let Ok(status) = rx.recv().await {
                // Row changes ride only on a successful cycle that changed
                // data; they are a refresh hint, so the entity events they
                // drive are re-reads by primary key, not a trusted stream.
                if let SyncLoopStatus::Succeeded(success) = &status {
                    if let Some(row_changes) = &success.row_changes {
                        let (changes, missing_fk) =
                            crate::library::sync_events::changes_from_row_changes(row_changes);
                        for _ in 0..missing_fk {
                            lm.diagnostics().event(TelemetryEvent::Anomaly {
                                kind: crate::diagnostics::AnomalyKind::ChangesetMissingFk,
                            });
                        }
                        lm.emit_sync_entity_changes(changes).await;
                    }
                }
                // Fold coven's sync-loop status onto bae's flat banner state:
                // `Started` marks a cycle in progress; a terminal status ends it,
                // clearing the banner and recording the sync time on success, or
                // setting the banner on failure. `error_update` is `None` when the
                // status has no verdict on the banner (a plain `Started`).
                let syncing = matches!(status, SyncLoopStatus::Started);
                let (error_update, last_sync_update): (Option<Option<String>>, Option<String>) =
                    match &status {
                        SyncLoopStatus::Started => (None, None),
                        SyncLoopStatus::Succeeded(success) => {
                            (Some(None), Some(success.last_sync_time.clone()))
                        }
                        SyncLoopStatus::Failed { error } => (Some(Some(error.clone())), None),
                    };
                let mut emit_error = None;
                let mut emit_syncing = None;
                let mut emit_time = None;
                {
                    let mut state = lm.sync_status.lock().unwrap();
                    if let Some(error) = error_update {
                        if error != state.error {
                            state.error = error.clone();
                            emit_error = Some(error.map(crate::ui::UiError::internal));
                        }
                    }
                    if syncing != state.syncing {
                        state.syncing = syncing;
                        emit_syncing = Some(syncing);
                    }
                    if let Some(raw) = last_sync_update {
                        if state.last_sync_time_raw.as_deref() != Some(raw.as_str()) {
                            match crate::util::time::rfc3339_to_epoch_millis(&raw) {
                                Ok(ms) => {
                                    state.last_sync_time_raw = Some(raw);
                                    state.last_sync_time = Some(ms);
                                    emit_time = Some(state.last_sync_time);
                                }
                                Err(e) => {
                                    let message =
                                        format!("unparseable last_sync_time {raw:?}: {e}");
                                    warn!("{message}");
                                    emit_error = Some(Some(crate::ui::UiError::internal(message)));
                                }
                            }
                        }
                    }
                }
                if let Some(error) = emit_error {
                    // A newly-set banner (inner `Some`) is a sync-cycle failure;
                    // clearing it (inner `None`) is not. Ship the typed event
                    // only for an actual failure, and only when a provider is
                    // configured — a sync failure with no provider can't happen,
                    // so its absence means don't fabricate one.
                    if error.is_some() {
                        let provider = lm.config_handle.config().cloud_home.provider.clone();
                        if let Some(provider) = provider {
                            lm.diagnostics.event(TelemetryEvent::SyncFailed {
                                provider,
                                operation: SyncOperation::Cycle,
                            });
                        }
                    }
                    // coven's error string is opaque (connectivity, auth, storage);
                    // the UI shows a generic line plus this as copyable, log-only
                    // detail. `None` clears the banner.
                    lm.emit(LibraryEvent::SyncError { error });
                }
                if let Some(syncing) = emit_syncing {
                    lm.emit(LibraryEvent::SyncingChanged { syncing });
                }
                if let Some(time) = emit_time {
                    lm.emit(LibraryEvent::SyncTimeChanged { time });
                }
                // coven gives no per-item drain signal in the status, so re-derive
                // the outbox snapshot each cycle to catch what it uploaded or failed.
                lm.emit_outbox_changed().await;
            }
        });
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<LibraryEvent> {
        self.event_tx.subscribe()
    }

    /// Emit a library event to all subscribers. Logs at warn-level when no
    /// subscribers remain — the bus is alive for the lifetime of the library
    /// so empty subscribers is unusual and worth a trace.
    fn emit(&self, event: LibraryEvent) {
        if let Err(err) = self.event_tx.send(event) {
            warn!("library event broadcast had no subscribers: {err}");
        }
    }

    /// Build the current outbox snapshot and emit it as `OutboxChanged`. Called
    /// at every outbox mutation, once per sync cycle, and on each upload
    /// lifecycle callback so the Storage Manager's queue panel stays current.
    pub(crate) async fn emit_outbox_changed(&self) {
        self.sync.emit_outbox_changed().await
    }

    /// Build the current download-queue snapshot and emit it as
    /// `DownloadQueueChanged`. Called at every queue mutation (enqueue,
    /// worker pick-up, per-file progress, success, failure, cancel, retry,
    /// pause/resume) so the Storage Manager's Downloads pane stays current.
    pub(crate) fn emit_download_queue_changed(&self) {
        self.emit(LibraryEvent::DownloadQueueChanged {
            snapshot: self.download_snapshot(),
        });
    }

    /// The current download-queue snapshot — per-release state and a
    /// pre-formatted summary, built from the in-memory queue. Seeds the
    /// Downloads pane before the first `DownloadQueueChanged` event arrives.
    pub fn download_snapshot(&self) -> crate::library::DownloadSnapshot {
        crate::library::download_snapshot::build_download_snapshot(
            &self.download_queue.ops(),
            self.download_queue.is_paused(),
        )
    }

    /// Build the current export-queue snapshot and emit it as
    /// `OutputQueueChanged`. Called at every queue mutation (enqueue, worker
    /// pick-up, per-file progress, success, failure, cancel, retry,
    /// pause/resume) so the Storage Manager's Exporting pane stays current.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn emit_output_queue_changed(&self) {
        self.emit(LibraryEvent::OutputQueueChanged {
            snapshot: self.output_snapshot(),
        });
    }

    /// The current export-queue snapshot — per-release state built from the
    /// in-memory queue. Seeds the Exporting pane before the first
    /// `OutputQueueChanged` event arrives.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn output_snapshot(&self) -> crate::library::OutputSnapshot {
        crate::library::output_snapshot::build_output_snapshot(
            &self.output_queue.ops(),
            self.output_queue.is_paused(),
        )
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary. Seeds the Storage Manager panel before the first
    /// `OutboxChanged` event arrives.
    pub async fn outbox_snapshot(&self) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        self.sync.outbox_snapshot().await
    }

    // ── Fat-event emit helpers ───────────────────────────────────────
    // Each reads the current state of the entity post-mutation from the DB
    // and packs it into the event payload.

    pub async fn emit_album_added(&self, album_id: &str) {
        match self.find_album_detail(album_id).await {
            Ok(Some(album)) => {
                self.emit(LibraryEvent::AlbumAdded { album });
            }
            Ok(None) => {
                warn!("emit_album_added: album {album_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_album_added: DB error for album {album_id}: {e}");
            }
        }
    }

    pub async fn emit_album_updated(&self, album_id: &str) {
        match self.find_album_detail(album_id).await {
            Ok(Some(album)) => self.emit(LibraryEvent::AlbumUpdated { album }),
            Ok(None) => {
                warn!("emit_album_updated: album {album_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_album_updated: DB error for album {album_id}: {e}");
            }
        }
    }

    /// Re-emit `AlbumUpdated` for every album, on each cloud-home transition. A
    /// release's available storage actions are computed at resolve time from
    /// whether a cloud home exists and then baked into the cached `ReleaseDetail`,
    /// so connecting or disconnecting one leaves every already-resolved release
    /// holding stale actions until a restart. Re-resolving reads `has_cloud_home()`
    /// fresh. A burst, but connect/disconnect is rare.
    pub async fn emit_all_albums_updated(&self) {
        let albums = match self.get_albums(&[]).await {
            Ok(albums) => albums,
            Err(e) => {
                warn!("emit_all_albums_updated: failed to list albums: {e}");
                return;
            }
        };
        for album in albums {
            self.emit_album_updated(&album.id).await;
        }
    }

    pub fn emit_album_removed(&self, album_id: &str, release_ids: Vec<String>) {
        self.emit(LibraryEvent::AlbumRemoved {
            album_id: album_id.to_string(),
            release_ids,
        });
    }

    pub async fn emit_release_added(&self, album_id: &str, release_id: &str) {
        let release = match self.find_release_detail(release_id).await {
            Ok(Some(release)) => release,
            Ok(None) => {
                warn!("emit_release_added: release {release_id} not found in DB, skipping event");
                return;
            }
            Err(e) => {
                warn!("emit_release_added: DB error for release {release_id}: {e}");
                return;
            }
        };
        let raw_album = match self.database.find_album_summary(album_id).await {
            Ok(Some(raw)) => raw,
            Ok(None) => {
                warn!("emit_release_added: album {album_id} not found in DB, skipping event");
                return;
            }
            Err(e) => {
                warn!("emit_release_added: DB error for album {album_id}: {e}");
                return;
            }
        };
        // The release/album rows are already committed, so the event must fire for
        // them. A cover lookup failure degrades to no covers (the UI lazily fetches
        // them by id) — never drops the event for committed state.
        let covers = self
            .cover_refs(&raw_album.release_ids)
            .await
            .unwrap_or_else(|e| {
                warn!(
                    "emit_release_added: cover lookup failed for album {album_id}: {e}; \
                 emitting without covers"
                );
                HashMap::new()
            });
        let album = AlbumSummary::from_raw(raw_album, |rid| covers.get(rid).cloned());
        self.emit(LibraryEvent::ReleaseAdded { album, release });
    }

    pub async fn emit_release_updated(&self, album_id: &str, release_id: &str) {
        match self.find_release_detail(release_id).await {
            Ok(Some(release)) => {
                self.emit(LibraryEvent::ReleaseUpdated {
                    album_id: album_id.to_string(),
                    release,
                });
            }
            Ok(None) => {
                warn!("emit_release_updated: release {release_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_release_updated: DB error for release {release_id}: {e}");
            }
        }
    }

    pub async fn emit_release_removed(&self, album_id: &str, release_id: &str) {
        // Ship the parent album's post-removal summary so the reducer interns it
        // rather than reading the old release list to patch it — a read-to-write
        // that goes stale on the sync path, where no AlbumUpdated co-fires. `None`
        // means strictly "the album itself was removed with its last release"; a
        // transient cover-lookup failure must NOT misreport the album as gone, so
        // it degrades to a summary with no covers (the UI lazily fetches them).
        let album = match self.database.find_album_summary(album_id).await {
            Ok(Some(raw)) => {
                let covers = self.cover_refs(&raw.release_ids).await.unwrap_or_else(|e| {
                    warn!(
                        "emit_release_removed: cover lookup failed for album {album_id}: {e}; \
                         emitting without covers"
                    );
                    HashMap::new()
                });
                Some(AlbumSummary::from_raw(raw, |rid| covers.get(rid).cloned()))
            }
            Ok(None) => None,
            Err(e) => {
                warn!("emit_release_removed: DB error for album {album_id}: {e}");
                None
            }
        };
        self.emit(LibraryEvent::ReleaseRemoved {
            album_id: album_id.to_string(),
            release_id: release_id.to_string(),
            album,
        });
    }

    /// Emit granular library events for the entity changes an applied changeset
    /// produced.
    pub async fn emit_sync_entity_changes(
        &self,
        mut changes: crate::library::sync_events::ChangesetEntityChanges,
    ) {
        use crate::library::sync_events::{AlbumChangeEvent, ReleaseChangeEvent};

        // The changeset couldn't resolve these to albums itself: a track whose
        // release it didn't carry, or a cover whose release it didn't carry (a peer's
        // `change_cover` writes the `covers` row alone). Both become album updates —
        // the album payload carries its releases' tracks and cover refs — deduped
        // against the albums the changeset already produced an event for, so one
        // changeset never updates an album twice.
        let mut seen: std::collections::HashSet<String> = changes
            .album_events
            .iter()
            .map(|event| match event {
                AlbumChangeEvent::Added(id) | AlbumChangeEvent::Updated(id) => id.clone(),
                AlbumChangeEvent::Removed { album_id, .. } => album_id.clone(),
            })
            .collect();

        if !changes.unresolved_track_ids.is_empty() {
            match self
                .database
                .get_album_ids_for_tracks(&changes.unresolved_track_ids)
                .await
            {
                Ok(resolved) => {
                    for album_id in resolved.values() {
                        if seen.insert(album_id.clone()) {
                            changes
                                .album_events
                                .push(AlbumChangeEvent::Updated(album_id.clone()));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to resolve track IDs to album IDs: {e}");
                }
            }
        }

        if !changes.unresolved_release_ids.is_empty() {
            match self
                .database
                .get_album_ids_for_releases(&changes.unresolved_release_ids)
                .await
            {
                Ok(resolved) => {
                    for album_id in resolved.values() {
                        if seen.insert(album_id.clone()) {
                            changes
                                .album_events
                                .push(AlbumChangeEvent::Updated(album_id.clone()));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to resolve release IDs to album IDs: {e}");
                }
            }
        }

        for event in changes.album_events {
            match event {
                AlbumChangeEvent::Added(id) => self.emit_album_added(&id).await,
                AlbumChangeEvent::Updated(id) => self.emit_album_updated(&id).await,
                AlbumChangeEvent::Removed {
                    album_id,
                    release_ids,
                } => self.emit_album_removed(&album_id, release_ids),
            }
        }
        for event in changes.release_events {
            match event {
                ReleaseChangeEvent::Added {
                    album_id,
                    release_id,
                } => self.emit_release_added(&album_id, &release_id).await,
                ReleaseChangeEvent::Updated {
                    album_id,
                    release_id,
                } => self.emit_release_updated(&album_id, &release_id).await,
                ReleaseChangeEvent::Removed {
                    album_id,
                    release_id,
                } => self.emit_release_removed(&album_id, &release_id).await,
            }
        }
    }
}

/// The release-files [`BlobRef`](coven::BlobRef) addressing a representative
/// file for a coven cache-state query (the pin check via
/// [`CovenHandle::is_pinned`]), which keys only on namespace + id. The other
/// fields carry the release-files constants (see
/// [`LibraryManager::release_file_blob_ref`]); `cloud_path` is `None` because the
/// pin check reads only `storage/pinned/<namespace>/<id>`, never the cloud layout.
fn release_files_pin_ref(file_id: &str) -> coven::BlobRef {
    coven::BlobRef {
        namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
        id: file_id.to_string(),
        scope: coven::BlobScope::Master,
        cloud_path: None,
        provenance: coven::Provenance::UserProvided,
        fill: coven::CacheFill::CacheLazy,
    }
}

/// The pin-state answer for a release file. A structurally invalid blob id (e.g.
/// a path-traversal token forged by a peer) is `RejectedBadId` rather than
/// silently folded into `NotPinned`, so the caller that holds the diagnostics
/// sink can count it as an anomaly before treating it as not pinned.
enum ReleasePinState {
    Pinned,
    NotPinned,
    RejectedBadId,
}

/// Whether `file_id`'s release is pinned offline, answered through the handle's
/// cache-state query. A structurally invalid blob id can't name a real cached
/// blob, so it's rejected (never trusted); a real I/O failure still surfaces.
async fn release_file_pin_state(
    handle: &CovenHandle,
    file_id: &str,
) -> Result<ReleasePinState, LibraryError> {
    match handle.is_pinned(&[release_files_pin_ref(file_id)]).await {
        Ok(true) => Ok(ReleasePinState::Pinned),
        Ok(false) => Ok(ReleasePinState::NotPinned),
        Err(coven::BlobCacheError::Path(e)) => {
            warn!("pin-state check: rejecting bad blob id {file_id}: {e}");
            Ok(ReleasePinState::RejectedBadId)
        }
        Err(e) => Err(LibraryError::Storage(format!(
            "pin-state for {file_id}: {e}"
        ))),
    }
}

impl LibraryManager {
    /// The full cloud object key a cover blob lives at (namespace `covers`),
    /// derived through coven for the configured home's scheme. `blob_id` is the
    /// `covers` row's `blob_id` — the blob holding the bytes. Used by the bae-side
    /// cover delete path.
    fn cover_cloud_key(
        &self,
        blob_id: &str,
        cloud_path: Option<&str>,
    ) -> Result<String, LibraryError> {
        self.handle
            .blob_cloud_key(&Self::image_blob_ref(
                crate::sync::COVERS_NAMESPACE,
                blob_id,
                cloud_path.map(str::to_string),
            ))
            .map_err(|e| LibraryError::Storage(format!("cloud key for cover blob {blob_id}: {e}")))
    }

    /// Build the database cleanup writes and cache blob refs for a release delete
    /// without mutating durable state. The caller commits `db_cleanup` in the same
    /// DB transaction as the row deletion, then asks coven to drop any leftover
    /// on-device copies.
    async fn release_delete_plan(
        &self,
        release: &DbRelease,
    ) -> Result<ReleaseDeletePlan, LibraryError> {
        let release_id = &release.id;
        let files = self.get_files_for_release(release_id).await?;
        let mut evict_blobs = Vec::new();
        let mut cloud_delete_keys = Vec::new();
        let mut in_flight_make_remote_blobs = Vec::new();
        let mut external_blob_ids_to_clear = Vec::new();
        let mut make_remote_release_ids_to_clear = Vec::new();
        let make_remote_in_flight = !release.remote
            && self
                .database
                .has_make_remote_intent_for_release(release_id)
                .await?;

        if release.remote {
            evict_blobs.extend(files.iter().map(Self::release_file_blob_ref));
            cloud_delete_keys.extend(
                files
                    .iter()
                    .map(|file| self.release_file_cloud_key(file))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        } else if make_remote_in_flight {
            evict_blobs.extend(files.iter().map(Self::release_file_blob_ref));
            in_flight_make_remote_blobs.extend(
                files
                    .iter()
                    .map(|file| {
                        Ok(InFlightMakeRemoteBlobCleanup {
                            blob_id: file.id.clone(),
                            cloud_key: self.release_file_cloud_key(file)?,
                        })
                    })
                    .collect::<Result<Vec<_>, LibraryError>>()?,
            );
            external_blob_ids_to_clear.extend(files.iter().map(|file| file.id.clone()));
            make_remote_release_ids_to_clear.push(release_id.clone());
        } else {
            // Local: the files are the user's own files in place — never delete
            // them. Just clear coven's external refs in the delete transaction so
            // no orphan ref outlives the release row.
            external_blob_ids_to_clear.extend(files.iter().map(|file| file.id.clone()));
        }

        let cover = self
            .database
            .find_library_image(release_id, &LibraryImageType::Cover)
            .await?;

        if let Some(cover) = cover.as_ref().filter(|_| release.remote) {
            cloud_delete_keys
                .push(self.cover_cloud_key(&cover.blob_id, cover.cloud_path.as_deref())?);
        }

        if let Some(cover) = cover {
            evict_blobs.push(Self::image_blob_ref(
                crate::sync::COVERS_NAMESPACE,
                &cover.blob_id,
                cover.cloud_path.clone(),
            ));
        }

        Ok(ReleaseDeletePlan {
            db_cleanup: DeleteCleanupPlan {
                cloud_delete_keys,
                in_flight_make_remote_blobs,
                external_blob_ids_to_clear,
                make_remote_release_ids_to_clear,
            },
            evict_blobs,
        })
    }

    async fn evict_delete_blobs(&self, blobs: Vec<coven::BlobRef>) {
        for blob in blobs {
            if let Err(e) = self.handle.evict_blob(&blob).await {
                warn!(
                    "Failed to drop on-device copies during deletion for {}/{} ({:?}): {e}",
                    blob.namespace, blob.id, blob.cloud_path
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
