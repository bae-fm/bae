//! Demo data provider for screenshot generation
//!
//! When the `demo` feature is enabled, this module provides fake library data
//! without requiring a database connection. Used for web-based screenshots.

use crate::db::{DbAlbum, DbArtist};
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use uuid::Uuid;

/// Embedded fixture data (compiled into the binary)
const FIXTURE_JSON: &str = include_str!("../../fixtures/screenshots/data.json");

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

/// Parsed demo data, lazily initialized
struct DemoData {
    albums: Vec<DbAlbum>,
    artists: Vec<DbArtist>,
    album_artist_map: HashMap<String, String>, // album_id -> artist_id
}

static DEMO_DATA: OnceLock<DemoData> = OnceLock::new();

fn get_demo_data() -> &'static DemoData {
    DEMO_DATA.get_or_init(|| {
        let fixture: FixtureData =
            serde_json::from_str(FIXTURE_JSON).expect("Failed to parse fixture JSON");

        let now = Utc::now();
        let mut albums = Vec::new();
        let mut artists = Vec::new();
        let mut artist_ids: HashMap<String, String> = HashMap::new();
        let mut album_artist_map = HashMap::new();

        for album_data in fixture.albums {
            // Get or create artist
            let artist_id = artist_ids
                .entry(album_data.artist.clone())
                .or_insert_with(|| {
                    let id = Uuid::new_v4().to_string();
                    artists.push(DbArtist {
                        id: id.clone(),
                        name: album_data.artist.clone(),
                        sort_name: None,
                        discogs_artist_id: None,
                        bandcamp_artist_id: None,
                        created_at: now,
                        updated_at: now,
                    });
                    id
                })
                .clone();

            // Build cover URL from fixture covers
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
            // For web, we'll serve covers from /assets/demo-covers/
            let cover_url = Some(format!("/assets/demo-covers/{}", cover_filename));

            let album_id = Uuid::new_v4().to_string();
            album_artist_map.insert(album_id.clone(), artist_id);

            albums.push(DbAlbum {
                id: album_id,
                title: album_data.title,
                year: Some(album_data.year),
                discogs_release: None,
                musicbrainz_release: None,
                bandcamp_album_id: None,
                cover_image_id: None,
                cover_art_url: cover_url,
                is_compilation: false,
                created_at: now,
                updated_at: now,
            });
        }

        DemoData {
            albums,
            artists,
            album_artist_map,
        }
    })
}

/// Get all demo albums
pub fn get_albums() -> Vec<DbAlbum> {
    get_demo_data().albums.clone()
}

/// Get artists for a demo album
pub fn get_artists_for_album(album_id: &str) -> Vec<DbArtist> {
    let data = get_demo_data();
    if let Some(artist_id) = data.album_artist_map.get(album_id) {
        data.artists
            .iter()
            .filter(|a| &a.id == artist_id)
            .cloned()
            .collect()
    } else {
        Vec::new()
    }
}
