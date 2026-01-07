use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use uuid::Uuid;
const IMPORT_STATUS_QUEUED: &str = "queued";
const IMPORT_STATUS_IMPORTING: &str = "importing";
const IMPORT_STATUS_COMPLETE: &str = "complete";
const IMPORT_STATUS_FAILED: &str = "failed";
/// Database models for bae storage system
///
/// This implements the storage strategy described in the README:
/// - Albums and tracks stored as metadata
/// - Files stored encrypted in cloud or local storage
///
/// Import status for albums and tracks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum ImportStatus {
    Queued,
    Importing,
    Complete,
    Failed,
}
impl ImportStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportStatus::Queued => IMPORT_STATUS_QUEUED,
            ImportStatus::Importing => IMPORT_STATUS_IMPORTING,
            ImportStatus::Complete => IMPORT_STATUS_COMPLETE,
            ImportStatus::Failed => IMPORT_STATUS_FAILED,
        }
    }
}
/// Artist metadata
///
/// Represents an individual artist or band. Artists are linked to albums and tracks
/// via junction tables (album_artists, track_artists) to support:
/// - Multiple artists per album (collaborations)
/// - Different artists per track (compilations, features)
/// - Artist deduplication across imports
///
/// Supports multiple metadata sources:
/// - Discogs: discogs_artist_id for deduplication
/// - Bandcamp: bandcamp_artist_id for future integration
/// - Other sources can be added as needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbArtist {
    pub id: String,
    pub name: String,
    /// Sort name for alphabetical ordering (e.g., "Beatles, The")
    pub sort_name: Option<String>,
    /// Artist ID from Discogs (for deduplication across imports)
    pub discogs_artist_id: Option<String>,
    /// Artist ID from Bandcamp (for future multi-source support)
    pub bandcamp_artist_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
/// Links artists to albums (many-to-many)
///
/// Supports albums with multiple artists (e.g., collaborations).
/// Position field maintains the order of artists for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAlbumArtist {
    pub id: String,
    pub album_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist albums (0-indexed)
    pub position: i32,
}
/// Links artists to tracks (many-to-many)
///
/// Supports tracks with multiple artists (features, remixes, etc.).
/// Role field distinguishes between main artist, featured artist, remixer, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTrackArtist {
    pub id: String,
    pub track_id: String,
    pub artist_id: String,
    /// Order of this artist in multi-artist tracks (0-indexed)
    pub position: i32,
    /// Role: "main", "featuring", "remixer", etc.
    pub role: Option<String>,
}
/// Discogs master release information for an album
///
/// When an album is imported from Discogs, both the master_id and release_id
/// are always known together (the release_id is the main_release for that master).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscogsMasterRelease {
    pub master_id: String,
    pub release_id: String,
}
/// MusicBrainz release information for an album
///
/// MusicBrainz has Release Groups (abstract albums) and Releases (specific versions).
/// Similar to Discogs master_id/release_id relationship.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MusicBrainzRelease {
    pub release_group_id: String,
    pub release_id: String,
}
/// Album metadata - represents a logical album (the "master")
///
/// A logical album can have multiple physical releases (e.g., "1973 Original", "2016 Remaster").
/// This table stores the high-level album information that's common across all releases.
/// Specific release details and import status are tracked in the `releases` table.
///
/// Artists are linked via the `album_artists` junction table to support multiple artists.
///
/// Supports multiple metadata sources:
/// - Discogs: discogs_release links to the Discogs master release and its main release
/// - Bandcamp: bandcamp_album_id would link to the Bandcamp album
/// - Other sources can be added as needed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbAlbum {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// Discogs release information
    pub discogs_release: Option<DiscogsMasterRelease>,
    /// MusicBrainz release information
    pub musicbrainz_release: Option<MusicBrainzRelease>,
    /// Album ID from Bandcamp (optional, for future multi-source support)
    pub bandcamp_album_id: Option<String>,
    /// Reference to the cover image (DbImage.id) - set after import
    pub cover_image_id: Option<String>,
    /// Cover art URL for immediate display (remote URL or bae://local/... for local files)
    /// Used before import completes and cover_image_id is set
    pub cover_art_url: Option<String>,
    /// True for "Various Artists" compilation albums
    pub is_compilation: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
/// Release metadata - represents a specific version/pressing of an album
///
/// A release is a physical or digital version of a logical album.
/// Examples: "1973 Original Pressing", "2016 Remaster", "180g Vinyl", "Digital Release"
///
/// Files and tracks belong to releases (not albums), because:
/// - Users import specific releases, not abstract albums
/// - Each release has its own audio files and metadata
/// - Multiple releases of the same album can coexist in the library
///
/// The release_name field distinguishes between versions (e.g., "2016 Remaster").
/// If the user doesn't specify a release, we create one with release_name=None.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbRelease {
    pub id: String,
    /// Links to the logical album (DbAlbum)
    pub album_id: String,
    /// Human-readable release name (e.g., "2016 Remaster", "180g Vinyl")
    pub release_name: Option<String>,
    /// Release-specific year (may differ from album year)
    pub year: Option<i32>,
    /// Discogs release ID (optional)
    pub discogs_release_id: Option<String>,
    /// Bandcamp release ID (optional, for future multi-source support)
    pub bandcamp_release_id: Option<String>,
    /// Format (e.g., "CD", "Vinyl", "Digital")
    pub format: Option<String>,
    /// Record label
    pub label: Option<String>,
    /// Catalog number
    pub catalog_number: Option<String>,
    /// Country of release
    pub country: Option<String>,
    /// Barcode
    pub barcode: Option<String>,
    pub import_status: ImportStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
/// Track metadata within a release
///
/// Represents a single track on a specific release. Tracks are linked to releases
/// (not logical albums) because track listings can vary between releases.
///
/// Track artists are linked via the `track_artists` junction table to support:
/// - Multiple artists per track (features, collaborations)
/// - Different artists than the album artist (compilations)
///
/// The discogs_position field stores the track position from metadata
/// (e.g., "A1", "1", "1-1" for vinyl sides).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DbTrack {
    pub id: String,
    /// Links to the specific release (DbRelease), not the logical album
    pub release_id: String,
    pub title: String,
    /// Disc number (1-indexed) for multi-disc releases
    pub disc_number: Option<i32>,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Position from metadata source (e.g., "A1", "1", "1-1")
    pub discogs_position: Option<String>,
    pub import_status: ImportStatus,
    pub created_at: DateTime<Utc>,
}
/// Physical file belonging to a release
///
/// Stores original file information needed to reconstruct file structure for export
/// or BitTorrent seeding.
///
/// Files are linked to releases (not logical albums or tracks), because:
/// - Files are part of a specific release (e.g., "2016 Remaster" has different files than "1973 Original")
/// - Some files are metadata (cover.jpg, .cue sheets) not associated with any track
///
/// When a release has no storage profile, the `source_path` field stores the actual
/// file location for direct playback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbFile {
    pub id: String,
    /// Release this file belongs to
    pub release_id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub format: String,
    /// Absolute path to the source file on disk.
    /// Set when release has no storage profile (bae doesn't manage storage).
    /// For local imports: user's original file path.
    /// For torrent imports: temp folder path (ephemeral).
    pub source_path: Option<String>,
    pub created_at: DateTime<Utc>,
}
/// Audio format metadata for a track
///
/// Stores format information needed for playback. One record per track (1:1 with track).
///
/// **FLAC headers** are stored for CUE/FLAC tracks where we need to prepend them during playback
/// (track audio starts mid-file, decoder needs headers). Also stored for regular FLAC for seeking.
///
/// **Seektables** enable frame-accurate seeking. Built during import by scanning every FLAC frame
/// (~93ms precision, vs ~10s for embedded seektables). Both byte and sample offsets are track-relative
/// (byte 0, sample 0 = first frame of this track's audio data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbAudioFormat {
    pub id: String,
    pub track_id: String,
    pub format: String,
    pub flac_headers: Option<Vec<u8>>,
    pub needs_headers: bool,
    /// Start byte offset within the source file (for CUE/FLAC tracks).
    /// Calculated at import time using a dense seektable for frame-accurate positioning.
    pub start_byte_offset: Option<i64>,
    /// End byte offset within the source file (for CUE/FLAC tracks)
    pub end_byte_offset: Option<i64>,
    /// Pre-gap duration in milliseconds (for CUE/FLAC tracks with INDEX 00)
    /// When present, playback starts at INDEX 00 and shows negative time until INDEX 01
    pub pregap_ms: Option<i64>,
    /// Offset in samples from the start of extracted bytes to actual track content.
    /// Due to FLAC frame alignment, extracted bytes start at a frame boundary which may
    /// be up to ~4096 samples before the track's actual start. This offset tells the
    /// decoder how many samples to skip when playing.
    /// Stored as samples (not ms) to avoid rounding errors.
    pub frame_offset_samples: Option<i64>,
    /// Exact number of samples this track should contain (for gapless playback).
    /// After decoding, trim output to this count. Required because FLAC frames
    /// at track boundaries may extend past the logical track end.
    pub exact_sample_count: Option<i64>,
    /// Sample rate in Hz (for time-to-sample conversion during seek)
    pub sample_rate: i64,
    /// Dense seektable for frame-accurate seeking.
    /// JSON array of {sample_number, byte_offset} entries built by scanning FLAC frames.
    /// Enables seeking: prepend headers + data from frame boundary.
    pub seektable_json: String,
    /// Byte offset where audio data starts in the file (after headers).
    /// Seektable byte offsets are relative to this position.
    pub audio_data_start: i64,
    /// FK to DbFile containing this track's audio data.
    /// Links to files.id to get the actual source_path.
    pub file_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
impl DbArtist {
    /// Create an artist from Discogs artist data
    pub fn from_discogs_artist(discogs_artist_id: &str, name: &str) -> Self {
        let now = Utc::now();
        DbArtist {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            sort_name: None,
            discogs_artist_id: Some(discogs_artist_id.to_string()),
            bandcamp_artist_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}
impl DbAlbumArtist {
    pub fn new(album_id: &str, artist_id: &str, position: i32) -> Self {
        DbAlbumArtist {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
        }
    }
}
impl DbTrackArtist {
    pub fn new(track_id: &str, artist_id: &str, position: i32, role: Option<String>) -> Self {
        DbTrackArtist {
            id: Uuid::new_v4().to_string(),
            track_id: track_id.to_string(),
            artist_id: artist_id.to_string(),
            position,
            role,
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
            discogs_release: None,
            musicbrainz_release: None,
            bandcamp_album_id: None,
            cover_image_id: None,
            cover_art_url: None,
            is_compilation: false,
            created_at: now,
            updated_at: now,
        }
    }
    /// Create a logical album from a Discogs release
    /// Note: Artists should be created separately and linked via DbAlbumArtist
    ///
    /// master_id and master_year are always provided for releases imported from Discogs.
    /// The master year is used for the album year (not the release year).
    /// cover_art_url is for immediate display before import completes.
    pub fn from_discogs_release(
        release: &crate::discogs::DiscogsRelease,
        master_year: u32,
        cover_art_url: Option<String>,
    ) -> Self {
        let now = Utc::now();
        let discogs_release = DiscogsMasterRelease {
            master_id: release.master_id.clone(),
            release_id: release.id.clone(),
        };
        DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: release.title.clone(),
            year: Some(master_year as i32),
            discogs_release: Some(discogs_release),
            musicbrainz_release: None,
            bandcamp_album_id: None,
            cover_image_id: None,
            cover_art_url,
            is_compilation: false,
            created_at: now,
            updated_at: now,
        }
    }
    /// cover_art_url is for immediate display before import completes.
    pub fn from_mb_release(
        release: &crate::musicbrainz::MbRelease,
        master_year: u32,
        cover_art_url: Option<String>,
    ) -> Self {
        let now = Utc::now();
        let musicbrainz_release = crate::db::MusicBrainzRelease {
            release_group_id: release.release_group_id.clone(),
            release_id: release.release_id.clone(),
        };
        let year = release
            .first_release_date
            .as_ref()
            .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()))
            .or(Some(master_year as i32));
        DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: release.title.clone(),
            year,
            discogs_release: None,
            musicbrainz_release: Some(musicbrainz_release),
            bandcamp_album_id: None,
            cover_image_id: None,
            cover_art_url,
            is_compilation: false,
            created_at: now,
            updated_at: now,
        }
    }
}
impl DbRelease {
    #[cfg(test)]
    pub fn new_test(album_id: &str, release_id: &str) -> Self {
        let now = chrono::Utc::now();
        DbRelease {
            id: release_id.to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            year: None,
            discogs_release_id: None,
            bandcamp_release_id: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
            import_status: ImportStatus::Queued,
            created_at: now,
            updated_at: now,
        }
    }
    /// Create a release from a Discogs release
    pub fn from_discogs_release(album_id: &str, release: &crate::discogs::DiscogsRelease) -> Self {
        let now = Utc::now();
        DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            year: release.year.map(|y| y as i32),
            discogs_release_id: Some(release.id.clone()),
            bandcamp_release_id: None,
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: None,
            import_status: ImportStatus::Queued,
            created_at: now,
            updated_at: now,
        }
    }
    pub fn from_mb_release(album_id: &str, release: &crate::musicbrainz::MbRelease) -> Self {
        let now = Utc::now();
        let year = release
            .date
            .as_ref()
            .and_then(|d| d.split('-').next().and_then(|y| y.parse::<i32>().ok()));
        DbRelease {
            id: Uuid::new_v4().to_string(),
            album_id: album_id.to_string(),
            release_name: None,
            year,
            discogs_release_id: None,
            bandcamp_release_id: None,
            format: release.format.clone(),
            label: release.label.clone(),
            catalog_number: release.catalog_number.clone(),
            country: release.country.clone(),
            barcode: release.barcode.clone(),
            import_status: ImportStatus::Queued,
            created_at: now,
            updated_at: now,
        }
    }
}
impl DbTrack {
    #[cfg(test)]
    pub fn new_test(
        release_id: &str,
        track_id: &str,
        title: &str,
        track_number: Option<i32>,
    ) -> Self {
        DbTrack {
            id: track_id.to_string(),
            release_id: release_id.to_string(),
            title: title.to_string(),
            disc_number: None,
            track_number,
            duration_ms: None,
            discogs_position: None,
            import_status: ImportStatus::Queued,
            created_at: chrono::Utc::now(),
        }
    }
    pub fn from_discogs_track(
        discogs_track: &crate::discogs::DiscogsTrack,
        release_id: &str,
        track_index: usize,
        disc_number: Option<i32>,
    ) -> Result<Self, String> {
        Ok(DbTrack {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            title: discogs_track.title.clone(),
            disc_number,
            track_number: Some((track_index + 1) as i32),
            duration_ms: None,
            discogs_position: Some(discogs_track.position.clone()),
            import_status: ImportStatus::Queued,
            created_at: Utc::now(),
        })
    }
}
impl DbFile {
    /// Create a file record for export/torrent metadata
    ///
    /// Files are linked to releases. Used for reconstructing original file structure
    /// during export or BitTorrent seeding.
    pub fn new(release_id: &str, original_filename: &str, file_size: i64, format: &str) -> Self {
        DbFile {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            original_filename: original_filename.to_string(),
            file_size,
            format: format.to_string(),
            source_path: None,
            created_at: Utc::now(),
        }
    }
    /// Set the source path for None storage mode.
    /// This is the actual file location on disk for direct playback.
    pub fn with_source_path(mut self, path: &str) -> Self {
        self.source_path = Some(path.to_string());
        self
    }
}
impl DbAudioFormat {
    pub fn new(
        track_id: &str,
        format: &str,
        flac_headers: Option<Vec<u8>>,
        needs_headers: bool,
        sample_rate: i64,
        seektable_json: String,
        audio_data_start: i64,
    ) -> Self {
        Self::new_full(
            track_id,
            format,
            flac_headers,
            needs_headers,
            None,
            None,
            None,
            None,
            None,
            sample_rate,
            seektable_json,
            audio_data_start,
            None,
        )
    }

    pub fn new_with_byte_offsets(
        track_id: &str,
        format: &str,
        flac_headers: Option<Vec<u8>>,
        needs_headers: bool,
        start_byte_offset: i64,
        end_byte_offset: i64,
        pregap_ms: Option<i64>,
        frame_offset_samples: Option<i64>,
        exact_sample_count: Option<i64>,
        sample_rate: i64,
        seektable_json: String,
        audio_data_start: i64,
    ) -> Self {
        Self::new_full(
            track_id,
            format,
            flac_headers,
            needs_headers,
            Some(start_byte_offset),
            Some(end_byte_offset),
            pregap_ms,
            frame_offset_samples,
            exact_sample_count,
            sample_rate,
            seektable_json,
            audio_data_start,
            None,
        )
    }

    /// Set the file_id linking to DbFile
    pub fn with_file_id(mut self, file_id: &str) -> Self {
        self.file_id = Some(file_id.to_string());
        self
    }

    fn new_full(
        track_id: &str,
        format: &str,
        flac_headers: Option<Vec<u8>>,
        needs_headers: bool,
        start_byte_offset: Option<i64>,
        end_byte_offset: Option<i64>,
        pregap_ms: Option<i64>,
        frame_offset_samples: Option<i64>,
        exact_sample_count: Option<i64>,
        sample_rate: i64,
        seektable_json: String,
        audio_data_start: i64,
        file_id: Option<String>,
    ) -> Self {
        DbAudioFormat {
            id: Uuid::new_v4().to_string(),
            track_id: track_id.to_string(),
            format: format.to_string(),
            flac_headers,
            needs_headers,
            start_byte_offset,
            end_byte_offset,
            pregap_ms,
            frame_offset_samples,
            exact_sample_count,
            sample_rate,
            seektable_json,
            audio_data_start,
            file_id,
            created_at: Utc::now(),
        }
    }
}
/// Torrent import metadata for a release
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTorrent {
    pub id: String,
    pub release_id: String,
    pub info_hash: String,
    pub magnet_link: Option<String>,
    pub torrent_name: String,
    pub total_size_bytes: i64,
    pub piece_length: i32,
    pub num_pieces: i32,
    pub is_seeding: bool,
    pub created_at: DateTime<Utc>,
}
/// Maps torrent pieces to bae chunks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbTorrentPieceMapping {
    pub id: String,
    pub torrent_id: String,
    pub piece_index: i32,
    pub chunk_ids: String,
    pub start_byte_in_first_chunk: i64,
    pub end_byte_in_last_chunk: i64,
}
impl DbTorrent {
    pub fn new(
        release_id: &str,
        info_hash: &str,
        magnet_link: Option<String>,
        torrent_name: &str,
        total_size_bytes: i64,
        piece_length: i32,
        num_pieces: i32,
    ) -> Self {
        DbTorrent {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            info_hash: info_hash.to_string(),
            magnet_link,
            torrent_name: torrent_name.to_string(),
            total_size_bytes,
            piece_length,
            num_pieces,
            is_seeding: false,
            created_at: Utc::now(),
        }
    }
}
impl DbTorrentPieceMapping {
    pub fn new(
        torrent_id: &str,
        piece_index: i32,
        chunk_ids: Vec<String>,
        start_byte_in_first_chunk: i64,
        end_byte_in_last_chunk: i64,
    ) -> Result<Self, serde_json::Error> {
        Ok(DbTorrentPieceMapping {
            id: Uuid::new_v4().to_string(),
            torrent_id: torrent_id.to_string(),
            piece_index,
            chunk_ids: serde_json::to_string(&chunk_ids)?,
            start_byte_in_first_chunk,
            end_byte_in_last_chunk,
        })
    }
    pub fn chunk_ids(&self) -> Result<Vec<String>, serde_json::Error> {
        serde_json::from_str(&self.chunk_ids)
    }
}
const IMPORT_OP_STATUS_PREPARING: &str = "preparing";
const IMPORT_OP_STATUS_IMPORTING: &str = "importing";
const IMPORT_OP_STATUS_COMPLETE: &str = "complete";
const IMPORT_OP_STATUS_FAILED: &str = "failed";
/// Status of an import operation (distinct from release/track ImportStatus)
///
/// Tracks the lifecycle of an import from button click through completion:
/// - Preparing: Phase 0 work in ImportHandle (parsing, validation, DB setup)
/// - Importing: Phase 1 work in ImportService (file processing, encryption, upload)
/// - Complete: Successfully finished
/// - Failed: Error occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum ImportOperationStatus {
    Preparing,
    Importing,
    Complete,
    Failed,
}
impl ImportOperationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImportOperationStatus::Preparing => IMPORT_OP_STATUS_PREPARING,
            ImportOperationStatus::Importing => IMPORT_OP_STATUS_IMPORTING,
            ImportOperationStatus::Complete => IMPORT_OP_STATUS_COMPLETE,
            ImportOperationStatus::Failed => IMPORT_OP_STATUS_FAILED,
        }
    }
}
/// Tracks an import operation from button click through completion
///
/// Created when user clicks Import, before any database records exist.
/// Provides a stable ID for progress subscriptions during phase 0.
/// Linked to release_id after phase 0 completes and release is created.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbImport {
    pub id: String,
    pub status: ImportOperationStatus,
    /// Linked after phase 0 when release is created
    pub release_id: Option<String>,
    /// Album title for display before release exists
    pub album_title: String,
    /// Artist name for display
    pub artist_name: String,
    /// Source folder path
    pub folder_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Error message if status is Failed
    pub error_message: Option<String>,
}
impl DbImport {
    pub fn new(id: &str, album_title: &str, artist_name: &str, folder_path: &str) -> Self {
        let now = Utc::now().timestamp();
        DbImport {
            id: id.to_string(),
            status: ImportOperationStatus::Preparing,
            release_id: None,
            album_title: album_title.to_string(),
            artist_name: artist_name.to_string(),
            folder_path: folder_path.to_string(),
            created_at: now,
            updated_at: now,
            error_message: None,
        }
    }
}
/// Source of an image file
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum ImageSource {
    /// Image came with the release files (scans, artwork folder, etc.)
    Local,
    /// Fetched from MusicBrainz Cover Art Archive
    MusicBrainz,
    /// Fetched from Discogs
    Discogs,
}
impl ImageSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImageSource::Local => "local",
            ImageSource::MusicBrainz => "musicbrainz",
            ImageSource::Discogs => "discogs",
        }
    }
}
/// Image metadata for a release
///
/// Tracks all images associated with a release, including:
/// - Local images that came with the release (scans, artwork, etc.)
/// - Fetched images from MusicBrainz or Discogs stored in .bae/ folder
///
/// One image per release is designated as the cover (is_cover = true).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbImage {
    pub id: String,
    /// Release this image belongs to
    pub release_id: String,
    /// Relative path from release root (e.g., "cover.jpg", ".bae/front-mb.jpg", "Artwork/front.jpg")
    pub filename: String,
    /// True if this is the designated cover image for the release
    pub is_cover: bool,
    /// Where this image came from
    pub source: ImageSource,
    /// Image width in pixels (if known)
    pub width: Option<i32>,
    /// Image height in pixels (if known)
    pub height: Option<i32>,
    pub created_at: DateTime<Utc>,
}
impl DbImage {
    pub fn new(release_id: &str, filename: &str, is_cover: bool, source: ImageSource) -> Self {
        DbImage {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            filename: filename.to_string(),
            is_cover,
            source,
            width: None,
            height: None,
            created_at: Utc::now(),
        }
    }
    pub fn with_dimensions(mut self, width: i32, height: i32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
}
/// Where release data is stored
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum StorageLocation {
    /// Local filesystem path (bae manages storage)
    Local,
    /// Cloud storage (S3/MinIO, bae manages storage)
    Cloud,
}
impl StorageLocation {
    pub fn as_str(&self) -> &'static str {
        match self {
            StorageLocation::Local => "local",
            StorageLocation::Cloud => "cloud",
        }
    }
}
/// Reusable storage configuration template
///
/// Defines how releases should be stored. Users create profiles like
/// "Local Raw", "Cloud Encrypted", etc. and select them during import.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DbStorageProfile {
    pub id: String,
    pub name: String,
    /// Where to store: local filesystem or cloud
    pub location: StorageLocation,
    /// Path for local storage (ignored for cloud)
    pub location_path: String,
    /// Whether to encrypt data
    pub encrypted: bool,
    /// True if this is the default profile for new imports
    pub is_default: bool,
    /// S3 bucket name
    pub cloud_bucket: Option<String>,
    /// AWS region (e.g., "us-east-1")
    pub cloud_region: Option<String>,
    /// Custom endpoint URL for S3-compatible services (MinIO, etc.)
    pub cloud_endpoint: Option<String>,
    /// Access key ID
    pub cloud_access_key: Option<String>,
    /// Secret access key
    pub cloud_secret_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl DbStorageProfile {
    /// Create a new local storage profile
    pub fn new_local(name: &str, path: &str, encrypted: bool) -> Self {
        let now = Utc::now();
        DbStorageProfile {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            location: StorageLocation::Local,
            location_path: path.to_string(),
            encrypted,
            is_default: false,
            cloud_bucket: None,
            cloud_region: None,
            cloud_endpoint: None,
            cloud_access_key: None,
            cloud_secret_key: None,
            created_at: now,
            updated_at: now,
        }
    }
    /// Create a new cloud storage profile
    pub fn new_cloud(
        name: &str,
        bucket: &str,
        region: &str,
        endpoint: Option<&str>,
        access_key: &str,
        secret_key: &str,
        encrypted: bool,
    ) -> Self {
        let now = Utc::now();
        DbStorageProfile {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            location: StorageLocation::Cloud,
            location_path: String::new(),
            encrypted,
            is_default: false,
            cloud_bucket: Some(bucket.to_string()),
            cloud_region: Some(region.to_string()),
            cloud_endpoint: endpoint.map(|s| s.to_string()),
            cloud_access_key: Some(access_key.to_string()),
            cloud_secret_key: Some(secret_key.to_string()),
            created_at: now,
            updated_at: now,
        }
    }
    pub fn with_default(mut self, is_default: bool) -> Self {
        self.is_default = is_default;
        self
    }

    /// Convert cloud storage fields to S3Config for creating a client.
    /// Returns None if this is not a cloud profile or credentials are missing.
    pub fn to_s3_config(&self) -> Option<crate::cloud_storage::S3Config> {
        if self.location != StorageLocation::Cloud {
            return None;
        }
        Some(crate::cloud_storage::S3Config {
            bucket_name: self.cloud_bucket.clone()?,
            region: self.cloud_region.clone()?,
            access_key_id: self.cloud_access_key.clone()?,
            secret_access_key: self.cloud_secret_key.clone()?,
            endpoint_url: self.cloud_endpoint.clone(),
        })
    }
}
/// Links a release to its storage profile
///
/// Each release has exactly one storage configuration that determines
/// where and how its data is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbReleaseStorage {
    pub id: String,
    pub release_id: String,
    pub storage_profile_id: String,
    pub created_at: DateTime<Utc>,
}
impl DbReleaseStorage {
    pub fn new(release_id: &str, storage_profile_id: &str) -> Self {
        DbReleaseStorage {
            id: Uuid::new_v4().to_string(),
            release_id: release_id.to_string(),
            storage_profile_id: storage_profile_id.to_string(),
            created_at: Utc::now(),
        }
    }
}
