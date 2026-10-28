//! Resolved types for the album path — what `LibraryManager` returns and what
//! events carry across the bridge.
//!
//! They carry **formatted labels, resolved paths, derived flags, and pre-computed
//! totals**. `LibraryManager` produces them by combining raw DB aggregates (from
//! `crate::db`) with the pure formatters in `crate::util::format`; the bridge is
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
//! Tracks carry a core-rendered position string plus a structured [`TrackPosition`]
//! for grouping. A *side* is one contiguous playback unit — a CD disc, a vinyl face,
//! a cassette face — and which it is depends on the release's format. The side
//! letter, the disc-vs-side decision, and the position text are domain logic and
//! live here; only the words "Side" and "Disc" belong to the UI.
//!
//! ## Projection constructors
//!
//! Each resolved type owns its projection from the raw `Db*` aggregate as a
//! `from_raw`. `LibraryManager` gathers what the DB and coven's cache can tell it
//! (covers, pin state, cloud-home presence) and hands that in, so the derivation
//! lives with the type it produces.

use crate::db::LibraryImageType;

mod album;
mod artist;
mod composer;
mod file_kind;
mod playability;
mod release;
mod search;
mod source_audio;
mod storage;

pub use album::*;
pub use artist::*;
pub use composer::*;
pub use file_kind::{FileKind, FilePlace};
pub use playability::ReleasePlayability;
pub(crate) use playability::{
    recorded_originals, release_playability, OriginalOnDisk, OriginalsSeen,
};
pub use release::*;
pub use search::*;
pub use source_audio::*;
pub use storage::*;

/// What a release is called where a list of an album's releases names it.
/// Each surface words it in the reader's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseName {
    /// The name the person gave it.
    Named(String),
    /// No name: its year and media, whichever it states.
    Described {
        year: Option<i32>,
        media: Vec<bae_types::pressing::MediaCount>,
    },
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
            None if year.is_some() || !media.is_empty() => Self::Described {
                year,
                media: media.to_vec(),
            },
            None => Self::Numbered(release_number),
        }
    }
}

/// Whether every file of the release is in storage, so other devices can play
/// it.
///
/// This is ORTHOGONAL to pinned-ness. Whether coven keeps a remote release's files
/// pinned or evictable is a separate per-device cache property, carried alongside
/// as a `pinned: bool` and NEVER folded in here: mixing them would conflate "where
/// the bytes live" with "is a copy kept offline."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageState {
    /// A file storage doesn't hold yet: the release plays from its originals
    /// on the device that holds them, uploading or not.
    Local,
    /// Every file is in storage, read through coven's cache or storage.
    Remote,
}

/// Pure: the no-cloud-home overlay belongs to [`available_storage_actions`].
pub fn storage_state(remote: bool) -> ReleaseStorageState {
    if remote {
        ReleaseStorageState::Remote
    } else {
        ReleaseStorageState::Local
    }
}

/// A storage transition the user can trigger from the release "Storage…" sheet.
/// `Upload` / `DetachAudio` move between the two storage states; `Pin` / `Unpin`
/// toggle the orthogonal cache pin on a remote release. The core computes which are
/// available; the UI renders them and never re-derives availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageAction {
    /// Local -> Remote (upload to the cloud home).
    Upload,
    /// Keep a remote release offline: fetch its files into coven's pinned cache.
    Pin,
    /// Stop keeping a remote release offline: drop its files from the pinned
    /// cache (still in the cloud).
    Unpin,
    /// Remote -> Local (move the files back out to a user folder).
    DetachAudio,
}

/// Remote requires a cloud home, so without one there are no transitions at all.
///
/// `Upload` uploads from this device's originals, so it is offered only
/// where the release plays from them: not for a release another device
/// imported, nor one whose originals are gone or changed since import. It
/// admits work to coven's durable outbox; a stopped loop leaves that work
/// truthfully queued until a cycle can drain it, and does not remove the
/// action. Pin / Unpin / DetachAudio gate on the connected cloud home alone.
///
/// A release mid-transition offers none of these: its summary empties them
/// while its files wait to upload or a transfer runs.
pub fn available_storage_actions(
    state: ReleaseStorageState,
    playability: ReleasePlayability,
    pinned: bool,
    has_cloud_home: bool,
) -> Vec<ReleaseStorageAction> {
    use ReleaseStorageAction::*;
    if !has_cloud_home {
        return Vec::new();
    }
    match state {
        ReleaseStorageState::Local if playability == ReleasePlayability::Playable => {
            vec![Upload]
        }
        ReleaseStorageState::Local => Vec::new(),
        ReleaseStorageState::Remote if pinned => vec![Unpin, DetachAudio],
        ReleaseStorageState::Remote => vec![Pin, DetachAudio],
    }
}

/// What the album-detail download control shows for one release — the join of
/// the release's pin state, the transitions core offers it, and its entry in the
/// download queue.
///
/// Lives here, next to [`available_storage_actions`], because two of the rules it
/// encodes are properties of *that* gate rather than of any one app's control:
/// `Unpin` appears exactly when `pinned`, and `Pin` exactly when a remote release
/// is unpinned and there is a cloud home to pin from.
#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseDownloadStatus {
    /// Kept offline, and no download in flight.
    Downloaded,
    Queued,
    /// Bytes are arriving; how many is the release's download transfer.
    Downloading,
    Failed {
        error: String,
    },
    /// Not kept offline, but it could be.
    Available,
}

/// The control's state for one release, or `None` when there is no control to
/// show at all — no cloud home, or a release whose audio is already local.
///
/// **A live queue entry outranks `pinned`.** On pin success the pin marker
/// coven watches (which flips `pinned`) and the entry leaving the download
/// queue arrive in either order, so honoring the queue entry keeps the control
/// offering Cancel until the download has actually left the queue — instead of
/// flickering to Downloaded and back.
///
/// **Available means exactly "core offers Pin".** Deriving it from `pinned` alone
/// would re-answer a question [`available_storage_actions`] has already answered,
/// and would disagree with it the moment its gate changes. `Unpin` is not
/// consulted for the same reason in reverse: it appears exactly when `pinned`, so
/// `pinned` already says it. `DetachAudio` / `Upload` are desktop transitions
/// this control has nothing to do with.
pub fn release_download_status(
    pinned: bool,
    storage_actions: &[ReleaseStorageAction],
    download: Option<&crate::library::DownloadPhase>,
) -> Option<ReleaseDownloadStatus> {
    use crate::library::DownloadPhase;

    match download {
        Some(DownloadPhase::Queued) => Some(ReleaseDownloadStatus::Queued),
        Some(DownloadPhase::Active { progress: () }) => Some(ReleaseDownloadStatus::Downloading),
        Some(DownloadPhase::Failed { error }) => Some(ReleaseDownloadStatus::Failed {
            error: error.clone(),
        }),
        None if pinned => Some(ReleaseDownloadStatus::Downloaded),
        None if storage_actions.contains(&ReleaseStorageAction::Pin) => {
            Some(ReleaseDownloadStatus::Available)
        }
        None => None,
    }
}

/// How a track's place reads ([`crate::mediums::track_position`]): on a
/// lettered side of a record or cassette, on a numbered disc of a release of
/// several mediums, or plain on a release of one. Core renders `position_text`
/// from this; the UI only resolves the header word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackPosition {
    /// Header "Side A", position "A1". `side_letter` names the side among the
    /// release's sided mediums (A, B, … Z, AA, …); `number` is the track's
    /// number on that side.
    Sided { side_letter: String, number: u32 },
    /// Header "Disc 2", position "3" (the header carries the disc).
    Disc { disc: u32, number: u32 },
    /// Position "5", no header.
    Flat { number: u32 },
}

/// What the UI renders as the "Side A" / "Disc 2" header; `Flat` means no header.
/// Separate from [`TrackPosition`] because a header carries no per-track number —
/// every track on a side shares one `TrackSide`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackSide {
    Sided { side_letter: String },
    Disc { disc: u32 },
    Flat,
}

/// Track with resolved artist names and a structured display position.
#[derive(Debug, Clone)]
pub struct TrackDetail {
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    /// Effective artist names for display -- the track's own artists when it
    /// has per-track artist rows, otherwise the album artists. Always
    /// populated so UI consumers can render a row label without joining
    /// artist data themselves.
    pub artist_names: String,
    /// The artist to show on the track row, or `None` when the row should show
    /// none. Set only for a compilation, whose header names no single artist so
    /// each row needs its own; on a single-artist album the row would only repeat
    /// the header. Whether a row shows an artist is a domain decision, made here.
    pub display_artist: Option<String>,
    /// Core-rendered position string: "A1"/"3"/"5", or the stable prefix
    /// when the source has no track number.
    pub position_text: String,
    /// Structured position retained for side/disc grouping.
    pub position: TrackPosition,
}

/// The tracks of one side. A single-side release has one `Flat` group.
#[derive(Debug, Clone)]
pub struct TrackGroup {
    pub side: TrackSide,
    pub tracks: Vec<TrackDetail>,
    /// Playing time of this group's tracks. A multi-disc release shows it
    /// under each disc, where the album-wide total alone reads as belonging to
    /// the last disc.
    pub total_duration_ms: u64,
}

/// A file with pre-computed display fields.
#[derive(Debug, Clone, PartialEq)]
pub struct FileDetail {
    pub id: String,
    pub original_filename: String,
    /// Its name and folder within the release's folder.
    pub place: FilePlace,
    pub kind: FileKind,
    pub file_size: i64,
    pub is_image: bool,
    pub content_type: String,
    /// The scan's facts for this physical source file. `None` for non-audio.
    pub source_audio: Option<SourceAudioFile>,
    /// The file's upload while it waits in this device's upload queue.
    pub upload: Option<FileUpload>,
    /// Where this device's pin of the file stands, as coven's `coven_pins`
    /// says; `None` while it isn't pinned.
    pub pin: Option<FilePin>,
}

/// Where this device's pin of a file stands: kept whole on this device, or
/// waiting for its copy to arrive. Which of the files waiting coven is
/// fetching now, and how far each has got, coven doesn't say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePin {
    Kept,
    Waiting,
}

/// A release file's file waiting in this device's upload queue, read with the
/// release: which file coven uploads, whether this process is sending it now,
/// and the record of what it waits for. How far it has got is its file's
/// transfer, keyed by `file`.
#[derive(Debug, Clone, PartialEq)]
pub struct FileUpload {
    pub file: coven::FileId,
    pub phase: crate::library::UploadState,
    /// What it waits for, without the error behind it, which only this
    /// process holds.
    pub pending: Option<coven::PendingRecord>,
}

impl FileUpload {
    pub(crate) fn from_raw(raw: crate::db::DbFileUpload) -> Self {
        Self {
            file: raw.file,
            phase: if raw.sending {
                crate::library::UploadState::Uploading
            } else {
                crate::library::UploadState::Waiting
            },
            pending: raw.pending,
        }
    }
}

/// A library image's reference: kind, subject id, and content version. The id is a
/// release id for a cover and an artist id for an artist image; the version is the
/// content hash coven records for the image row's file, so it moves exactly when
/// the bytes do. The UI passes the
/// whole reference back to read bytes, so core dispatches to the right table rather
/// than probe every image namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub id: String,
    pub version: String,
    pub image_type: LibraryImageType,
}

/// Turn a subject-id -> content-version map — the shape the DB and every
/// live-query projection carry images in — into the [`ImageRef`]s a resolver
/// looks its subjects up in.
pub(crate) fn image_refs(
    versions: std::collections::HashMap<String, String>,
    image_type: LibraryImageType,
) -> std::collections::HashMap<String, ImageRef> {
    versions
        .into_iter()
        .map(|(id, version)| {
            (
                id.clone(),
                ImageRef {
                    id,
                    version,
                    image_type: image_type.clone(),
                },
            )
        })
        .collect()
}

/// Where a gallery slot's bytes come from. Each variant is self-contained: a cover
/// is read by its [`ImageRef`], a release-file image by its file id.
/// `read_gallery_bytes` dispatches on this, so the UI never picks a source itself.
#[derive(Debug, Clone)]
pub enum GallerySource {
    Cover(ImageRef),
    ReleaseFile { file_id: String },
}

/// One slot in a release's lightbox gallery. Read by id — there is no filesystem
/// path.
#[derive(Debug, Clone)]
pub struct GalleryItem {
    /// List identity only — `"cover"`, else the release-file id. The id used to
    /// *fetch* lives in `source`.
    pub id: String,
    /// `"Cover"`, or the file's original filename.
    pub label: String,
    pub source: GallerySource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReleasePlayability::*;
    use ReleaseStorageAction::*;
    use ReleaseStorageState::*;

    #[test]
    fn storage_state_is_just_the_remote_fact() {
        assert_eq!(storage_state(false), Local);
        assert_eq!(storage_state(true), Remote);
    }

    #[test]
    fn no_cloud_home_has_no_actions() {
        for state in [Local, Remote] {
            for pinned in [false, true] {
                assert_eq!(
                    available_storage_actions(state, Playable, pinned, false),
                    Vec::<ReleaseStorageAction>::new(),
                    "no cloud home blocks all actions for {state:?} (pinned={pinned})"
                );
            }
        }
    }

    #[test]
    fn local_with_cloud_offers_upload() {
        // `pinned` is irrelevant for a local release.
        assert_eq!(
            available_storage_actions(Local, Playable, false, true),
            vec![Upload]
        );
        assert_eq!(
            available_storage_actions(Local, Playable, true, true),
            vec![Upload]
        );
    }

    /// Uploading reads this device's originals, so a release another device
    /// imported, or one whose originals are gone, offers nothing to upload.
    #[test]
    fn local_without_its_originals_here_offers_nothing() {
        for playability in [NotUploaded, OriginalMissing] {
            assert_eq!(
                available_storage_actions(Local, playability, false, true),
                Vec::<ReleaseStorageAction>::new(),
                "{playability:?} offers no upload"
            );
        }
    }

    /// A stopped loop delays the upload; it does not prevent admitting the work.
    /// The durable outbox remains the truthful queued state while the loop is
    /// stopped and until the loop drains it.
    #[test]
    fn local_without_a_running_loop_still_offers_upload() {
        for pinned in [false, true] {
            assert_eq!(
                available_storage_actions(Local, Playable, pinned, true),
                vec![Upload],
                "Upload queues durably even with the sync loop stopped (pinned={pinned})"
            );
        }
    }

    /// A Remote release's actions move bytes through the connected manager's
    /// storage and need no drain loop.
    #[test]
    fn remote_actions_do_not_need_a_running_loop() {
        assert_eq!(
            available_storage_actions(Remote, Playable, true, true),
            vec![Unpin, DetachAudio]
        );
        assert_eq!(
            available_storage_actions(Remote, Playable, false, true),
            vec![Pin, DetachAudio]
        );
    }

    #[test]
    fn remote_pinned_offers_unpin_and_detach_audio() {
        assert_eq!(
            available_storage_actions(Remote, Playable, true, true),
            vec![Unpin, DetachAudio]
        );
    }

    #[test]
    fn remote_unpinned_offers_pin_and_detach_audio() {
        assert_eq!(
            available_storage_actions(Remote, Playable, false, true),
            vec![Pin, DetachAudio]
        );
    }

    /// The whole point of the join: on pin success the release value and
    /// the queue-drop snapshot arrive in either order, so while an entry is still
    /// in the queue it decides the control — even once `pinned` has flipped.
    #[test]
    fn a_queue_entry_outranks_pinned() {
        use crate::library::DownloadPhase;
        let cases = [
            (DownloadPhase::Queued, ReleaseDownloadStatus::Queued),
            (
                DownloadPhase::Active { progress: () },
                ReleaseDownloadStatus::Downloading,
            ),
            (
                DownloadPhase::Failed {
                    error: "read failed".to_string(),
                },
                ReleaseDownloadStatus::Failed {
                    error: "read failed".to_string(),
                },
            ),
        ];
        for (state, expected) in cases {
            for pinned in [false, true] {
                assert_eq!(
                    release_download_status(pinned, &[Unpin, DetachAudio], Some(&state),),
                    Some(expected.clone()),
                    "a {state:?} entry decides the control (pinned={pinned})"
                );
            }
        }
    }

    /// Pinned with nothing in flight is the settled offline state.
    #[test]
    fn pinned_with_an_empty_queue_is_downloaded() {
        assert_eq!(
            release_download_status(true, &[Unpin, DetachAudio], None,),
            Some(ReleaseDownloadStatus::Downloaded),
        );
    }

    /// Available is exactly "core offers Pin" — never re-derived from `pinned`.
    #[test]
    fn unpinned_with_pin_offered_is_available() {
        assert_eq!(
            release_download_status(false, &[Pin, DetachAudio], None,),
            Some(ReleaseDownloadStatus::Available),
        );
    }

    /// No Pin on offer means there is nothing to show: no cloud home, or the
    /// release's audio is already local.
    #[test]
    fn no_pin_on_offer_means_no_control() {
        assert_eq!(release_download_status(false, &[], None), None);
        assert_eq!(release_download_status(false, &[Upload], None,), None,);
    }
}
