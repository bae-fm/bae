//! Raw DB-shape types: `Db*` rows mirroring a table row, and
//! `Db*Detail` / `Db*Summary` / `Db*SearchResult` aggregates whose shape is a
//! join query's result — stitched together in SQL, not in Rust.
//!
//! **No formatting, no Rust-side derivation, no filesystem access.** Every
//! `*_label`, sum-of-fields, group-by, path resolution, `has_X` flag, or
//! `stored.or_else(first)` fallback belongs in the manager, not here.
//!
//! `LibraryManager` in `crate::library::manager` is the resolver: it turns
//! these into the display-ready `AlbumDetail`, `ReleaseDetail`, `AlbumSummary`,
//! `ReleaseStorageSummary`, and `SearchResults` of `crate::album_detail`, which
//! is what the bridge and event payloads carry. (`crate::queue::QueueItem` is
//! built directly by the queue catalog read — it has no raw counterpart here.)

use crate::catalog::Catalog;
use bae_util::content_type::ContentType;
use chrono::{DateTime, Utc};
use std::num::NonZeroU32;

mod images;
// Every type here describes the import queue's own state, which the mobile
// builds leave out along with the import pipeline that produces it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod import;
mod query;

pub use images::*;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use import::*;
pub use query::*;

/// An individual artist or band. Linked to albums and tracks through the
/// `album_artists` / `track_artists` junction tables, so an album or track can
/// credit several artists (collaborations, features, compilations).
#[derive(Debug, Clone, PartialEq)]
pub struct DbArtist {
    pub id: String,
    pub name: String,
    /// Sort name for alphabetical ordering (e.g., "Artist Name, The")
    pub sort_name: Option<String>,
    /// Artist ID from Discogs (for deduplication across imports)
    pub discogs_artist_id: Option<String>,
    /// Artist ID from MusicBrainz (for deduplication across imports)
    pub musicbrainz_artist_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// Well-known "Various Artists" IDs per metadata source.
pub struct VariousArtistsIds {
    pub discogs: &'static str,
    pub musicbrainz: &'static str,
}

pub const VARIOUS_ARTISTS: VariousArtistsIds = VariousArtistsIds {
    discogs: "194",
    musicbrainz: "89ad4ac3-39f7-470e-963a-56509c546377",
};

impl DbArtist {
    /// True if this artist is the well-known "Various Artists" placeholder of any
    /// metadata source. The destructure is deliberate: adding a `*_artist_id`
    /// field to `DbArtist` without handling it here is a compile error.
    pub fn is_various_artists(&self) -> bool {
        let DbArtist {
            discogs_artist_id,
            musicbrainz_artist_id,
            id: _,
            name: _,
            sort_name: _,
            created_at: _,
        } = self;

        discogs_artist_id.as_deref() == Some(VARIOUS_ARTISTS.discogs)
            || musicbrainz_artist_id.as_deref() == Some(VARIOUS_ARTISTS.musicbrainz)
    }
}

/// One place in an album's credit list: the artist credited there, the other
/// printed name of the artist's it prints if any, and what follows it.
///
/// An import or edit builds these before its credits are resolved, `credit`
/// holding the name the credit prints; the write resolves the artist and
/// keeps `credit` only where it is not the artist's own name, adding it to
/// the artist's `artist_credits` if it is new there.
#[derive(Debug, Clone)]
pub struct DbAlbumArtist {
    pub album_id: String,
    pub artist_id: String,
    /// The credit's place in the list, from 0.
    pub position: u32,
    pub credit: Option<crate::credits::CreditName>,
    /// Printed after the credit when another follows; `None` prints ", ".
    pub join_phrase: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// One place in a track's own credit list, as [`DbAlbumArtist`] is for an
/// album's.
#[derive(Debug, Clone)]
pub struct DbTrackArtist {
    pub track_id: String,
    pub artist_id: String,
    /// The credit's place in the list, from 0.
    pub position: u32,
    pub credit: Option<crate::credits::CreditName>,
    pub join_phrase: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWork {
    /// Computed from `musicbrainz_work_id` (`db::identity::work_id`), so every
    /// device that meets one work writes one row.
    pub id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    /// The MusicBrainz work this row came from — the only source of works — and
    /// what an import dedups on.
    pub musicbrainz_work_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkArtist {
    pub work_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: Catalog,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbWorkPart {
    pub parent_work_id: String,
    pub child_work_id: String,
    pub position: i32,
    pub source: Catalog,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackWork {
    pub id: String,
    pub track_id: String,
    pub work_id: String,
    pub position: i32,
    pub source: Catalog,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseArtistRole {
    pub id: String,
    pub release_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: Catalog,
    pub source_credit: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DbTrackArtistRole {
    pub id: String,
    pub track_id: String,
    pub artist_id: String,
    pub position: i32,
    pub source: Catalog,
    pub source_credit: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// A logical album (the "master"): the metadata common to every physical release
/// of it ("1973 Original", "2016 Remaster", …). The releases themselves, and
/// their import status, live in the `releases` table.
///
/// Albums carry no records of their own — a record is a catalog's description
/// of one release, kept in a side table, and cross-catalog
/// equivalences surface implicitly when an album's releases hold rows in
/// several catalogs. Releases
/// attach to albums loosely (title/artist match), so album-level identity
/// columns would claim a certainty the attach rule doesn't have.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbum {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// True for "Various Artists" compilation albums
    pub is_compilation: bool,
    pub created_at: DateTime<Utc>,
}

/// Raw album-summary aggregate: artist names, release IDs and the primary
/// release joined in SQL to avoid N+1 lookups. The resolver in
/// `LibraryManager` produces the display-ready
/// `crate::album_detail::AlbumSummary`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAlbumSummary {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    pub artist_names: String,
    /// The album's releases in album order.
    pub release_ids: Vec<String>,
    /// The album's lowest-ordered release.
    pub primary_release_id: String,
}

/// Probe whether a search-result candidate is already in the library: as the
/// same pressing (a record on this `source` whose key is
/// `release_id`), or as the same album (any release with
/// this `source` + `source_group_id`). Fields are the source's own IDs, not
/// bae's; the `LibraryStatus` result is correlated back via `release_id`.
///
/// `source_group_id` is optional because a search result may not surface a group
/// (rare for MusicBrainz, happens for Discogs releases with no master) — those
/// candidates skip the album-level lookup.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LibraryCheck {
    pub release_id: String,
    pub source: Catalog,
    pub source_group_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryStatus {
    pub release_id: String,
    /// Every library album holding this release or another release of its
    /// album: those holding the release itself first, then by age.
    pub albums: Vec<LibraryAlbumMatch>,
}

/// One library album a release's records name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryAlbumMatch {
    pub album_id: String,
    pub album_title: String,
    /// It holds a copy of the release itself, not only another release of
    /// its album.
    pub holds_release: bool,
}

impl LibraryStatus {
    /// Nothing in the library matches this release — neither the release itself
    /// nor its album, so there is no album to name either.
    pub fn absent(release_id: impl Into<String>) -> Self {
        Self {
            release_id: release_id.into(),
            albums: Vec::new(),
        }
    }

    /// Whether some album holds a copy of the release itself.
    pub fn release_in_library(&self) -> bool {
        self.albums.iter().any(|album| album.holds_release)
    }

    /// Whether some album holds the release or another release of its album.
    pub fn album_in_library(&self) -> bool {
        !self.albums.is_empty()
    }
}

/// A specific physical or digital version of a logical album ("1973 Original
/// Pressing", "2016 Remaster", "180g Vinyl", …). Files and tracks hang off
/// releases rather than albums: users import a specific release, each has its own
/// audio and metadata, and several releases of one album coexist in the library.
#[derive(Debug, Clone, PartialEq)]
pub struct DbRelease {
    /// A fresh random id for this imported copy, independent of its folder hash.
    pub id: String,
    pub album_id: String,
    /// Human-readable release name (e.g., "2016 Remaster", "180g Vinyl"). `None`
    /// when the user never named a version.
    pub release_name: Option<String>,
    /// Pressing-level editorial metadata. `Pressing::blank()` means "user claimed
    /// an album, not a specific pressing" (Approximate imports).
    pub pressing: bae_types::pressing::Pressing,
    /// Name of the source folder this release was imported from (just the final
    /// path component, not the full path).
    pub source_folder_name: Option<String>,
    /// Album-level integrated loudness (EBU R128) in LUFS, measured at import
    /// over all tracks combined. `None` = not measured. Playback derives a gain
    /// from this against a constant target; the raw measurement is stored, never
    /// a gain.
    pub album_loudness_lufs: Option<f64>,
    /// Album-level true peak as a linear ratio (1.0 = 0 dBTP), the max across
    /// tracks. `None` = not measured. Playback caps the album gain at `1.0/peak`.
    pub album_peak_linear: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// The playing context of a saved `playback_state` row: the recipe to refill the
/// lane on restart — what it played from, and whether it was shuffled. A
/// substruct so "no context playing" (a single track, or nothing) is one `None`
/// instead of two columns that are only ever both-present or both-absent — see
/// `many-fields-none-together-means-a-missing-type`. The SQLite columns stay flat
/// (`source`, `shuffled`); the DB client splits this apart on save and
/// reassembles it on load.
#[derive(Debug, Clone, PartialEq)]
pub struct DbPlaybackContext {
    /// What the context plays from, encoded for the flat column: a release id, a
    /// JSON array of release ids, or the library sentinel. See
    /// `source_to_str`/`source_from_str` in `playback::persisted`.
    pub source: String,
    /// Whether the lane was shuffled. Restore permutes the refilled lane afresh
    /// — the session's shuffled order is deliberately not persisted.
    pub shuffled: bool,
}

/// The single device-local `playback_state` row. Mirrors the table columns; the
/// playback service maps this to and from its queue snapshot + live
/// position/volume/mute.
#[derive(Debug, Clone)]
pub struct DbPlaybackState {
    /// The playing context, or `None` for a single track / nothing playing.
    pub context: Option<DbPlaybackContext>,
    pub manual: String,
    pub repeat: String,
    pub current_track_id: Option<String>,
    pub position_ms: Option<i64>,
    pub volume: f32,
    pub is_muted: bool,
}

/// The outcome of reading the device-local `playback_state` resume cache. The
/// row is bae's own write, so a structurally-impossible row (a `source` present
/// without a `shuffled`, or vice versa) is corruption — kept distinct from an
/// absent row so the caller can count it and clear it rather than silently
/// starting fresh over a masked failure.
pub enum LoadedPlaybackState {
    Absent,
    Corrupt,
    Present(DbPlaybackState),
}

/// A single track on a specific release, not on the logical album — track
/// listings vary between releases. Track artists are linked through the
/// `track_artists` junction table, so a track can credit features and artists
/// other than the album's.
#[derive(Debug, Clone, PartialEq)]
pub struct DbTrack {
    pub id: String,
    pub release_id: String,
    pub title: String,
    /// The release medium the track sits on ([`DbReleaseMedium::id`]).
    pub medium_id: String,
    /// The side of that medium, counted from one: a CD's only side is 1, a
    /// record's B side 2.
    pub side: std::num::NonZeroU8,
    /// The track's number on its side.
    pub track_number: NonZeroU32,
    /// How long the track's audio plays, measured when it was imported.
    pub duration_ms: u64,
    pub created_at: DateTime<Utc>,
}

/// One medium a release holds, at its position among the release's mediums.
#[derive(Debug, Clone, PartialEq)]
pub struct DbReleaseMedium {
    pub id: String,
    pub release_id: String,
    pub position: std::num::NonZeroU16,
    pub medium: crate::mediums::ReleaseMedium,
    pub created_at: DateTime<Utc>,
}

impl DbReleaseMedium {
    /// The row for `medium` at `position` of `release_id`.
    pub fn new(
        release_id: &str,
        position: std::num::NonZeroU16,
        medium: crate::mediums::ReleaseMedium,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: crate::db::identity::release_medium_id(release_id, position.get()),
            release_id: release_id.to_string(),
            position,
            medium,
            created_at,
        }
    }
}

/// A release's mediums in order, which its tracks' places are read against.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DbReleaseMediums(pub Vec<DbReleaseMedium>);

impl DbReleaseMediums {
    /// The mediums alone, in order.
    pub fn mediums(&self) -> Vec<crate::mediums::ReleaseMedium> {
        self.0.iter().map(|row| row.medium).collect()
    }

    /// Where `track`, one of this release's, sits. Its medium is one of these:
    /// the schema refuses a track whose medium row is missing.
    pub fn place(&self, track: &DbTrack) -> crate::mediums::TrackPlace {
        let medium = self
            .0
            .iter()
            .find(|medium| medium.id == track.medium_id)
            .expect("a track's medium row is its release's");
        crate::mediums::TrackPlace {
            medium: medium.position,
            side: track.side,
            number: track.track_number,
        }
    }

    /// How `track` reads among these mediums.
    pub fn position(&self, track: &DbTrack) -> crate::album_detail::TrackPosition {
        crate::mediums::track_position(&self.place(track), &self.mediums())
    }

    /// The disc number a surface outside bae states for `track` (a saved
    /// file's disc tag, a Subsonic song's disc): its medium's position, for a
    /// release of several mediums, and none for a release of one.
    pub fn disc_number(&self, track: &DbTrack) -> Option<u32> {
        (self.0.len() > 1).then(|| u32::from(self.place(track).medium.get()))
    }

    /// The medium `track` sits on.
    pub fn medium_of(&self, track: &DbTrack) -> &DbReleaseMedium {
        self.0
            .iter()
            .find(|medium| medium.id == track.medium_id)
            .expect("a track's medium row is its release's")
    }
}
/// A physical file belonging to a release — not to an album, and not to a track
/// (some files are metadata like cover.jpg or .cue sheets that no track owns).
/// Holds enough of the original file's information to reconstruct the folder
/// structure on export.
///
/// A file storage holds is read through coven, from its cache or storage. Any
/// other file is read from this device's original, when it holds one.
#[derive(Debug, Clone, PartialEq)]
pub struct DbFile {
    pub id: String,
    pub release_id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub content_type: ContentType,
    /// The scan's complete facts for this physical source file. `None` for
    /// non-audio files.
    pub source_audio: Option<crate::album_detail::SourceAudioFile>,
    /// Whether the row names a file storage holds: its upload completed.
    pub stored: bool,
    pub created_at: DateTime<Utc>,
}

/// Track-level audio format metadata, one row per track: codec/display metadata,
/// pregap durations, and measured loudness. The file windows that supply the
/// track's samples live in `DbAudioSegment`.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAudioFormat {
    pub id: String,
    pub track_id: String,
    pub content_type: ContentType,
    /// The stored audio before INDEX 01 (a CUE track's INDEX 00), in source
    /// samples. When present, playback starts at INDEX 00 and shows negative
    /// time until INDEX 01; its length in time comes from the sample rate.
    pub pregap_samples: Option<u64>,
    /// Silence a CUE `PREGAP` directive puts before INDEX 01, in source
    /// samples. It is no bytes of the source file: natural playback emits this
    /// many zero frames before decoding the source audio; direct starts skip it.
    pub generated_pregap_samples: Option<u64>,
    /// Sample rate in Hz (for time-to-sample conversion during seek).
    pub sample_rate: NonZeroU32,
    /// Bits per sample, 1 to 32 (16, 24, …). `None` for lossy codecs, which
    /// have no fixed sample depth.
    pub bits_per_sample: Option<NonZeroU32>,
    pub channels: NonZeroU32,
    /// Per-track integrated loudness (EBU R128) in LUFS, measured at import over
    /// this track's sample window. `None` = not measured (decode/measure failure
    /// or a near-silent track with no usable loudness). Playback derives a gain
    /// from this against a constant target; the raw measurement is stored.
    pub track_loudness_lufs: Option<f64>,
    /// Per-track true peak as a linear ratio (1.0 = 0 dBTP), the max across
    /// channels. `None` = not measured. Playback caps the track gain at
    /// `1.0/peak`.
    pub track_peak_linear: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbAudioSegmentRole {
    AudioPregap,
    Main,
}

impl DbAudioSegmentRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AudioPregap => "audio_pregap",
            Self::Main => "main",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "audio_pregap" => Some(Self::AudioPregap),
            "main" => Some(Self::Main),
            _ => None,
        }
    }
}

/// Where a segment begins and ends inside its backing file, in samples and in
/// bytes — the one declaration of the window every layer above the row carries
/// (resolved, prepared-for-playback, decode params). `end_sample`/`end_byte`
/// `None` = to end of file; `start_byte` `None` = byte 0 / no recorded landing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmentSpan {
    /// First sample of the segment within its backing file.
    pub start_sample: u64,
    /// One past the segment's last sample within its backing file.
    pub end_sample: Option<u64>,
    /// Byte the segment begins at within its backing file (the demuxer's
    /// recorded seek landing).
    pub start_byte: Option<u64>,
    /// One past the segment's last byte within its backing file.
    pub end_byte: Option<u64>,
}

impl SegmentSpan {
    /// The whole backing file: start at sample 0, run to EOF.
    pub fn whole_file() -> Self {
        Self {
            start_sample: 0,
            end_sample: None,
            start_byte: None,
            end_byte: None,
        }
    }
}

/// One ordered file-backed window that supplies samples for an audio format.
/// The schema keeps every position non-negative, and the row reader refuses a
/// negative one, so the positions are unsigned here.
#[derive(Debug, Clone, PartialEq)]
pub struct DbAudioSegment {
    pub id: String,
    pub audio_format_id: String,
    pub segment_index: i64,
    pub role: DbAudioSegmentRole,
    pub file_id: String,
    /// First sample of this segment within its backing file.
    pub start_sample: u64,
    /// One past this segment's last sample within its backing file.
    pub end_sample: Option<u64>,
    /// Byte this segment begins at within its backing file. `None` means byte 0.
    pub start_byte: Option<u64>,
    /// One past this segment's last byte within its backing file.
    pub end_byte: Option<u64>,
    pub created_at: DateTime<Utc>,
}

impl DbAudioSegment {
    /// The row's window within its backing file.
    pub fn span(&self) -> SegmentSpan {
        SegmentSpan {
            start_sample: self.start_sample,
            end_sample: self.end_sample,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
        }
    }
}

/// A frame in a segment's file that a scrub can jump to by byte: the frame's
/// first sample and the byte the frame begins at, both within the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrubPoint {
    pub sample: u64,
    pub byte: u64,
}

/// One stored scrub point and the segment whose file it lies in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbScrubPoint {
    pub audio_segment_id: String,
    pub point: ScrubPoint,
}
impl DbAlbumArtist {
    /// The credit's row id: one place of one album's list is one row on
    /// every device.
    pub fn id(&self) -> String {
        crate::db::identity::album_artist_id(&self.album_id, self.position)
    }

    /// `artist_id` at `position`, printed by its own name, followed by the
    /// default separator.
    pub fn new(album_id: &str, artist_id: &str, position: u32, now: DateTime<Utc>) -> Self {
        DbAlbumArtist {
            album_id: album_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            credit: None,
            join_phrase: None,
            created_at: now,
        }
    }

    /// A test album's one credit: `artist_id` at position 0.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_test(album_id: &str, artist_id: &str) -> Self {
        Self::new(album_id, artist_id, 0, chrono::Utc::now())
    }
}
impl DbTrackArtist {
    /// The credit's row id: one place of one track's list is one row on
    /// every device.
    pub fn id(&self) -> String {
        crate::db::identity::track_artist_id(&self.track_id, self.position)
    }

    /// `artist_id` at `position`, printed by its own name, followed by the
    /// default separator.
    pub fn new(track_id: &str, artist_id: &str, position: u32, now: DateTime<Utc>) -> Self {
        DbTrackArtist {
            track_id: track_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            credit: None,
            join_phrase: None,
            created_at: now,
        }
    }
}

impl DbWorkArtist {
    /// The credit's row id, the same on every device that records it.
    pub fn id(&self) -> String {
        crate::db::identity::work_artist_id(&self.work_id, &self.artist_id, self.position)
    }

    pub fn new(
        work_id: &str,
        artist_id: &str,
        position: i32,
        source: Catalog,
        now: DateTime<Utc>,
    ) -> Self {
        DbWorkArtist {
            work_id: work_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            source,
            created_at: now,
        }
    }
}

impl DbWorkPart {
    /// The membership's row id, the same on every device that records it.
    pub fn id(&self) -> String {
        crate::db::identity::work_part_id(&self.parent_work_id, &self.child_work_id)
    }

    pub fn new(
        parent_work_id: &str,
        child_work_id: &str,
        position: i32,
        source: Catalog,
        now: DateTime<Utc>,
    ) -> Self {
        DbWorkPart {
            parent_work_id: parent_work_id.to_string(),
            child_work_id: child_work_id.to_string(),
            position,
            source,
            created_at: now,
        }
    }
}

impl DbTrackWork {
    pub fn new(
        track_id: &str,
        work_id: &str,
        position: i32,
        source: Catalog,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbTrackWork {
            id,
            track_id: track_id.to_string(),
            work_id: work_id.to_string(),
            position,
            source,
            created_at: now,
        }
    }
}

impl DbReleaseArtistRole {
    pub fn new(
        release_id: &str,
        artist_id: &str,
        position: i32,
        source: Catalog,
        source_credit: Option<String>,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbReleaseArtistRole {
            id,
            release_id: release_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            source,
            source_credit,
            created_at: now,
        }
    }
}

impl DbTrackArtistRole {
    pub fn new(
        track_id: &str,
        artist_id: &str,
        position: i32,
        source: Catalog,
        source_credit: Option<String>,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbTrackArtistRole {
            id,
            track_id: track_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            source,
            source_credit,
            created_at: now,
        }
    }
}
impl DbAlbum {
    #[cfg(test)]
    pub fn new_test(title: &str) -> Self {
        let now = chrono::Utc::now();
        DbAlbum {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.to_string(),
            year: None,
            is_compilation: false,
            created_at: now,
        }
    }
}

impl DbRelease {
    #[cfg(any(test, feature = "test-utils"))]
    /// A release fixture with no attached files.
    pub fn new_test(album_id: &str, release_id: &str) -> Self {
        let now = chrono::Utc::now();
        DbRelease {
            id: release_id.to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            pressing: bae_types::pressing::Pressing::blank(),
            source_folder_name: None,
            album_loudness_lufs: None,
            album_peak_linear: None,
            created_at: now,
        }
    }
}

impl DbTrack {
    /// Track `track_number` of the only side of `release_id`'s first medium
    /// ([`DbReleaseMedium::new_test`]).
    #[cfg(test)]
    pub fn new_test(release_id: &str, track_id: &str, title: &str, track_number: u32) -> Self {
        let now = chrono::Utc::now();
        DbTrack {
            id: track_id.to_string(),
            release_id: release_id.to_string(),
            title: title.to_string(),
            medium_id: crate::db::identity::release_medium_id(release_id, 1),
            side: std::num::NonZeroU8::MIN,
            track_number: NonZeroU32::new(track_number).expect("track numbers count from one"),
            duration_ms: 180_000,
            created_at: now,
        }
    }
}
impl DbReleaseMedium {
    /// The first medium of `release_id`: one CD side.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_test(release_id: &str) -> Self {
        Self::new(
            release_id,
            std::num::NonZeroU16::MIN,
            crate::mediums::ReleaseMedium::one_sided(Some(bae_types::pressing::Medium::Cd)),
            chrono::Utc::now(),
        )
    }
}

impl DbFile {
    /// Create a file record
    ///
    /// Files are linked to releases. Used for reconstructing original file structure
    /// during export. Coven owns the file content hash and fills its private
    /// declaration while registering the prepared external file.
    pub fn new(
        release_id: &str,
        original_filename: &str,
        file_size: i64,
        content_type: ContentType,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbFile {
            id,
            release_id: release_id.to_string(),
            original_filename: original_filename.to_string(),
            file_size,
            content_type,
            source_audio: None,
            stored: false,
            created_at: now,
        }
    }
}
impl DbAudioFormat {
    pub fn new(
        track_id: &str,
        content_type: ContentType,
        sample_rate: NonZeroU32,
        bits_per_sample: Option<NonZeroU32>,
        channels: NonZeroU32,
        id: String,
        now: DateTime<Utc>,
    ) -> Self {
        DbAudioFormat {
            id,
            track_id: track_id.to_string(),
            content_type,
            pregap_samples: None,
            generated_pregap_samples: None,
            sample_rate,
            bits_per_sample,
            channels,
            track_loudness_lufs: None,
            track_peak_linear: None,
            created_at: now,
        }
    }

    /// The pregap a CUE-backed track carries, in samples: the gap the sheet
    /// states and the one bae generates. A per-track file has neither.
    pub fn with_pregaps(
        mut self,
        pregap_samples: Option<u64>,
        generated_pregap_samples: Option<u64>,
    ) -> Self {
        self.pregap_samples = pregap_samples;
        self.generated_pregap_samples = generated_pregap_samples;
        self
    }
}
