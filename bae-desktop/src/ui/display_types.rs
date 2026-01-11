//! Conversions from DB types to bae-ui display types

use crate::ui::image_url;
use bae_core::db::{DbAlbum, DbArtist, DbRelease, DbTrack, ImportStatus};
use bae_core::playback::PlaybackState;

// Re-export bae-ui types so existing code continues to work
pub use bae_ui::{Album, Artist, PlaybackDisplay, QueueItem, Release, Track, TrackImportState};

pub fn album_from_db_ref(db: &DbAlbum) -> Album {
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

pub fn artist_from_db_ref(db: &DbArtist) -> Artist {
    Artist {
        id: db.id.clone(),
        name: db.name.clone(),
    }
}

pub fn track_from_db_ref(db: &DbTrack) -> Track {
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

pub fn playback_display_from_state(state: &PlaybackState) -> PlaybackDisplay {
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

pub fn release_from_db_ref(db: &DbRelease) -> Release {
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
        musicbrainz_release_id: None,
    }
}
