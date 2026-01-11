//! Display types for UI components
//!
//! These types are lightweight versions of database models, containing
//! only the fields needed for display. They enable props-based components
//! that can work with either real or demo data.

/// Album display info
#[derive(Clone, Debug, PartialEq)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub cover_url: Option<String>,
    pub is_compilation: bool,
}

/// Artist display info
#[derive(Clone, Debug, PartialEq)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

/// Track import state for UI display
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum TrackImportState {
    /// Track import not started or not applicable
    #[default]
    None,
    /// Track is being imported with progress percentage
    Importing(u8),
    /// Track import completed
    Complete,
}

/// Track display info
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub track_number: Option<i32>,
    pub disc_number: Option<i32>,
    pub duration_ms: Option<i64>,
    pub is_available: bool,
    /// Import state for reactive UI updates during import
    pub import_state: TrackImportState,
}

/// Playback display state
#[derive(Clone, Debug, PartialEq, Default)]
pub enum PlaybackDisplay {
    #[default]
    Stopped,
    Loading {
        track_id: String,
    },
    Playing {
        track_id: String,
        position_ms: u64,
        duration_ms: u64,
    },
    Paused {
        track_id: String,
        position_ms: u64,
        duration_ms: u64,
    },
}

/// Queue item for display
#[derive(Clone, Debug, PartialEq)]
pub struct QueueItem {
    pub track: Track,
    pub album_title: String,
    pub cover_url: Option<String>,
}

/// Release display info
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Release {
    pub id: String,
    pub album_id: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    pub discogs_release_id: Option<String>,
    pub musicbrainz_release_id: Option<String>,
}

/// File display info
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub id: String,
    pub filename: String,
    pub file_size: i64,
    pub format: String,
}

/// Image display info
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub id: String,
    pub filename: String,
    pub is_cover: bool,
    pub source: String,
}

/// Import operation status for UI display
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ImportStatus {
    #[default]
    Preparing,
    Importing,
    Complete,
    Failed,
}

/// Active import for UI display
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveImport {
    pub import_id: String,
    pub album_title: String,
    pub artist_name: String,
    pub status: ImportStatus,
    /// Human-readable text for current step (e.g., "Parsing metadata...")
    pub current_step_text: Option<String>,
    pub progress_percent: Option<u8>,
    pub release_id: Option<String>,
    pub cover_url: Option<String>,
}

// ============================================================================
// Import Workflow Display Types
// ============================================================================

/// Import workflow phase for UI display
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ImportPhase {
    #[default]
    FolderSelection,
    ReleaseSelection,
    MetadataDetection,
    ExactLookup,
    ManualSearch,
    Confirmation,
}

/// Search tab for manual search panel
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum SearchTab {
    #[default]
    General,
    CatalogNumber,
    Barcode,
}

/// Search source (metadata provider)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum SearchSource {
    #[default]
    MusicBrainz,
    Discogs,
}

/// Match candidate source type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MatchSourceType {
    MusicBrainz,
    Discogs,
}

/// Match candidate for UI display
#[derive(Clone, Debug, PartialEq)]
pub struct MatchCandidate {
    pub title: String,
    pub artist: String,
    pub year: Option<String>,
    pub cover_url: Option<String>,
    pub format: Option<String>,
    pub country: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub source_type: MatchSourceType,
    /// Original year / first release date (for MusicBrainz)
    pub original_year: Option<String>,
}

/// Detected folder metadata for UI display
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FolderMetadata {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
    pub track_count: Option<u32>,
    pub discid: Option<String>,
    pub confidence: f32,
    /// Tokens extracted from folder name for search suggestions
    pub folder_tokens: Vec<String>,
}

/// File info for UI display (simplified)
#[derive(Clone, Debug, PartialEq)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub format: String,
}

/// Artwork file info with display URL
#[derive(Clone, Debug, PartialEq)]
pub struct ArtworkFile {
    pub name: String,
    /// URL to display the artwork (resolved by caller)
    pub display_url: String,
}

/// A CUE/FLAC pair for UI display
#[derive(Clone, Debug, PartialEq)]
pub struct CueFlacPairInfo {
    pub cue_name: String,
    pub flac_name: String,
    pub total_size: u64,
    pub track_count: usize,
}

/// Audio content type for UI display
#[derive(Clone, Debug, PartialEq)]
pub enum AudioContentInfo {
    /// One or more CUE/FLAC pairs
    CueFlacPairs(Vec<CueFlacPairInfo>),
    /// Individual track files
    TrackFiles(Vec<FileInfo>),
}

impl Default for AudioContentInfo {
    fn default() -> Self {
        AudioContentInfo::TrackFiles(Vec::new())
    }
}

/// Pre-categorized files for UI display
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CategorizedFileInfo {
    /// Audio content - CUE/FLAC pairs or track files
    pub audio: AudioContentInfo,
    /// Artwork/image files
    pub artwork: Vec<FileInfo>,
    /// Document files (.log, .txt, .nfo) - CUE files in pairs are NOT here
    pub documents: Vec<FileInfo>,
    /// Everything else
    pub other: Vec<FileInfo>,
}

impl CategorizedFileInfo {
    /// Total number of files across all categories
    pub fn total_count(&self) -> usize {
        let audio_count = match &self.audio {
            AudioContentInfo::CueFlacPairs(pairs) => pairs.len() * 2,
            AudioContentInfo::TrackFiles(tracks) => tracks.len(),
        };
        audio_count + self.artwork.len() + self.documents.len() + self.other.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.total_count() == 0
    }
}

/// Torrent file info for UI display
#[derive(Clone, Debug, PartialEq)]
pub struct TorrentFileInfo {
    pub path: String,
    pub size: i64,
}

/// Torrent info for UI display
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TorrentInfo {
    pub name: String,
    pub trackers: Vec<String>,
    pub comment: String,
    pub creator: String,
    pub creation_date: i64,
    pub is_private: bool,
    pub total_size: i64,
    pub piece_length: i32,
    pub num_pieces: i32,
    pub files: Vec<TorrentFileInfo>,
}

/// Storage profile for UI display
#[derive(Clone, Debug, PartialEq)]
pub struct StorageProfileInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Selected cover for import UI
#[derive(Clone, Debug, PartialEq)]
pub enum SelectedCover {
    /// Remote cover from MusicBrainz/Discogs
    Remote { url: String, source: String },
    /// Local artwork file from the album folder
    Local { filename: String },
}

/// Detected release (subfolder) for multi-release import
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedRelease {
    pub name: String,
    pub path: String,
}

/// CD drive info for selection UI
#[derive(Clone, Debug, PartialEq)]
pub struct CdDriveInfo {
    pub device_path: String,
    pub name: String,
}
