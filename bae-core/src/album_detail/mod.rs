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

/// The two states a release's audio can be in — the shared `releases.remote` fact.
///
/// This is ORTHOGONAL to pinned-ness. Whether coven keeps a remote release's blobs
/// pinned or evictable is a separate per-device cache property, carried alongside
/// as a `pinned: bool` and NEVER folded in here: mixing them would conflate "where
/// the bytes live" with "is a copy kept offline."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageState {
    /// A local file the user owns, played in place. Stays Local throughout an
    /// upload — `remote` flips only once every blob is in the cloud.
    Local,
    /// A cloud blob, with coven's cache transparently in front of it.
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
/// `MakeRemote` / `MakeLocal` move between the two storage states; `Pin` / `Unpin`
/// toggle the orthogonal cache pin on a remote release. The core computes which are
/// available; the UI renders them and never re-derives availability.
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

/// Remote requires a cloud home, so without one there are no transitions at all.
///
/// The in-flight-uploads gate — acting mid-upload would race the observer that
/// completes the remote transition — lives in the UI instead, which suppresses
/// these actions when the outbox snapshot has work for the release. That stays
/// fresh on every queue mutation, where a core-side `has_pending_uploads` flag
/// would bake a stale value into each cached `ReleaseDetail`.
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

/// Where a track sits in its release. The variant carries the domain decision —
/// sided physical medium, multi-disc digital, or flat single-disc digital. Core
/// renders `position_text` from this; the UI only resolves the header word.
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

/// What the UI renders as the "Side A" / "Disc 2" header; `Flat` means no header.
/// Separate from [`TrackPosition`] because a header carries no per-track number —
/// every track on a side shares one `TrackSide`.
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

/// The tracks of one side. A single-side release has one `Flat` group.
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

/// The parts the UI composes into a one-line label ("FLAC · 44.1 kHz · 16-bit ·
/// stereo"): the codec is a proper noun, the channel count maps to a localized word,
/// and the numbers format per locale. A present `bits_per_sample` means lossless —
/// show the bit depth; absent means lossy — show `bitrate_kbps` instead.
#[derive(Debug, Clone)]
pub struct AudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}

/// A library image's reference: kind, subject id, and content version. The id is a
/// release id for a cover and an artist id for an artist image; the version is the
/// image row's `_updated_at`, which moves when the bytes change. The UI passes the
/// whole reference back to read bytes, so core dispatches to the right table rather
/// than probe every image namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub id: String,
    pub version: String,
    pub image_type: LibraryImageType,
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
