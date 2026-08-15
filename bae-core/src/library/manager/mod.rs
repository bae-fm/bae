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
    DbLibraryImage, DbRelease, DbTrack, DeleteCleanupPlan, LibraryImageType, Pressing,
};
use crate::diagnostics::{Diagnostics, SyncOperation, TelemetryEvent};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::library::save::SaveService;
use crate::library::sync_controller::SyncController;
use crate::playback::QueueEntry;
use crate::queue::QueueItem;
use crate::sync::S3ConfigData;
use coven::ClockRef;
#[cfg(any(test, feature = "test-utils"))]
use coven::ExactCloudHome;
use coven::IdRef;
use coven::SyncLoopStatus;

/// Transient library events can burst during imports and sync catch-up.
mod service;
mod storage_operations;
use storage_operations::*;
const LIBRARY_EVENT_CHANNEL_CAPACITY: usize = 1024;

mod album;
mod artist;
mod composer;
mod config;
mod coven_blobs;
mod discogs;
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use discogs::discogs_validation_from_result;

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
    Config(#[from] crate::config::ConfigError),
    /// Building or probing a cloud home failed — bad credentials/config
    /// (`Configuration`) or an unreachable backend (`Transport`).
    #[error("Cloud home error: {0}")]
    CloudHome(#[from] coven::CloudHomeError),
    #[error("Cloud setup error: {0}")]
    CloudSetup(#[from] coven::CloudHomeSetupError),
    #[error("Cloud unlock error: {0}")]
    CloudUnlock(#[from] coven::CloudHomeUnlockError),
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
    /// Establishing this store's device identity failed.
    #[error("Identity error: {0}")]
    Identity(#[from] coven::IdentityError),
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
            LibraryError::CloudSetup(error) => cloud_setup_category(error),
            LibraryError::CloudUnlock(error) => cloud_unlock_category(error),
            LibraryError::Sync(e) => sync_category(e),
            LibraryError::Import(_) | LibraryError::Edit(_) => C::Import,
            LibraryError::Export(_) => C::Export,
            LibraryError::Save(_) => C::Save,
            LibraryError::MasterKey(_) | LibraryError::Identity(_) => C::Keyring,
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

fn cloud_setup_category(error: &coven::CloudHomeSetupError) -> crate::ui::UiErrorCategory {
    cloud_setup_failure_category(error.failure())
}

fn cloud_setup_failure_category(
    failure: coven::CloudHomeSetupFailure,
) -> crate::ui::UiErrorCategory {
    crate::ui::UiErrorCategory::CloudSetup(failure)
}

fn cloud_unlock_category(error: &coven::CloudHomeUnlockError) -> crate::ui::UiErrorCategory {
    use coven::CloudHomeUnlockError;
    match error {
        CloudHomeUnlockError::Connection(error) => sync_category(error),
        CloudHomeUnlockError::Rollback { failure, .. } => cloud_unlock_category(failure),
        CloudHomeUnlockError::KeyNotRequired => crate::ui::UiErrorCategory::Config,
        CloudHomeUnlockError::MasterKey(_) | CloudHomeUnlockError::Commit(_) => {
            crate::ui::UiErrorCategory::Keyring
        }
    }
}

/// Classify a coven sync/membership failure into a user-facing class: keyring vs
/// cloud credentials/network vs the membership chain itself.
fn sync_category(error: &coven::SyncError) -> crate::ui::UiErrorCategory {
    use crate::ui::UiErrorCategory as C;
    use coven::SyncError;
    if error.is_retryable() {
        return C::Network;
    }
    match error {
        SyncError::Key(coven::KeyError::NoDeviceIdentity) => C::DeviceIdentityMissing,
        SyncError::Key(_) => C::Keyring,
        SyncError::CloudHome(e) => cloud_home_category(e),
        SyncError::Setup(_) => C::Credentials,
        SyncError::Membership(_) => C::Membership,
        SyncError::DeviceJoin(_) => C::Membership,
        // Admitting a device: a malformed join-request code, or the handshake's
        // storage transport failing (including the deadline that means the other
        // device never took its step). Both are the membership operation failing,
        // not the library or this device's credentials.
        SyncError::InvalidJoinRequest(_) => C::Membership,
        SyncError::DeviceJoinTransport(_) => C::Membership,
        // The other membership operations that carry a pasted/scanned code —
        // excluding a device from the store, promoting a member to owner — and a
        // code that doesn't decode as the operation it was pasted into. Same
        // class as an invalid join request: the membership operation failed, not
        // the library or this device's credentials.
        SyncError::InvalidMembershipOperationCode(_) => C::Membership,
        SyncError::DeviceExclusion(_) => C::Membership,
        SyncError::OwnerPromotion(_) => C::Membership,
        SyncError::StorageSetup(_) => C::Network,
        SyncError::NotConfigured
        | SyncError::LoopNotRunning
        | SyncError::NotEncryptedHome
        | SyncError::MasterKeyNotEstablished
        | SyncError::Init(_)
        | SyncError::Store(_)
        | SyncError::Circle(_)
        | SyncError::Database(_)
        | SyncError::RoutingEncryption(_)
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
    fn initial(database: &Database) -> Self {
        Self {
            error: None,
            last_sync_time_raw: None,
            last_sync_time: None,
            syncing: database.is_syncing(),
        }
    }
}

struct ReleaseDeletePlan {
    db_cleanup: DeleteCleanupPlan,
    evict_blobs: Vec<coven::RowBlobRef>,
    /// The release was mid-make-remote, so coven must unwind that transition
    /// (intent, queued uploads, and any cloud object already written) before the
    /// rows go.
    cancel_make_remote: bool,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) struct ImportReplacementPlan {
    pub(crate) db_delete: crate::db::ImportReplacementDelete,
    pub(crate) evict_blobs: Vec<coven::RowBlobRef>,
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
    /// Where this segment sits inside its backing file, in samples and bytes.
    pub span: crate::db::SegmentSpan,
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
    /// Bits per sample as stored at import (16, 24, …). `None` for lossy codecs,
    /// where the source has no fixed sample depth. Carried so callers that
    /// describe the audio to a client (the Subsonic server's `bitDepth`) don't
    /// re-fetch the raw audio-format row.
    pub bits_per_sample: Option<u32>,
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
                    span: segment.span(),
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
            bits_per_sample: meta
                .audio_format
                .bits_per_sample
                .and_then(|bits| u32::try_from(bits).ok()),
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

/// Removes the transfer action from the value stream when the transfer future
/// completes or is dropped.
struct TransferValueGuard {
    transfer_actions: Arc<Mutex<HashMap<String, ReleaseStorageAction>>>,
    transfer_values: tokio::sync::watch::Sender<HashMap<String, ReleaseStorageAction>>,
    release_id: String,
}

impl Drop for TransferValueGuard {
    fn drop(&mut self) {
        let actions = {
            let mut actions = self.transfer_actions.lock().unwrap();
            actions.remove(&self.release_id);
            actions.clone()
        };
        self.transfer_values.send_replace(actions);
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

/// Transient operation events emitted by `LibraryManager`.
#[derive(Clone, Debug)]
pub enum LibraryEvent {
    TracksDeleted { track_ids: Vec<String> },
}
/// Persistence and queries for albums, tracks, and files: import state
/// transitions, library browsing, and deletion with cloud-storage cleanup.
#[derive(Clone)]
pub struct LibraryManager {
    database: Database,
    config_handle: Arc<ConfigHandle>,
    clock: ClockRef,
    ids: IdRef,
    /// Typed telemetry sink, injected at bootstrap alongside the clock/id
    /// sources. The sync-failure path emits through it; the playback and import
    /// services read it back off this manager for their own events.
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    event_tx: broadcast::Sender<LibraryEvent>,
    /// The cloud-sync responsibility: the upload pipeline (outbox in-flight,
    /// throughput, pause), provider connection, membership, and the coven
    /// make-Remote/make-Local primitives.
    sync: SyncController,
    sync_status: Arc<Mutex<SyncStatusState>>,
    sync_status_values: tokio::sync::watch::Sender<crate::library::SyncStatusSnapshot>,
    outbox_values:
        tokio::sync::watch::Sender<Option<Result<crate::library::OutboxSnapshot, String>>>,
    download_values: tokio::sync::watch::Sender<crate::library::DownloadSnapshot>,
    /// Cancellation tokens for in-progress foreground transfers (unmanage),
    /// keyed by release id. `cancel_release_transition` fires the token; the
    /// transfer observes it between files, deletes the partial copies it wrote,
    /// and leaves the release remote (no orphans). Registered for the transfer's
    /// duration; transient.
    transfer_cancels: Arc<Mutex<HashMap<String, crate::library::CancellationToken>>>,
    transfer_actions: Arc<Mutex<HashMap<String, ReleaseStorageAction>>>,
    transfer_values: tokio::sync::watch::Sender<HashMap<String, ReleaseStorageAction>>,
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
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    output_values: tokio::sync::watch::Sender<crate::library::OutputSnapshot>,
    /// The upload observer coven reports blob transitions to. coven holds only a
    /// `Weak` to it (through `WeakUploadObserver`), so this strong `Arc` is its
    /// sole owner and its lifetime is the manager's. Its event sender feeds a task
    /// that owns a `SyncController`; dropping the last manager clone drops this
    /// sender, ends that task, and releases its database clone and store-open lock.
    /// Registering the observer strongly in coven would close that cycle. Held for
    /// its lifetime and read only by named test operations, so it carries the
    /// leading underscore.
    _upload_observer: Arc<crate::sync::upload_observer::ReleaseUploadObserver>,
    /// Bytes of provider art (Cover Art Archive, Discogs) that isn't in the
    /// library, cached under HTTP freshness rules. It lives here because all
    /// three readers — the cover picker, the import commit worker, and
    /// `change_cover` — reach it through a manager clone, so picking a remote
    /// cover and then importing it downloads once.
    remote_images: crate::import::cover_art::RemoteImageCache,
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
