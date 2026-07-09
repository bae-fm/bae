//! Resolved types for the album path: album detail, album summary, release
//! summary/detail, artist, track detail, track group, file detail, gallery,
//! storage summary/page, search results. These are what `LibraryManager`
//! returns and what events carry across the bridge.
//!
//! These types carry **formatted labels, resolved paths, derived flags, and
//! pre-computed totals**. The resolver in `LibraryManager` produces them by
//! combining raw DB aggregates (from `crate::db`) with the pure formatters in
//! `crate::util::format` and filesystem context (`library_dir`). The bridge
//! in `bae-bridge` is then a pure field-by-field copy to UniFFI types.
//!
//! The type split enforces the invariant: once a caller holds a resolved
//! type, derivation has already happened -- no consumer downstream (bridge,
//! events, UI) has to re-compute.
//!
//! ## Summary / detail composition
//!
//! For albums and releases, list views and detail views need different
//! projections of the same underlying entity. List views are "light": they
//! render a row and that's it. Detail views are "fat": tracks, files,
//! galleries, etc. We split each entity into two resolved types:
//!
//! - `AlbumSummary` / `ReleaseSummary` -- slim. What a list row renders.
//! - `AlbumDetail`  / `ReleaseDetail`  -- fat. Composes the summary with
//!   the heavy per-entity data loaded on demand.
//!
//! The `Detail` embeds the `Summary` rather than duplicating its fields --
//! consumers that hold a detail can treat it as a superset. Reducers on the
//! UI side can intern the summary portion into the "summaries" slice and the
//! detail portion into the "details" slice from a single event payload.
//!
//! Tracks carry a core-rendered position string plus a structured
//! [`TrackPosition`] for grouping.
//! A *side* is a contiguous playback unit (one CD disc, one vinyl face, one
//! cassette face); which case applies depends on the release's physical format.
//! The side letter (A/B/C...), the disc-vs-side decision, and the per-track
//! position text are domain logic and stay here; only the words "Side"/"Disc"
//! are the UI's, resolved from catalog keys.
//!
//! ## Projection constructors
//!
//! Each resolved type owns the pure projection from its raw `Db*` aggregate as a
//! `from_raw` constructor (a genuine `From` where there's no extra context). The
//! `LibraryManager` reads the DB / coven cache to gather the inputs (covers, pin
//! state, cloud-home presence) and hands them to these constructors; the
//! derivation logic lives with the type it produces.

use crate::db::{DbArtist, LibraryImageType};

mod album;
mod artist;
mod composer;
mod release;
mod search;
mod storage;

pub use album::*;
pub use artist::*;
pub use composer::*;
pub use release::*;
pub use search::*;
pub use storage::*;

/// Comma-join artist names for display. Shared by the `from_raw` projections
/// (track/album/release artist lists) and by the export path.
pub(crate) fn join_artist_names(artists: &[DbArtist]) -> String {
    artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn release_display_name(
    release_name: Option<&str>,
    year: Option<i32>,
    format: Option<&str>,
    release_number: i64,
) -> String {
    if let Some(name) = release_name {
        return name.to_string();
    }

    let mut parts = Vec::new();
    if let Some(year) = year {
        parts.push(year.to_string());
    }
    if let Some(format) = format {
        parts.push(format.to_string());
    }
    if parts.is_empty() {
        format!("Release {release_number}")
    } else {
        parts.join(" ")
    }
}

/// The TWO storage states a release's audio can be in -- the shared
/// `releases.remote` fact. This is ORTHOGONAL to pinned-ness: whether coven keeps
/// a remote release's blobs local (`storage/pinned/`) vs evictable
/// (`storage/cache/`) is a separate per-device coven-cache property, carried
/// alongside this as a `pinned: bool` and NEVER folded into this enum. Mixing the
/// two would conflate "where the bytes live" with "is a copy kept offline."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageState {
    /// A local file the user owns, played in place; not in the cloud. Stays
    /// Local while uploading (the upload is a sub-state of local --
    /// `remote` only flips once every blob is in the cloud).
    Local,
    /// A cloud blob; coven's cache sits transparently in front of it. Whether a
    /// copy is kept offline on this device is the orthogonal `pinned` property.
    Remote,
}

/// The storage state for the shared `remote` fact. Pinned-ness is NOT part of
/// this -- it is a separate coven-cache property the caller carries as a `pinned`
/// bool. Pure: the no-cloud-home overlay belongs to [`available_storage_actions`].
pub fn storage_state(remote: bool) -> ReleaseStorageState {
    if remote {
        ReleaseStorageState::Remote
    } else {
        ReleaseStorageState::Local
    }
}

/// A storage transition the user can trigger from the release "Storage..."
/// sheet. The core computes which are available; the UI renders them and
/// never re-derives availability. `MakeRemote`/`MakeLocal` move between the two storage
/// states; `Pin`/`Unpin` toggle the orthogonal coven-cache pin on a remote
/// release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageAction {
    /// Local -> Remote (upload to the cloud home).
    MakeRemote,
    /// Keep a remote release offline: fetch its blobs into coven's pinned cache.
    Pin,
    /// Stop keeping a remote release offline: drop its blobs from the pinned
    /// cache (still in the cloud).
    Unpin,
    /// Remote -> Local (move the files back out to a user folder).
    MakeLocal,
}

/// Which storage actions are available for a release, given its storage `state`
/// and the orthogonal `pinned` cache property.
///
/// "Remote" requires a cloud home, so with no cloud home there are no
/// transitions at all. The in-flight-uploads gate (acting mid-upload races the
/// observer that completes the remote transition) lives in the UI: it
/// consults the outbox snapshot's release groups and suppresses these actions
/// when the release has work in flight. Snapshot-driven gating stays fresh on
/// every queue mutation, where a core-side `has_pending_uploads` flag would bake
/// a stale value into each cached `ReleaseDetail`.
pub fn available_storage_actions(
    state: ReleaseStorageState,
    pinned: bool,
    has_cloud_home: bool,
) -> Vec<ReleaseStorageAction> {
    use ReleaseStorageAction::*;
    if !has_cloud_home {
        return Vec::new();
    }
    match state {
        ReleaseStorageState::Local => vec![MakeRemote],
        ReleaseStorageState::Remote if pinned => vec![Unpin, MakeLocal],
        ReleaseStorageState::Remote => vec![Pin, MakeLocal],
    }
}

/// Where a track sits in its release, in structured form. The case carries the
/// domain decision (sided physical medium vs. multi-disc digital vs. flat
/// single-disc digital). Core renders the track's `position_text` from this;
/// the UI only resolves the "Side"/"Disc" header word from a catalog key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackPosition {
    /// Vinyl/cassette: header "Side A", position "A1". `side_letter` is the
    /// letter for the face (A/B/C...); `number` is the within-side track
    /// number.
    Sided { side_letter: String, number: i32 },
    /// Vinyl/cassette track whose source has no per-track number. The side
    /// still determines grouping and the visible prefix.
    SidedUnnumbered { side_letter: String },
    /// Multi-disc digital (CD etc.): header "Disc 2", position "2-3".
    Disc { disc: i32, number: i32 },
    /// Multi-disc digital track whose source has no per-track number.
    DiscUnnumbered { disc: i32 },
    /// Single-disc digital: position "5", no header.
    Flat { number: i32 },
    /// Single-disc digital track whose source has no per-track number.
    Unnumbered,
}

/// A track group's side discriminant -- what the UI renders as the "Side A" /
/// "Disc 2" header. `Flat` is single-disc digital: one group, no header.
/// Separate from [`TrackPosition`] because a header carries no per-track number
/// (every track on a side shares one `TrackSide`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackSide {
    Sided { side_letter: String },
    Disc { disc: i32 },
    Flat,
}

/// Track with resolved artist names and a structured display position.
#[derive(Debug, Clone)]
pub struct TrackDetail {
    pub id: String,
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Effective artist names for display -- the track's own artists when it
    /// has per-track artist rows, otherwise the album artists. Always
    /// populated so UI consumers can render a row label without joining
    /// artist data themselves.
    pub artist_names: String,
    /// Core-rendered position string: "A1"/"2-3"/"5", or the stable prefix
    /// when the source has no track number.
    pub position_text: String,
    /// Structured position retained for side/disc grouping.
    pub position: TrackPosition,
}

/// A group of tracks sharing the same side. `side` is the group's discriminant
/// (the UI renders the "Side A" / "Disc 2" header from it; `Flat` means no
/// header). For single-side releases, there's one `Flat` group.
#[derive(Debug, Clone)]
pub struct TrackGroup {
    pub side: TrackSide,
    pub tracks: Vec<TrackDetail>,
}

/// A file with pre-computed display fields.
#[derive(Debug, Clone)]
pub struct FileDetail {
    pub id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub is_image: bool,
    pub content_type: String,
    /// Structured audio format. `None` for non-audio files (images, cue sheets)
    /// and for audio files with no stored format row.
    pub audio_format: Option<AudioFormat>,
}

/// Structured audio-format descriptor. The UI composes the one-line label
/// ("FLAC - 44.1 kHz - 16-bit - stereo") from these parts: the codec is a
/// proper noun, the channel count maps to a localized word, and the numbers
/// format per locale. `bits_per_sample` present means lossless (show the bit
/// depth); absent means lossy (show `bitrate_kbps` instead).
#[derive(Debug, Clone)]
pub struct AudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}

/// A host-provided library image's reference: the image kind, subject id, and
/// content version. The id is a release id for a cover and an artist id for an
/// artist image; the version is the image row's `_updated_at`, which moves when
/// the bytes change. The UI passes the whole reference back when reading bytes,
/// so core dispatches to the known table instead of probing image namespaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub id: String,
    pub version: String,
    pub image_type: LibraryImageType,
}

/// Which byte source a gallery slot is read from. Each variant is
/// self-contained: the release's own cover composes the [`ImageRef`] (id +
/// version) and is read by image id, a release-file image carries its own file
/// id and is read by it. Core dispatches the byte read on this
/// (`read_gallery_bytes`) so the UI never picks the source itself.
#[derive(Debug, Clone)]
pub enum GallerySource {
    Cover(ImageRef),
    ReleaseFile { file_id: String },
}

/// One slot in a release's lightbox gallery. Every slot is read by id -- there is
/// no filesystem path -- and `source` names which byte source core reads from.
#[derive(Debug, Clone)]
pub struct GalleryItem {
    /// Stable list/ForEach identity only: `"cover"` for the cover slot, otherwise
    /// the release-file id. The fetch id lives in `source`.
    pub id: String,
    /// Display label: `"Cover"` or the file's original filename.
    pub label: String,
    /// Which byte source to read this slot from.
    pub source: GallerySource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReleaseStorageAction::*;
    use ReleaseStorageState::*;

    #[test]
    fn storage_state_is_just_the_remote_fact() {
        // Storage state is the 2-way `remote` fact; pinned-ness is orthogonal and
        // not part of it.
        assert_eq!(storage_state(false), Local);
        assert_eq!(storage_state(true), Remote);
    }

    #[test]
    fn no_cloud_home_has_no_actions() {
        for state in [Local, Remote] {
            for pinned in [false, true] {
                assert_eq!(
                    available_storage_actions(state, pinned, false),
                    Vec::<ReleaseStorageAction>::new(),
                    "no cloud home blocks all actions for {state:?} (pinned={pinned})"
                );
            }
        }
    }

    #[test]
    fn local_with_cloud_offers_manage() {
        // `pinned` is irrelevant for a local release.
        assert_eq!(
            available_storage_actions(Local, false, true),
            vec![MakeRemote]
        );
        assert_eq!(
            available_storage_actions(Local, true, true),
            vec![MakeRemote]
        );
    }

    #[test]
    fn remote_pinned_offers_unpin_and_unmanage() {
        assert_eq!(
            available_storage_actions(Remote, true, true),
            vec![Unpin, MakeLocal]
        );
    }

    #[test]
    fn remote_unpinned_offers_pin_and_unmanage() {
        assert_eq!(
            available_storage_actions(Remote, false, true),
            vec![Pin, MakeLocal]
        );
    }
}
