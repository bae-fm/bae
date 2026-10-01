//! Library contents a debug UI test opens the app on, written straight into
//! the library's tables the way the Rust tests seed theirs: no folder scan,
//! no identification, no import.

use super::*;
use crate::db::{DbAlbumArtist, DbArtist, DbRelease, DbTrack, SeededAlbum};

/// What a UI test asks its library to hold, read from the JSON file it names.
/// The albums are in the order they were added, each a second after the one
/// before it, so a newest-first grid shows the last one on top.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LibraryFixture {
    pub albums: Vec<FixtureAlbum>,
}

/// One album of a [`LibraryFixture`]: one release of `tracks`, titled in
/// order, credited to `artist`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixtureAlbum {
    pub title: String,
    pub artist: String,
    pub tracks: Vec<String>,
}

/// Why a fixture was not written.
#[derive(Error, Debug)]
pub enum LibraryFixtureError {
    #[error("read library fixture {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parse library fixture {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("write library fixture: {0}")]
    Write(#[from] LibraryError),
}

impl LibraryFixture {
    pub fn read(path: &std::path::Path) -> Result<Self, LibraryFixtureError> {
        let bytes = std::fs::read(path).map_err(|source| LibraryFixtureError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_slice(&bytes).map_err(|source| LibraryFixtureError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }
}

impl LibraryManager {
    /// Write `fixture`'s albums into the library in one transaction, the last
    /// one added now and each before it a second earlier. One artist row per
    /// distinct name.
    pub async fn write_fixture(&self, fixture: &LibraryFixture) -> Result<(), LibraryFixtureError> {
        let now = self.clock.now();
        let count = fixture.albums.len() as i64;
        let mut artists: Vec<DbArtist> = Vec::new();
        let mut albums = Vec::with_capacity(fixture.albums.len());
        for (index, album) in fixture.albums.iter().enumerate() {
            let added = now - chrono::Duration::seconds(count - 1 - index as i64);
            let artist_id = match artists.iter().find(|artist| artist.name == album.artist) {
                Some(artist) => artist.id.clone(),
                None => {
                    let artist = DbArtist {
                        id: self.ids.new_id(),
                        name: album.artist.clone(),
                        sort_name: None,
                        discogs_artist_id: None,
                        musicbrainz_artist_id: None,
                        created_at: added,
                    };
                    let id = artist.id.clone();
                    artists.push(artist);
                    id
                }
            };
            let release_id = self.ids.new_id();
            let db_album = DbAlbum {
                id: self.ids.new_id(),
                title: album.title.clone(),
                artist_id: artist_id.clone(),
                year: None,
                primary_release_id: Some(release_id.clone()),
                is_compilation: false,
                created_at: added,
            };
            let tracks = album
                .tracks
                .iter()
                .enumerate()
                .map(|(number, title)| DbTrack {
                    id: self.ids.new_id(),
                    release_id: release_id.clone(),
                    title: title.clone(),
                    side: Some(1),
                    track_number: Some(number as i32 + 1),
                    duration_ms: Some(180_000),
                    discogs_position: None,
                    created_at: added,
                })
                .collect();
            albums.push(SeededAlbum {
                credit: DbAlbumArtist::new(&db_album.id, &artist_id, 0, added),
                release: DbRelease {
                    id: release_id,
                    album_id: db_album.id.clone(),
                    release_name: None,
                    pressing: crate::pressing::Pressing::blank(),
                    draft_from_tags: false,
                    remote: false,
                    source_folder_name: None,
                    content_hash: None,
                    album_loudness_lufs: None,
                    album_peak_linear: None,
                    created_at: added,
                },
                album: db_album,
                tracks,
            });
        }
        self.database
            .insert_seeded_albums(artists, albums)
            .await
            .map_err(LibraryError::from)?;
        Ok(())
    }
}
