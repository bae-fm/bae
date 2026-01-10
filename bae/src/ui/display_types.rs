//! Display types for UI components
//!
//! These types are lightweight versions of the database models, containing
//! only the fields needed for display. They enable props-based components
//! that can work with either real or demo data.

use crate::db::{DbAlbum, DbArtist, DbTrack, ImportStatus};
use crate::playback::PlaybackState;
use crate::ui::image_url;

/// Album display info
#[derive(Clone, Debug, PartialEq)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub cover_url: Option<String>,
    pub is_compilation: bool,
}

impl From<DbAlbum> for Album {
    fn from(db: DbAlbum) -> Self {
        let cover_url = db
            .cover_image_id
            .as_ref()
            .map(|id| image_url(id))
            .or(db.cover_art_url);

        Album {
            id: db.id,
            title: db.title,
            year: db.year,
            cover_url,
            is_compilation: db.is_compilation,
        }
    }
}

impl From<&DbAlbum> for Album {
    fn from(db: &DbAlbum) -> Self {
        let cover_url = db
            .cover_image_id
            .as_ref()
            .map(|id| image_url(id))
            .or_else(|| db.cover_art_url.clone());

        Album {
            id: db.id.clone(),
            title: db.title.clone(),
            year: db.year,
            cover_url,
            is_compilation: db.is_compilation,
        }
    }
}

/// Artist display info
#[derive(Clone, Debug, PartialEq)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

impl From<DbArtist> for Artist {
    fn from(db: DbArtist) -> Self {
        Artist {
            id: db.id,
            name: db.name,
        }
    }
}

impl From<&DbArtist> for Artist {
    fn from(db: &DbArtist) -> Self {
        Artist {
            id: db.id.clone(),
            name: db.name.clone(),
        }
    }
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

impl From<DbTrack> for Track {
    fn from(db: DbTrack) -> Self {
        let is_available = db.import_status == ImportStatus::Complete;
        Track {
            id: db.id,
            title: db.title,
            track_number: db.track_number,
            disc_number: db.disc_number,
            duration_ms: db.duration_ms,
            is_available,
            import_state: if is_available {
                TrackImportState::Complete
            } else {
                TrackImportState::None
            },
        }
    }
}

impl From<&DbTrack> for Track {
    fn from(db: &DbTrack) -> Self {
        let is_available = db.import_status == ImportStatus::Complete;
        Track {
            id: db.id.clone(),
            title: db.title.clone(),
            track_number: db.track_number,
            disc_number: db.disc_number,
            duration_ms: db.duration_ms,
            is_available,
            import_state: if is_available {
                TrackImportState::Complete
            } else {
                TrackImportState::None
            },
        }
    }
}

/// Playback display state (simplified from PlaybackState)
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

impl From<&PlaybackState> for PlaybackDisplay {
    fn from(state: &PlaybackState) -> Self {
        match state {
            PlaybackState::Stopped => PlaybackDisplay::Stopped,
            PlaybackState::Loading { track_id } => PlaybackDisplay::Loading {
                track_id: track_id.clone(),
            },
            PlaybackState::Playing {
                track,
                position,
                duration,
                ..
            } => PlaybackDisplay::Playing {
                track_id: track.id.clone(),
                position_ms: position.as_millis() as u64,
                duration_ms: duration.map(|d| d.as_millis() as u64).unwrap_or(0),
            },
            PlaybackState::Paused {
                track,
                position,
                duration,
                ..
            } => PlaybackDisplay::Paused {
                track_id: track.id.clone(),
                position_ms: position.as_millis() as u64,
                duration_ms: duration.map(|d| d.as_millis() as u64).unwrap_or(0),
            },
        }
    }
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
    // MusicBrainz release ID (from album level, since releases don't have it)
    pub musicbrainz_release_id: Option<String>,
}

impl From<&crate::db::DbRelease> for Release {
    fn from(db: &crate::db::DbRelease) -> Self {
        Release {
            id: db.id.clone(),
            album_id: db.album_id.clone(),
            release_name: db.release_name.clone(),
            year: db.year,
            format: db.format.clone(),
            label: db.label.clone(),
            catalog_number: db.catalog_number.clone(),
            country: db.country.clone(),
            barcode: db.barcode.clone(),
            discogs_release_id: db.discogs_release_id.clone(),
            musicbrainz_release_id: None, // Set by caller if available
        }
    }
}

/// File display info
#[derive(Clone, Debug, PartialEq)]
pub struct File {
    pub id: String,
    pub filename: String,
    pub file_size: i64,
    pub format: String,
}

impl From<&crate::db::DbFile> for File {
    fn from(db: &crate::db::DbFile) -> Self {
        File {
            id: db.id.clone(),
            filename: db.original_filename.clone(),
            file_size: db.file_size,
            format: db.format.clone(),
        }
    }
}

/// Image display info
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub id: String,
    pub filename: String,
    pub is_cover: bool,
    pub source: String,
}

impl From<&crate::db::DbImage> for Image {
    fn from(db: &crate::db::DbImage) -> Self {
        Image {
            id: db.id.clone(),
            filename: db.filename.clone(),
            is_cover: db.is_cover,
            source: match db.source {
                crate::db::ImageSource::Local => "Local".to_string(),
                crate::db::ImageSource::MusicBrainz => "MusicBrainz".to_string(),
                crate::db::ImageSource::Discogs => "Discogs".to_string(),
            },
        }
    }
}
