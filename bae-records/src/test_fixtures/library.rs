//! A library a test describes row by row: artists, albums with their
//! credits, releases and their tracks, and works with their composers, parts
//! and the tracks performing them, written through the records' own row
//! inserts in one write.

use crate::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbReleaseMedium, DbTrack,
    DbTrackArtist, DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart, Records,
};
use bae_catalogs::catalog::Catalog;
use bae_types::credit_role::{CreditRoleKind, CreditedAs};
use chrono::{DateTime, Utc};

/// The rows of a test library, each kind inserted in this order.
#[derive(Default)]
pub struct LibraryRows {
    pub artists: Vec<DbArtist>,
    pub albums: Vec<DbAlbum>,
    pub album_artists: Vec<DbAlbumArtist>,
    pub releases: Vec<DbRelease>,
    pub mediums: Vec<DbReleaseMedium>,
    pub tracks: Vec<DbTrack>,
    pub track_artists: Vec<DbTrackArtist>,
    pub works: Vec<DbWork>,
    pub work_artists: Vec<DbWorkArtist>,
    pub work_parts: Vec<DbWorkPart>,
    pub track_works: Vec<DbTrackWork>,
    pub release_artist_roles: Vec<DbReleaseArtistRole>,
    pub track_artist_roles: Vec<DbTrackArtistRole>,
}

/// A library described row by row, every row written at one time until
/// [`RecordsFixture::at`] names another.
pub struct RecordsFixture {
    rows: LibraryRows,
    at: DateTime<Utc>,
}

impl Default for RecordsFixture {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordsFixture {
    /// An empty library whose rows are written at 2026-01-01T00:00:00Z.
    pub fn new() -> Self {
        Self { rows: LibraryRows::default(), at: Self::time("2026-01-01T00:00:00Z") }
    }

    fn time(at: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(at).expect("an RFC 3339 time").with_timezone(&Utc)
    }

    /// The rows described after this are written at `at`, in RFC 3339.
    pub fn at(mut self, at: &str) -> Self {
        self.at = Self::time(at);
        self
    }

    /// The artist `id`, named `name`, with no sort name or catalog id.
    pub fn artist(self, id: &str, name: &str) -> Self {
        self.artist_row(id, name, None)
    }

    /// The artist `id`, named `name`, sorted as `sort_name`.
    pub fn sorted_artist(self, id: &str, name: &str, sort_name: &str) -> Self {
        self.artist_row(id, name, Some(sort_name))
    }

    fn artist_row(mut self, id: &str, name: &str, sort_name: Option<&str>) -> Self {
        self.rows.artists.push(DbArtist {
            id: id.to_string(),
            name: name.to_string(),
            sort_name: sort_name.map(str::to_string),
            discogs_artist_id: None,
            musicbrainz_artist_id: None,
            created_at: self.at,
        });
        self
    }

    /// The artist `id`, described already, holding the catalog ids given.
    pub fn catalog_ids(
        mut self,
        id: &str,
        discogs: Option<&str>,
        musicbrainz: Option<&str>,
    ) -> Self {
        let artist =
            self.rows.artists.iter_mut().find(|artist| artist.id == id).expect("an artist");
        artist.discogs_artist_id = discogs.map(str::to_string);
        artist.musicbrainz_artist_id = musicbrainz.map(str::to_string);
        self
    }

    /// The album `id`, titled `title`, of `year` if known, credited to
    /// `credits` in that order.
    pub fn album(mut self, id: &str, title: &str, year: Option<i32>, credits: &[&str]) -> Self {
        self.rows.albums.push(DbAlbum {
            id: id.to_string(),
            title: title.to_string(),
            year,
            is_compilation: false,
            created_at: self.at,
        });
        for (artist, position) in credits.iter().zip(0..) {
            self.rows.album_artists.push(DbAlbumArtist::new(id, artist, position, self.at));
        }
        self
    }

    /// The album `id`, described already, a compilation.
    pub fn compilation(mut self, id: &str) -> Self {
        self.rows
            .albums
            .iter_mut()
            .find(|album| album.id == id)
            .expect("an album")
            .is_compilation = true;
        self
    }

    /// The release `id` of `album`, ordered after the album's others.
    pub fn release(mut self, album: &str, id: &str) -> Self {
        self.rows
            .releases
            .push(DbRelease { created_at: self.at, ..DbRelease::new_test(album, id) });
        self
    }

    /// `release`'s one CD side, holding `tracks`, each an id and a title, in
    /// order from track 1.
    pub fn tracks(mut self, release: &str, tracks: &[(&str, &str)]) -> Self {
        self.rows
            .mediums
            .push(DbReleaseMedium { created_at: self.at, ..DbReleaseMedium::new_test(release) });
        for ((id, title), number) in tracks.iter().zip(1..) {
            self.rows.tracks.push(DbTrack {
                created_at: self.at,
                ..DbTrack::new_test(release, id, title, number)
            });
        }
        self
    }

    /// `track`'s own credits, `artists` in that order.
    pub fn track_credits(mut self, track: &str, artists: &[&str]) -> Self {
        for (artist, position) in artists.iter().zip(0..) {
            self.rows.track_artists.push(DbTrackArtist::new(track, artist, position, self.at));
        }
        self
    }

    /// The work `id`, titled `title`, of type "work".
    pub fn work(self, id: &str, title: &str) -> Self {
        self.work_of_type(id, title, "work")
    }

    /// The work `id`, titled `title`, of MusicBrainz's type `work_type`.
    pub fn work_of_type(mut self, id: &str, title: &str, work_type: &str) -> Self {
        self.rows.works.push(DbWork {
            id: id.to_string(),
            title: title.to_string(),
            disambiguation: None,
            work_type: Some(work_type.to_string()),
            musicbrainz_work_id: format!("mb-work-{id}"),
            created_at: self.at,
        });
        self
    }

    /// `artist` credited on `work` from MusicBrainz as its composer, after
    /// its others.
    pub fn composer(self, work: &str, artist: &str) -> Self {
        self.work_credit(work, artist, CreditedAs::of(CreditRoleKind::Composer))
    }

    /// `artist` credited on `work` from MusicBrainz in `role`, after its
    /// others.
    pub fn work_credit(mut self, work: &str, artist: &str, role: CreditedAs) -> Self {
        let position = self.rows.work_artists.iter().filter(|link| link.work_id == work).count();
        self.rows.work_artists.push(DbWorkArtist::new(
            work,
            artist,
            position as i32,
            Catalog::MusicBrainz,
            role,
            None,
            self.at,
        ));
        self
    }

    /// `child` a part of `parent`, after its others.
    pub fn part(mut self, parent: &str, child: &str) -> Self {
        let position =
            self.rows.work_parts.iter().filter(|part| part.parent_work_id == parent).count();
        self.rows.work_parts.push(DbWorkPart::new(
            parent,
            child,
            position as i32,
            Catalog::MusicBrainz,
            self.at,
        ));
        self
    }

    /// `track` performing `work`, after the others it performs.
    pub fn performs(mut self, track: &str, work: &str) -> Self {
        let position = self.rows.track_works.iter().filter(|link| link.track_id == track).count();
        self.rows.track_works.push(DbTrackWork::new(
            track,
            work,
            position as i32,
            Catalog::MusicBrainz,
            coven::IdSource::new_id(&coven::UuidIds).to_string(),
            self.at,
        ));
        self
    }

    /// The role credit `id` naming `artist` on `release` in `role`, as `source`
    /// stated it.
    pub fn release_role(
        mut self,
        id: &str,
        release: &str,
        artist: &str,
        (source, role): (Catalog, CreditedAs),
    ) -> Self {
        let position =
            self.rows.release_artist_roles.iter().filter(|role| role.release_id == release).count();
        self.rows.release_artist_roles.push(DbReleaseArtistRole::new(
            release,
            artist,
            position as i32,
            source,
            role,
            None,
            id.to_string(),
            self.at,
        ));
        self
    }

    /// The role credit `id` naming `artist` on `track` in `role`, as `source`
    /// stated it.
    pub fn track_role(
        mut self,
        id: &str,
        track: &str,
        artist: &str,
        (source, role): (Catalog, CreditedAs),
    ) -> Self {
        let position =
            self.rows.track_artist_roles.iter().filter(|role| role.track_id == track).count();
        self.rows.track_artist_roles.push(DbTrackArtistRole::new(
            track,
            artist,
            position as i32,
            source,
            role,
            None,
            id.to_string(),
            self.at,
        ));
        self
    }

    /// Change the rows described so far as `edit` does: a column no method
    /// here names.
    pub fn edit(mut self, edit: impl FnOnce(&mut LibraryRows)) -> Self {
        edit(&mut self.rows);
        self
    }

    /// The rows described, for a write that takes them whole.
    pub fn rows(self) -> LibraryRows {
        self.rows
    }

    /// Write the library in one write.
    pub async fn write(self, records: &Records) {
        records.insert_library_rows_for_test(self.rows).await.unwrap();
    }

    /// Write the library inside `conn`'s write, its labels made at the time
    /// its last rows were described.
    pub fn write_on(self, conn: &coven::CovenWrite<'_>) -> Result<(), crate::IdentityWriteError> {
        let labels = self.rows.label_parts(&coven::UuidIds, self.at);
        self.rows.write_on(conn, &labels)
    }
}
