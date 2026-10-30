//! The state a debug UI test opens the app on, written straight into the
//! library's tables the way the Rust tests seed theirs: no folder scan, no
//! identification, no import.

use super::*;
use crate::db::{DbAlbumArtist, DbArtist, DbLibraryImage, DbRelease, DbTrack, SeededAlbum};

mod import_list;
mod playing;
pub use import_list::{
    FixtureCandidate, FixtureCandidateState, FixtureWatchedFolder, FIXTURE_IMPORT_FAILURE,
    FIXTURE_LOOKUP_FAILURE,
};
pub use playing::FixturePlaying;

/// What a UI test asks its library to hold, read from the JSON file it names.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LibraryFixture {
    /// The library's albums, in the order they were added, each a second
    /// after the one before it, so a newest-first grid shows the last one on
    /// top.
    pub albums: Vec<FixtureAlbum>,
    /// The folders the library watches, in order, and what the import list
    /// lists under each.
    pub watched_folders: Vec<FixtureWatchedFolder>,
    /// The album playback is on as the library opens, if any.
    pub playing: Option<FixturePlaying>,
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
    #[error("the library fixture does not hold: {0}")]
    Invalid(String),
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
    /// Write `fixture` into the library: its albums and the album playing in
    /// one transaction, the playing album added now, the last album a second
    /// before it and each before that a second earlier, one artist row per
    /// distinct name; then its watched folders, each write the one the scan or
    /// identification makes; then where playback resumes.
    pub async fn write_fixture(&self, fixture: &LibraryFixture) -> Result<(), LibraryFixtureError> {
        let now = chrono::DateTime::<chrono::Utc>::from(self.clock.now());
        let count = (fixture.albums.len() + usize::from(fixture.playing.is_some())) as i64;
        let added = |index: usize| now - chrono::Duration::seconds(count - 1 - index as i64);
        let mut artists: Vec<DbArtist> = Vec::new();
        let mut albums = Vec::with_capacity(count as usize);
        for (index, album) in fixture.albums.iter().enumerate() {
            albums.push(self.seeded_album(album, added(index), &mut artists).await?);
        }
        let resume = match &fixture.playing {
            Some(playing) => {
                let (album, track_id) = self
                    .seeded_playing_album(playing, added(fixture.albums.len()), &mut artists)
                    .await?;
                let release_id = album.release.id.clone();
                albums.push(album);
                Some((release_id, track_id, playing.position_ms))
            }
            None => None,
        };
        // A write that changes nothing is refused, so a fixture of no albums
        // writes none.
        if !albums.is_empty() {
            self.database
                .insert_seeded_albums(artists, albums)
                .await
                .map_err(LibraryError::from)?;
        }
        self.write_fixture_watched_folders(&fixture.watched_folders)
            .await?;
        if let Some((release_id, track_id, position_ms)) = resume {
            self.write_resume_point(&release_id, track_id, Some(position_ms))
                .await?;
        }
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
        if credited.is_empty() {
            return Err(LibraryFixtureError::NoArtist {
                album: album.title.clone(),
            });
        }
        let release_id = self.ids.new_id().to_string();
        let db_album = DbAlbum {
            id: self.ids.new_id().to_string(),
            title: album.title.clone(),
            year: None,
            is_compilation: false,
            created_at: added,
        };
        let credits = credited
            .iter()
            .zip(0..)
            .map(|(artist_id, position)| {
                DbAlbumArtist::new(&db_album.id, artist_id, position, added)
            })
            .collect();
        // One disc of unstated carrier, its tracks numbered in order.
        let medium = crate::db::DbReleaseMedium::new(
            &release_id,
            std::num::NonZeroU16::MIN,
            crate::mediums::ReleaseMedium::one_sided(None),
            added,
        );
        let tracks = album
            .tracks
            .iter()
            .enumerate()
            .map(|(index, title)| DbTrack {
                id: self.ids.new_id().to_string(),
                release_id: release_id.clone(),
                title: title.clone(),
                medium_id: medium.id.clone(),
                side: std::num::NonZeroU8::MIN,
                track_number: std::num::NonZeroU32::new(
                    u32::try_from(index + 1).expect("a fixture album's tracks fit u32"),
                )
                .expect("numbers count from one"),
                duration_ms: 180_000,
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
                pressing: bae_types::pressing::Pressing::blank(),

                source_folder_name: None,
                album_loudness_lufs: None,
                album_peak_linear: None,
                created_at: added,
            },
            album: db_album,
            medium,
            tracks,
            cover,
            audio: None,
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
            id: self.ids.new_id().to_string(),
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
            bae_util::stored_image::stored_jpeg(&bytes)
        })
        .await
        .map_err(|error| failed(error.to_string()))?
        .map_err(failed)?;
        let image = DbLibraryImage::cover(release_id, "local", None, added);
        Ok((image, bytes))
    }
}
