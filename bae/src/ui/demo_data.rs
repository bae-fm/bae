//! Demo data for screenshot generation
//!
//! Provides static fixture data for rendering the UI without a database.
//! Used when the `demo` feature is enabled.

use crate::ui::display_types::{Album, Artist, Track, TrackImportState};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Embedded fixture data (compiled into the binary)
const FIXTURE_JSON: &str = include_str!("../../../website/fixtures/screenshots/data.json");

#[derive(Debug, Deserialize)]
struct FixtureData {
    albums: Vec<FixtureAlbum>,
}

#[derive(Debug, Deserialize)]
struct FixtureAlbum {
    artist: String,
    title: String,
    year: i32,
    #[serde(default)]
    tracks: Vec<String>,
}

use crate::ui::display_types::Release;

/// Parsed demo data, lazily initialized
struct DemoData {
    albums: Vec<Album>,
    artists: Vec<Artist>,
    artists_by_album: HashMap<String, Vec<Artist>>,
    tracks_by_album: HashMap<String, Vec<Track>>,
    releases_by_album: HashMap<String, Vec<Release>>,
}

static DEMO_DATA: OnceLock<DemoData> = OnceLock::new();

/// Generate a stable ID from a string (for consistent IDs across runs)
fn stable_id(s: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Generate cover URL from artist and title
fn cover_url(artist: &str, title: &str) -> String {
    let filename = format!(
        "{}_{}.png",
        artist.to_lowercase().replace(' ', "-").replace('\'', ""),
        title.to_lowercase().replace(' ', "-").replace('\'', "")
    );
    // For demo mode, serve covers from the fixtures directory
    // In web build, these would be served from a static path
    format!("/assets/demo-covers/{}", filename)
}

fn get_demo_data() -> &'static DemoData {
    DEMO_DATA.get_or_init(|| {
        let fixture: FixtureData =
            serde_json::from_str(FIXTURE_JSON).expect("Failed to parse fixture JSON");

        let mut albums = Vec::new();
        let mut artists = Vec::new();
        let mut artist_ids: HashMap<String, String> = HashMap::new();
        let mut artists_by_album = HashMap::new();
        let mut tracks_by_album = HashMap::new();
        let mut releases_by_album = HashMap::new();

        for album_data in fixture.albums {
            // Get or create artist
            let artist_id = artist_ids
                .entry(album_data.artist.clone())
                .or_insert_with(|| {
                    let id = stable_id(&format!("artist:{}", album_data.artist));
                    artists.push(Artist {
                        id: id.clone(),
                        name: album_data.artist.clone(),
                    });
                    id
                })
                .clone();

            let album_id = stable_id(&format!("album:{}:{}", album_data.artist, album_data.title));
            let release_id = stable_id(&format!(
                "release:{}:{}",
                album_data.artist, album_data.title
            ));

            // Create album
            albums.push(Album {
                id: album_id.clone(),
                title: album_data.title.clone(),
                year: Some(album_data.year),
                cover_url: Some(cover_url(&album_data.artist, &album_data.title)),
                is_compilation: false,
            });

            // Link artist to album
            let album_artist = Artist {
                id: artist_id,
                name: album_data.artist.clone(),
            };
            artists_by_album.insert(album_id.clone(), vec![album_artist]);

            // Create release (one per album for demo)
            let release = Release {
                id: release_id.clone(),
                album_id: album_id.clone(),
                release_name: None,
                year: Some(album_data.year),
                format: Some("Digital".to_string()),
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
                discogs_release_id: None,
                musicbrainz_release_id: None,
            };
            releases_by_album.insert(album_id.clone(), vec![release]);

            // Create tracks
            let tracks: Vec<Track> = album_data
                .tracks
                .iter()
                .enumerate()
                .map(|(i, title)| {
                    let track_id = stable_id(&format!(
                        "track:{}:{}:{}",
                        album_data.artist, album_data.title, title
                    ));
                    Track {
                        id: track_id,
                        title: title.clone(),
                        track_number: Some((i + 1) as i32),
                        disc_number: Some(1),
                        duration_ms: Some(180_000 + (i as i64 * 30_000)), // Fake durations 3:00-5:30
                        is_available: true,
                        import_state: TrackImportState::Complete,
                    }
                })
                .collect();
            tracks_by_album.insert(album_id, tracks);
        }

        DemoData {
            albums,
            artists,
            artists_by_album,
            tracks_by_album,
            releases_by_album,
        }
    })
}

/// Get all demo albums
pub fn get_albums() -> Vec<Album> {
    get_demo_data().albums.clone()
}

/// Get all demo artists
pub fn get_artists() -> Vec<Artist> {
    get_demo_data().artists.clone()
}

/// Get artists by album ID
pub fn get_artists_by_album() -> HashMap<String, Vec<Artist>> {
    get_demo_data().artists_by_album.clone()
}

/// Get artists for a specific album
pub fn get_artists_for_album(album_id: &str) -> Vec<Artist> {
    get_demo_data()
        .artists_by_album
        .get(album_id)
        .cloned()
        .unwrap_or_default()
}

/// Get tracks for a specific album
pub fn get_tracks_for_album(album_id: &str) -> Vec<Track> {
    get_demo_data()
        .tracks_by_album
        .get(album_id)
        .cloned()
        .unwrap_or_default()
}

/// Get releases for a specific album
pub fn get_releases_for_album(album_id: &str) -> Vec<Release> {
    get_demo_data()
        .releases_by_album
        .get(album_id)
        .cloned()
        .unwrap_or_default()
}

/// Get a specific album by ID
pub fn get_album(album_id: &str) -> Option<Album> {
    get_demo_data()
        .albums
        .iter()
        .find(|a| a.id == album_id)
        .cloned()
}
