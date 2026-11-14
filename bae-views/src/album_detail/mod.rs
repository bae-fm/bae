//! Resolved types for the album path — what the library's views return and
//! what events carry across the bridge.
//!
//! They carry **formatted labels, resolved paths, derived flags, and pre-computed
//! totals**. The library's views produce them by combining raw DB aggregates (from
//! `bae_records`) with the position formatters of `bae_types::mediums`; the bridge is
//! then a field-by-field copy to UniFFI types. That split is the invariant: once a
//! caller holds a resolved type, derivation has already happened, and no consumer
//! downstream re-computes anything.
//!
//! ## Summary / detail composition
//!
//! A list row and a detail view need different projections of one entity, so albums
//! and releases each have two types: a slim `AlbumSummary` / `ReleaseSummary` that
//! renders a row, and a fat `AlbumDetail` / `ReleaseDetail` that adds tracks, files,
//! and gallery. The detail *embeds* its summary rather than duplicating its fields,
//! so a consumer holding a detail can treat it as a superset — and a UI reducer can
//! intern the summary half and the detail half from one event payload.
//!
//! Tracks carry a core-rendered position string plus a structured `TrackPosition`
//! for grouping. A *side* is one contiguous playback unit — a CD disc, a vinyl face,
//! a cassette face — and which it is depends on the release's format. The side
//! letter, the disc-vs-side decision, and the position text are domain logic and
//! live here; only the words "Side" and "Disc" belong to the UI.
//!
//! ## Projection constructors
//!
//! Each resolved type owns its projection from the raw `Db*` aggregate as a
//! `from_raw`. The library's views gather what the DB and coven's cache can tell
//! them (covers, pin state, whether the library syncs) and hand that in, so the derivation
//! lives with the type it produces.

use bae_records::source_audio::{SourceAudioFile, SourceAudioSummary};
use bae_records::{cover_ref, GallerySource, ImageRef};
use bae_types::release_storage::ReleaseStorageAction;

mod album;
mod artist;
mod composer;
mod credits;
mod file_kind;
mod playability;
mod release;
mod search;
mod storage;
mod track;

pub use album::*;
pub use artist::*;
pub use bae_people::collections::Stars;
pub use composer::*;
pub use credits::CreditRow;
pub use file_kind::{FileKind, FilePlace};
pub(crate) use playability::release_playability;
pub use playability::ReleasePlayability;
pub use release::*;
pub use search::*;
pub use storage::*;
pub(crate) use track::group_tracks_by_side;
pub use track::{TrackDetail, TrackGroup, TrackWork};

/// What a release is called where a list of an album's releases names it.
/// Each surface words it in the reader's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseName {
    /// The name the person gave it.
    Named(String),
    /// No name: its year and media, whichever it states.
    Described { year: Option<i32>, media: Vec<bae_types::pressing::MediaCount> },
    /// No name and nothing to describe it by: its place among the album's
    /// releases, counted from one.
    Numbered(i64),
}

impl ReleaseName {
    /// The stored `release_name`, else the year and media it states, else its
    /// place among the album's releases.
    pub(crate) fn of(
        release_name: Option<&str>,
        year: Option<i32>,
        media: &[bae_types::pressing::MediaCount],
        release_number: i64,
    ) -> Self {
        match release_name {
            Some(name) => Self::Named(name.to_string()),
            None if year.is_some() || !media.is_empty() => {
                Self::Described { year, media: media.to_vec() }
            }
            None => Self::Numbered(release_number),
        }
    }
}

/// Where a release's files are: on the one device that holds its originals,
/// or in storage, where every device reads them, with whether this device
/// keeps its own copy of every file (pinned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageState {
    /// A file storage doesn't hold yet: the release plays from its originals
    /// on the device that holds them, uploading or not.
    OnOneDevice,
    /// Every file is in storage, read through coven's cache or storage.
    InStorage {
        /// Whether this device keeps every file of it offline.
        pinned: bool,
    },
}

/// The state a release's listing gives: whether storage holds every file,
/// and whether this device pins every file. Only a file in storage is
/// pinned, so a release storage lacks a file of is never pinned whole.
pub fn storage_state(stored: bool, pinned: bool) -> ReleaseStorageState {
    if stored {
        ReleaseStorageState::InStorage { pinned }
    } else {
        ReleaseStorageState::OnOneDevice
    }
}

/// Storage requires the library to sync, so without it there are no transitions at
/// all.
///
/// `Upload` uploads from this device's originals, so it is offered only
/// where the library records this device holding them: not for a release
/// another device imported. An original gone or changed since import fails
/// the move when it reads the file. It
/// admits work to coven's durable outbox; a stopped loop leaves that work
/// truthfully queued until a cycle can drain it, and does not remove the
/// action. Pin / Unpin / MoveToThisDevice gate on the library syncing alone.
///
/// A release mid-transition offers none of these: its summary empties them
/// while its files wait to upload or a transfer runs.
pub fn available_storage_actions(
    state: ReleaseStorageState,
    playability: &ReleasePlayability,
    syncs: bool,
) -> Vec<ReleaseStorageAction> {
    use ReleaseStorageAction::*;
    if !syncs {
        return Vec::new();
    }
    match state {
        ReleaseStorageState::OnOneDevice if *playability == ReleasePlayability::Playable => {
            vec![Upload]
        }
        ReleaseStorageState::OnOneDevice => Vec::new(),
        ReleaseStorageState::InStorage { pinned: true } => vec![Unpin, MoveToThisDevice],
        ReleaseStorageState::InStorage { pinned: false } => vec![Pin, MoveToThisDevice],
    }
}

/// What the album-detail download control shows for one release — the join of
/// the release's pin state, the transitions core offers it, and its entry in the
/// download queue.
///
/// Lives here, next to [`available_storage_actions`], because two of the rules it
/// encodes are properties of *that* gate rather than of any one app's control:
/// `Unpin` appears exactly when the release is pinned, and `Pin` exactly when a
/// release in storage is not and the library syncs, to pin from.
#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseDownloadStatus {
    /// Kept offline, and no download in flight.
    Downloaded,
    /// Waiting its turn.
    Queued,
    /// Bytes are arriving; how many is the release's download transfer.
    Downloading,
    Failed {
        error: String,
    },
    /// Not kept offline, but it could be.
    Available,
}

/// Where a release stands in being moved to this device: its entry in the download
/// queue, when it is a move-to-this-device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveToThisDeviceStatus {
    /// Waiting its turn.
    Queued,
    /// Downloading its files into the person's folder. How many bytes is the
    /// release's download transfer.
    Making,
    /// Stopped by its error, until retried.
    Failed { error: String },
}

/// The release's move-to-this-device status from its entry in the download queue;
/// `None` when the entry is not a move-to-this-device, or there is none.
pub fn release_move_to_this_device_status(
    download: Option<&bae_transfers::DownloadEntry>,
) -> Option<MoveToThisDeviceStatus> {
    use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep};

    let Some(DownloadEntry { kind: DownloadKind::MoveToThisDevice, step }) = download else {
        return None;
    };
    Some(match step {
        DownloadStep::Queued => MoveToThisDeviceStatus::Queued,
        DownloadStep::Running => MoveToThisDeviceStatus::Making,
        DownloadStep::Failed { error } => MoveToThisDeviceStatus::Failed { error: error.clone() },
    })
}

/// The control's state for one release, or `None` when there is no control to
/// show at all — no storage, or a release whose audio is already local.
///
/// **Available means exactly "core offers Pin".** Deriving it from `pinned` alone
/// would re-answer a question [`available_storage_actions`] has already answered,
/// and would disagree with it the moment its gate changes. `Unpin` is not
/// consulted for the same reason in reverse: it appears exactly when `pinned`, so
/// `pinned` already says it. `MoveToThisDevice` / `Upload` are desktop transitions
/// this control has nothing to do with.
pub fn release_download_status(
    state: ReleaseStorageState,
    storage_actions: &[ReleaseStorageAction],
    download: Option<&bae_transfers::DownloadEntry>,
) -> Option<ReleaseDownloadStatus> {
    use bae_transfers::{DownloadEntry, DownloadKind, DownloadStep};

    match download {
        Some(DownloadEntry { kind: DownloadKind::Pin, step }) => Some(match step {
            DownloadStep::Queued => ReleaseDownloadStatus::Queued,
            DownloadStep::Running => ReleaseDownloadStatus::Downloading,
            DownloadStep::Failed { error } => {
                ReleaseDownloadStatus::Failed { error: error.clone() }
            }
        }),
        // Making the release local is not this control's to show.
        Some(DownloadEntry { kind: DownloadKind::MoveToThisDevice, .. }) => None,
        None if state == (ReleaseStorageState::InStorage { pinned: true }) => {
            Some(ReleaseDownloadStatus::Downloaded)
        }
        None if storage_actions.contains(&ReleaseStorageAction::Pin) => {
            Some(ReleaseDownloadStatus::Available)
        }
        None => None,
    }
}

/// A file with pre-computed display fields.
#[derive(Debug, Clone, PartialEq)]
pub struct FileDetail {
    pub id: String,
    pub original_filename: String,
    /// Its name and folder within the release's folder.
    pub place: FilePlace,
    pub kind: FileKind,
    pub file_size: u64,
    pub is_image: bool,
    pub content_type: String,
    /// The scan's facts for this physical source file. `None` for non-audio.
    pub source_audio: Option<SourceAudioFile>,
}

impl FileDetail {
    /// The release file `f` as a list of the release's files shows it.
    pub fn from_raw(f: bae_records::DbFile) -> Self {
        FileDetail {
            place: FilePlace::of(&f.original_filename),
            kind: FileKind::of(f.role, &f.content_type),
            is_image: f.content_type.is_image(),
            content_type: f.content_type.to_string(),
            source_audio: f.source_audio,
            id: f.id,
            original_filename: f.original_filename,
            file_size: f.file_size,
        }
    }
}

/// Where this device's pin of a file stands: kept whole on this device,
/// downloading now to be kept, as coven's `coven_downloading` lists it, or
/// waiting for the download queue's pin of its release to fetch it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePin {
    Kept,
    /// How far it has got is its file's transfer, keyed by `file`, the file
    /// coven downloads.
    Downloading {
        file: coven::FileId,
    },
    Waiting,
}

/// One slot in a release's lightbox gallery. Read by id — there is no filesystem
/// path.
#[derive(Debug, Clone)]
pub struct GalleryItem {
    /// List identity only — `"cover"`, else the release-file id. The id used to
    /// *fetch* lives in `source`.
    pub id: String,
    pub label: GalleryLabel,
    pub source: GallerySource,
}

/// What a gallery slot is called. The surface words the cover; a file is
/// called by its original filename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GalleryLabel {
    Cover,
    File { name: String },
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
