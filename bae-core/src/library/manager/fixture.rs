//! The state a debug UI test opens the app on, written straight into the
//! library's tables the way the Rust tests seed theirs: no folder scan, no
//! identification, no import.

use super::*;
use crate::db::{DbAlbumArtist, DbArtist, DbLibraryImage, DbRelease, DbTrack, SeededAlbum};

/// What a UI test asks its library to hold, read from the JSON file it names.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LibraryFixture {
    /// The library's albums, in the order they were added, each a second
    /// after the one before it, so a newest-first grid shows the last one on
    /// top.
    pub albums: Vec<FixtureAlbum>,
}

/// One album of a [`LibraryFixture`]: one release of `tracks`, titled in
/// order, credited to `artists`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FixtureAlbum {
    pub title: String,
    /// Who the album is credited to, in credit order: at least one.
    pub artists: Vec<String>,
    pub tracks: Vec<String>,
    /// An image file the release shows as its cover, stored as an import
    /// stores the cover it reads from a folder.
    pub cover: Option<PathBuf>,
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
    #[error("the album {album} of the library fixture credits no artist")]
    NoArtist { album: String },
    #[error("read the cover {path} of the library fixture: {detail}")]
    Cover { path: PathBuf, detail: String },
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
            albums.push(self.seeded_album(album, added, &mut artists).await?);
        }
        self.database
            .insert_seeded_albums(artists, albums)
            .await
            .map_err(LibraryError::from)?;
        Ok(())
    }

    /// `album` as the rows its write inserts, added at `added`, with each
    /// artist it credits that `artists` does not hold yet added to it.
    async fn seeded_album(
        &self,
        album: &FixtureAlbum,
        added: chrono::DateTime<chrono::Utc>,
        artists: &mut Vec<DbArtist>,
    ) -> Result<SeededAlbum, LibraryFixtureError> {
        let credited: Vec<String> = album
            .artists
            .iter()
            .map(|name| self.fixture_artist(name, added, artists))
            .collect();
        let Some(artist_id) = credited.first().cloned() else {
            return Err(LibraryFixtureError::NoArtist {
                album: album.title.clone(),
            });
        };
        let release_id = self.ids.new_id();
        let db_album = DbAlbum {
            id: self.ids.new_id(),
            title: album.title.clone(),
            artist_id,
            year: None,
            primary_release_id: Some(release_id.clone()),
            is_compilation: false,
            created_at: added,
        };
        // The first credit is the album's own artist; the rest are its
        // further credits, numbered on from it as an import numbers them.
        let credits = credited
            .iter()
            .enumerate()
            .skip(1)
            .map(|(position, artist_id)| {
                DbAlbumArtist::new(&db_album.id, artist_id, position as i32, added)
            })
            .collect();
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
        let cover = match &album.cover {
            Some(path) => Some(self.fixture_cover(path, &release_id, added).await?),
            None => None,
        };
        Ok(SeededAlbum {
            credits,
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
            cover,
        })
    }

    /// The id of the artist named `name`: the one `artists` holds, or a new
    /// one added to it.
    fn fixture_artist(
        &self,
        name: &str,
        added: chrono::DateTime<chrono::Utc>,
        artists: &mut Vec<DbArtist>,
    ) -> String {
        if let Some(artist) = artists.iter().find(|artist| artist.name == name) {
            return artist.id.clone();
        }
        let artist = DbArtist {
            id: self.ids.new_id(),
            name: name.to_string(),
            sort_name: None,
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: added,
        };
        let id = artist.id.clone();
        artists.push(artist);
        id
    }

    /// The cover row of `release_id` and the bytes it stores, read from the
    /// image at `path` and resized as every stored cover is.
    async fn fixture_cover(
        &self,
        path: &std::path::Path,
        release_id: &str,
        added: chrono::DateTime<chrono::Utc>,
    ) -> Result<(DbLibraryImage, Vec<u8>), LibraryFixtureError> {
        let failed = |detail: String| LibraryFixtureError::Cover {
            path: path.to_path_buf(),
            detail,
        };
        let source = path.to_path_buf();
        let bytes = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&source).map_err(|error| error.to_string())?;
            crate::util::cover::resize_cover(&bytes)
        })
        .await
        .map_err(|error| failed(error.to_string()))?
        .map_err(failed)?;
        let image =
            DbLibraryImage::cover(release_id, &self.ids.new_id(), "local", None, &bytes, added);
        Ok((image, bytes))
    }
}
