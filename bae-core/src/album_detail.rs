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
//! type, derivation has already happened — no consumer downstream (bridge,
//! events, UI) has to re-compute.
//!
//! ## Summary / detail composition
//!
//! For albums and releases, list views and detail views need different
//! projections of the same underlying entity. List views are "light": they
//! render a row and that's it. Detail views are "fat": tracks, files,
//! galleries, etc. We split each entity into two resolved types:
//!
//! - `AlbumSummary` / `ReleaseSummary` — slim. What a list row renders.
//! - `AlbumDetail`  / `ReleaseDetail`  — fat. Composes the summary with
//!   the heavy per-entity data loaded on demand.
//!
//! The `Detail` embeds the `Summary` rather than duplicating its fields —
//! consumers that hold a detail can treat it as a superset. Reducers on the
//! UI side can intern the summary portion into the "summaries" slice and the
//! detail portion into the "details" slice from a single event payload.
//!
//! Tracks carry a structured [`TrackPosition`] instead of pre-formatted prose.
//! A *side* is a contiguous playback unit (one CD disc, one vinyl face, one
//! cassette face); which case applies depends on the release's physical format.
//! The side letter (A/B/C…) and the disc-vs-side decision are domain logic and
//! stay here; only the words "Side"/"Disc" are the UI's, resolved from catalog
//! keys.

use crate::db::DbAlbum;

/// The TWO storage states a release's audio can be in — the shared
/// `releases.managed` fact. This is ORTHOGONAL to pinned-ness: whether coven keeps
/// a managed release's blobs local (`storage/pinned/`) vs evictable
/// (`storage/cache/`) is a separate per-device coven-cache property, carried
/// alongside this as a `pinned: bool` and NEVER folded into this enum. Mixing the
/// two would conflate "where the bytes live" with "is a copy kept offline."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageState {
    /// A local file the user owns, played in place; not in the cloud. Stays
    /// Unmanaged while uploading (the upload is a sub-state of unmanaged —
    /// `managed` only flips once every blob is in the cloud).
    Unmanaged,
    /// A cloud blob; coven's cache sits transparently in front of it. Whether a
    /// copy is kept offline on this device is the orthogonal `pinned` property.
    Managed,
}

/// The storage state for the shared `managed` fact. Pinned-ness is NOT part of
/// this — it is a separate coven-cache property the caller carries as a `pinned`
/// bool. Pure: the no-cloud-home overlay belongs to [`available_storage_actions`].
pub fn storage_state(managed: bool) -> ReleaseStorageState {
    if managed {
        ReleaseStorageState::Managed
    } else {
        ReleaseStorageState::Unmanaged
    }
}

/// A storage transition the user can trigger from the release "Storage…"
/// sheet. The core computes which are available; the UI renders them and
/// never re-derives availability. `Manage`/`Unmanage` move between the two storage
/// states; `Pin`/`Unpin` toggle the orthogonal coven-cache pin on a managed
/// release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStorageAction {
    /// Unmanaged → Managed (upload to the cloud home).
    Manage,
    /// Keep a managed release offline: fetch its blobs into coven's pinned cache.
    Pin,
    /// Stop keeping a managed release offline: drop its blobs from the pinned
    /// cache (still in the cloud).
    Unpin,
    /// Managed → Unmanaged (move the files back out to a user folder).
    Unmanage,
}

/// Which storage actions are available for a release, given its storage `state`
/// and the orthogonal `pinned` cache property.
///
/// "Managed" requires a cloud home, so with no cloud home there are no
/// transitions at all. The in-flight-uploads gate (acting mid-upload races the
/// observer that completes the managed transition) lives in the UI now: it
/// consults the outbox snapshot's `per_release` map and suppresses these actions
/// when the release has work in flight. Snapshot-driven gating stays fresh on
/// every queue mutation, whereas the previous core-side `has_pending_uploads` flag
/// baked a stale value into each cached `ReleaseDetail`.
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
        ReleaseStorageState::Unmanaged => vec![Manage],
        ReleaseStorageState::Managed if pinned => vec![Unpin, Unmanage],
        ReleaseStorageState::Managed => vec![Pin, Unmanage],
    }
}

/// Where a track sits in its release, in structured form. The case carries the
/// domain decision (sided physical medium vs. multi-disc digital vs. flat
/// single-disc digital); the UI composes the position string ("A1", "2-3", "5")
/// mechanically from the fields and resolves the "Side"/"Disc" header word from
/// a catalog key. A missing `number` means the source had no per-track number.
/// No prose lives here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackPosition {
    /// Vinyl/cassette: header "Side A", position "A1". `side_letter` is the
    /// letter for the face (A/B/C…); `number` is the within-side track number.
    Sided {
        side_letter: String,
        number: Option<i32>,
    },
    /// Multi-disc digital (CD etc.): header "Disc 2", position "2-3".
    Disc { disc: i32, number: Option<i32> },
    /// Single-disc digital: position "5", no header.
    Flat { number: Option<i32> },
}

/// A track group's side discriminant — what the UI renders as the "Side A" /
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
    /// Effective artist names for display — the track's own artists when it
    /// has per-track artist rows, otherwise the album artists. Always
    /// populated so UI consumers can render a row label without joining
    /// artist data themselves.
    pub artist_names: String,
    /// Structured position: the UI composes "A1"/"2-3"/"5" from the case.
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
/// ("FLAC · 44.1 kHz · 16-bit · stereo") from these parts: the codec is a
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

/// Resolved release summary: the slim projection that list views (storage
/// manager, release pickers, etc.) render one row per entity. Every field
/// is pre-computed; no downstream consumer needs to derive anything.
///
/// Composed into [`ReleaseDetail`] for detail views. Interned into the
/// UI-side "releases" slice — see notes on summary/detail composition at
/// the top of this file.
///
/// Invariant: `album_id` refers to an album that exists. Every release
/// belongs to an album (enforced by the `releases.album_id` FK and by
/// `delete_release`, which removes the album when its last release goes).
#[derive(Debug, Clone)]
pub struct ReleaseSummary {
    pub id: String,
    pub album_id: String,
    /// Audio format (e.g. "FLAC", "MP3"). `None` if unknown.
    pub format: Option<String>,
    /// The release's storage state — Unmanaged (local) or Managed (cloud) —
    /// derived from the shared `releases.managed` fact. Orthogonal to `pinned`.
    pub storage_state: ReleaseStorageState,
    /// Whether coven keeps this release's blobs pinned (kept offline) on this
    /// device — the orthogonal coven-cache property, asked of coven's cache.
    /// Meaningful only when `storage_state` is `Managed` (always `false` for an
    /// Unmanaged release, which is already a local file). Kept SEPARATE from
    /// `storage_state` so the two concepts are never confused.
    pub pinned: bool,
    /// Storage transitions available for this release right now, computed by
    /// the core from `storage_state`, `pinned`, and whether a cloud home exists.
    /// The UI renders these (the album-detail "Storage…" sheet and the Storage
    /// Manager row context menu); it never re-derives availability. Empty with
    /// no cloud home. The in-flight-uploads gate lives in the UI: it consults
    /// the outbox snapshot's `per_release` map before showing these actions.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
    /// Cache-bustable identifier for this release's own cover
    /// (`<path>#v=<mtime>`), or `None` when no cover is cached on disk. Keyed on
    /// the release id — covers are stored per release — so two releases of one
    /// album resolve to their own art rather than the album's primary cover.
    /// The image loader strips the `#v=…` suffix before opening the file.
    pub cover_path: Option<String>,
}

/// Resolved release detail: the fat projection for the album detail view.
/// Composes a [`ReleaseSummary`] (slim fields) with the per-release data
/// that only the detail view needs (tracks, files, gallery). Split this
/// way so a list consumer can display a row without loading tracks.
///
/// `display_name` is pre-computed: the release's own `release_name`, or a
/// "`year format`" derivation, or "Release $N" using the release's
/// position within its album. The resolver picks the position; consumers
/// never need the index.
///
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub summary: ReleaseSummary,
    /// Human-readable name for picker UI: the stored `release_name`, or
    /// "$year $format", or "Release $N" fallback.
    pub display_name: String,
    /// Raw `release_name` from the release row, preserved for wire
    /// compatibility with `BridgeRelease.release_name`. Consumers should
    /// prefer `display_name` for display.
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    /// Total duration across all tracks, in milliseconds. The UI formats it.
    pub total_duration_ms: i64,
    pub tracks: Vec<TrackDetail>,
    pub track_groups: Vec<TrackGroup>,
    pub files: Vec<FileDetail>,
    pub image_files: Vec<FileDetail>,
    /// Cover (if on disk) followed by every image file the release has —
    /// including cloud-only ones not yet downloaded (those carry no local path;
    /// the lightbox fetches them on demand).
    pub gallery_items: Vec<GalleryItem>,
}

/// One slot in a release's lightbox gallery.
#[derive(Debug, Clone)]
pub struct GalleryItem {
    /// Stable identifier: `"cover"` for the release cover, or the file id. For a
    /// cloud-only image this is the file id the lightbox passes back to fetch it.
    pub id: String,
    /// Display label: `"Cover"` or the file's original filename.
    pub label: String,
    /// Absolute local path when the image is on disk; `None` for a cloud-only
    /// image file that hasn't been downloaded yet.
    pub local_path: Option<String>,
}

/// Full album detail: album + releases (with tracks, files, gallery).
#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub album: DbAlbum,
    /// Comma-joined artist names for display.
    pub artist_names: String,
    pub releases: Vec<ReleaseDetail>,
    /// User-chosen primary release, falling back to first. Always set:
    /// every album has at least one release (enforced by `delete_release`
    /// which removes the album row when its last release is deleted).
    pub primary_release_id: String,
    /// Cache-bustable identifier for the album's cover (the primary release's
    /// image at `<path>#v=<mtime>`), or `None` when no cover is cached. The
    /// version moves when the cover bytes do, so the `AlbumUpdated` payload
    /// carries a changed field and the UI re-renders the cover.
    pub cover_path: Option<String>,
}

/// Resolved album summary: the slim projection list views render. Produced
/// by `LibraryManager` from `DbAlbumSummary`; carries the
/// `primary_release_id` fallback applied.
///
/// Composed into [`AlbumDetail`] for detail views. Interned into the
/// UI-side "summaries" slice alongside the summary/detail pattern
/// described at the top of this file.
#[derive(Debug, Clone)]
pub struct AlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_names: String,
    pub release_ids: Vec<String>,
    /// User-chosen primary release, or the first release if unset. Always
    /// set: every album has at least one release (enforced by
    /// `delete_release` which removes the album row when its last release
    /// is deleted).
    pub primary_release_id: String,
    /// Cache-bustable identifier for the album's cover (the primary release's
    /// image at `<path>#v=<mtime>`), or `None` when no cover is cached on disk.
    /// Carried on the summary so a cover change produces a changed field on the
    /// `AlbumUpdated` event: the version moves when the bytes do, so the UI's
    /// per-field re-render fires and the cover reloads. Stripping the `#v=…`
    /// suffix back to the file path happens in each platform's image loader.
    pub cover_path: Option<String>,
}

/// Resolved per-release storage summary for the Storage Manager view.
/// Produced by `LibraryManager` from `DbReleaseStorageSummary`: derives
/// `storage_state` from `releases.managed` and this device's
/// `release_local_copy` row. The UI formats `total_size` for the locale.
#[derive(Debug, Clone)]
pub struct ReleaseStorageSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_names: String,
    pub format: Option<String>,
    /// The album's primary release (for "am I the primary release"
    /// comparisons and cover art lookup). Always set: every album has at
    /// least one release.
    pub primary_release_id: String,
    /// The release's storage state — Unmanaged (local) or Managed (cloud) —
    /// derived from the shared `releases.managed` fact. Orthogonal to `pinned`.
    pub storage_state: ReleaseStorageState,
    /// Whether coven keeps this release's blobs pinned (kept offline) on this
    /// device — the orthogonal coven-cache property. Meaningful only when
    /// `storage_state` is `Managed`. Kept SEPARATE from `storage_state`.
    pub pinned: bool,
    /// The storage transitions this release allows now, gated on cloud-home
    /// only. The in-flight-uploads gate lives in the UI: it suppresses these
    /// actions when `OutboxSnapshot.per_release[release_id]` is non-empty.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
}

/// One row on the Storage Manager view: a release paired with its parent
/// album. The UI normalizes the shape into two slices (releases +
/// summaries) at ingest — rendering joins from the release to the album
/// at render time.
#[derive(Debug, Clone)]
pub struct StorageRow {
    pub release: ReleaseSummary,
    pub album: AlbumSummary,
}

/// One page of storage rows with the total count so paginated list
/// machinery knows where the end is. `total_count` reflects the filtered
/// subset, not the full library.
#[derive(Debug, Clone)]
pub struct StoragePage {
    pub rows: Vec<StorageRow>,
    pub total_count: u64,
}

/// Field the Storage Manager can sort on. Mirrors the columns
/// `StorageManagerView` renders today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortField {
    AlbumTitle,
    ArtistNames,
    Format,
    FileCount,
    TotalSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortDirection {
    Ascending,
    Descending,
}

/// A single sort criterion for `get_storage_page`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageSort {
    pub field: StorageSortField,
    pub direction: StorageSortDirection,
}

/// Filter applied to the Storage Manager list. Mirrors the four
/// mutually-exclusive filter chips the UI shows today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFilter {
    All,
    /// Only unmanaged releases (files live outside the library directory).
    Unmanaged,
    /// Only managed releases (files stored by the library).
    Managed,
    /// Only releases with at least one file pending cloud upload.
    Uploading,
}

/// Resolved search-result container produced by `LibraryManager` from
/// `DbLibrarySearchResults`.
#[derive(Debug, Clone)]
pub struct SearchResults {
    pub albums: Vec<AlbumSearchResult>,
    pub tracks: Vec<TrackSearchResult>,
}

/// Resolved album search result. Field-by-field copy of
/// `DbAlbumSearchResult` — no transformation, just the "public" name.
#[derive(Debug, Clone)]
pub struct AlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// User-chosen primary release, falling back to the album's first
    /// release. Always set: every album has at least one release.
    pub primary_release_id: String,
    pub artist_name: String,
}

/// Resolved track search result, produced from `DbTrackSearchResult`.
#[derive(Debug, Clone)]
pub struct TrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ReleaseStorageAction::*;
    use ReleaseStorageState::*;

    #[test]
    fn storage_state_is_just_the_managed_fact() {
        // Storage state is the 2-way `managed` fact; pinned-ness is orthogonal and
        // not part of it.
        assert_eq!(storage_state(false), Unmanaged);
        assert_eq!(storage_state(true), Managed);
    }

    #[test]
    fn no_cloud_home_has_no_actions() {
        for state in [Unmanaged, Managed] {
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
    fn unmanaged_with_cloud_offers_manage() {
        // `pinned` is irrelevant for an unmanaged release.
        assert_eq!(
            available_storage_actions(Unmanaged, false, true),
            vec![Manage]
        );
        assert_eq!(
            available_storage_actions(Unmanaged, true, true),
            vec![Manage]
        );
    }

    #[test]
    fn managed_pinned_offers_unpin_and_unmanage() {
        assert_eq!(
            available_storage_actions(Managed, true, true),
            vec![Unpin, Unmanage]
        );
    }

    #[test]
    fn managed_unpinned_offers_pin_and_unmanage() {
        assert_eq!(
            available_storage_actions(Managed, false, true),
            vec![Pin, Unmanage]
        );
    }
}
