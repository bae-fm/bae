//! Screenshot fixture data loader
//!
//! Loads fake albums/artists for screenshot generation.

use crate::db::{Database, DbAlbum, DbAlbumArtist, DbArtist};
use crate::ui::local_file_url::local_file_url;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct FixtureData {
    albums: Vec<FixtureAlbum>,
}

#[derive(Debug, Deserialize)]
struct FixtureAlbum {
    artist: String,
    title: String,
    year: i32,
}

/// Load screenshot fixtures into the database
pub async fn load_fixtures(
    db: &Database,
    fixtures_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_path = fixtures_dir.join("data.json");
    let data: FixtureData = serde_json::from_str(&std::fs::read_to_string(&data_path)?)?;

    let covers_dir = fixtures_dir.join("covers");

    // Track artists we've already created (by name -> id)
    let mut artist_ids: HashMap<String, String> = HashMap::new();

    let now = Utc::now();

    for album_data in &data.albums {
        // Get or create artist
        let artist_id = if let Some(id) = artist_ids.get(&album_data.artist) {
            id.clone()
        } else {
            let artist = DbArtist {
                id: Uuid::new_v4().to_string(),
                name: album_data.artist.clone(),
                sort_name: None,
                discogs_artist_id: None,
                bandcamp_artist_id: None,
                created_at: now,
                updated_at: now,
            };
            db.insert_artist(&artist).await?;
            artist_ids.insert(album_data.artist.clone(), artist.id.clone());
            artist.id
        };

        // Build cover filename
        let cover_filename = format!(
            "{}_{}.png",
            album_data
                .artist
                .to_lowercase()
                .replace(' ', "-")
                .replace('\'', ""),
            album_data
                .title
                .to_lowercase()
                .replace(' ', "-")
                .replace('\'', "")
        );
        let cover_path = covers_dir.join(&cover_filename);
        let cover_url = if cover_path.exists() {
            Some(local_file_url(cover_path.to_str().unwrap()))
        } else {
            None
        };

        // Create album
        let album = DbAlbum {
            id: Uuid::new_v4().to_string(),
            title: album_data.title.clone(),
            year: Some(album_data.year),
            discogs_release: None,
            musicbrainz_release: None,
            bandcamp_album_id: None,
            cover_image_id: None,
            cover_art_url: cover_url,
            is_compilation: false,
            created_at: now,
            updated_at: now,
        };
        db.insert_album(&album).await?;

        // Link artist to album
        let album_artist = DbAlbumArtist {
            id: Uuid::new_v4().to_string(),
            album_id: album.id.clone(),
            artist_id,
            position: 0,
        };
        db.insert_album_artist(&album_artist).await?;
    }

    tracing::info!("Loaded {} fixture albums", data.albums.len());
    Ok(())
}

/// Get the path to the screenshots fixtures directory
pub fn fixtures_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("screenshots")
}
