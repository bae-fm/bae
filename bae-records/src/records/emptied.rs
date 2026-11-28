//! Deleting what a write empties: an album whose last release leaves, a work
//! nothing performs, and an artist nothing credits. A write that can empty
//! them reads their ids before it removes or rewrites the rows that
//! reference them ([`Emptiable::of_releases`]), and after its last change
//! deletes those that nothing references now ([`Emptiable::remove_emptied`]).
//! Only what the write could have emptied is looked at, and the check waits
//! for the end of the write because a save replaces lists, taking a credit
//! out and writing it again.

use super::*;
use bae_store::WriteStatements as _;

/// The ids of the albums, works and artists a write may leave empty.
pub(super) struct Emptiable {
    albums: Vec<String>,
    works: Vec<String>,
    artists: Vec<String>,
}

// The works whose tracks the releases the JSON array `releases` names
// perform, and the works those are parts of, however far up. sqlive reads
// the recursive CTE's tables whole.
coven::fragment!(
    PerformedWorks(releases: String) { id: String },
    r#"
    WITH RECURSIVE performed(id) AS (
        SELECT tw.work_id FROM track_works AS tw JOIN tracks AS t ON t.id = tw.track_id
        WHERE t.release_id IN (SELECT j.value FROM json_each(:releases) AS j)
        UNION
        SELECT p.parent_work_id FROM work_parts AS p JOIN performed ON p.child_work_id = performed.id)
    SELECT performed.id FROM performed
"#
);

// The works the releases the JSON array `releases` names perform
// ([`PerformedWorks`]).
coven::read!(
    writer PerformedWorksOfReleases(releases: String),
    r#"
    SELECT work.id
    FROM @crate::records::emptied::PerformedWorks(releases: :releases) AS work
"#
);

// The albums of the releases the JSON array `releases` names, each once.
coven::read!(
    writer AlbumsOfReleases(releases: String),
    r#"
    SELECT DISTINCT release.album_id
    FROM releases AS release
    WHERE release.id IN (SELECT asked.value FROM json_each(:releases) AS asked)
"#
);

// Every artist that the releases the JSON array `releases` names credit,
// through their album, tracks and roles, and that the works they perform
// credit ([`PerformedWorks`]).
coven::read!(
    writer CreditedByReleases(releases: String) { artist_id: String },
    r#"
    SELECT album_credit.artist_id
    FROM album_artists AS album_credit
    JOIN releases AS release ON release.album_id = album_credit.album_id
    WHERE release.id IN (SELECT asked.value FROM json_each(:releases) AS asked)
    UNION
    SELECT track_credit.artist_id
    FROM track_artists AS track_credit
    JOIN tracks AS track ON track.id = track_credit.track_id
    WHERE track.release_id IN (SELECT asked.value FROM json_each(:releases) AS asked)
    UNION
    SELECT track_role.artist_id
    FROM track_artist_roles AS track_role
    JOIN tracks AS track ON track.id = track_role.track_id
    WHERE track.release_id IN (SELECT asked.value FROM json_each(:releases) AS asked)
    UNION
    SELECT release_role.artist_id
    FROM release_artist_roles AS release_role
    WHERE release_role.release_id IN (SELECT asked.value FROM json_each(:releases) AS asked)
    UNION
    SELECT work_credit.artist_id
    FROM work_artists AS work_credit
    WHERE work_credit.work_id IN (
        SELECT work.id FROM @crate::records::emptied::PerformedWorks(releases: :releases) AS work)
"#
);

// The artists the album `album`'s credits name.
#[cfg(desktop)]
coven::read!(
    writer ArtistsOfAlbum(album: String),
    r#"
    SELECT credit.artist_id
    FROM album_artists AS credit
    WHERE credit.album_id = :album
"#
);

coven::write!(
    DeleteEmptiedAlbum(album: &'p str),
    r#"
    DELETE FROM albums WHERE id = :album
    AND NOT EXISTS (SELECT 1 FROM releases WHERE album_id = :album)
"#
);

coven::write!(
    DeleteUnperformedWork(work: &'p str),
    r#"
    DELETE FROM works WHERE id = :work AND NOT EXISTS (
        WITH RECURSIVE parts(id) AS (
            SELECT :work
            UNION
            SELECT p.child_work_id FROM work_parts p JOIN parts ON p.parent_work_id = parts.id)
        SELECT 1 FROM track_works tw WHERE tw.work_id IN (SELECT id FROM parts))
"#
);

// An artist a merge names stays: its credits resolve through the merge.
coven::write!(
    DeleteUncreditedArtist(artist: &'p str),
    r#"
    DELETE FROM artists WHERE id = :artist
    AND NOT EXISTS (SELECT 1 FROM album_artists WHERE artist_id = :artist)
    AND NOT EXISTS (SELECT 1 FROM track_artists WHERE artist_id = :artist)
    AND NOT EXISTS (SELECT 1 FROM work_artists WHERE artist_id = :artist)
    AND NOT EXISTS (SELECT 1 FROM release_artist_roles WHERE artist_id = :artist)
    AND NOT EXISTS (SELECT 1 FROM track_artist_roles WHERE artist_id = :artist)
    AND NOT EXISTS (
        SELECT 1 FROM artist_merges WHERE id = :artist OR into_artist_id = :artist)
"#
);

impl Emptiable {
    /// What removing or rewriting the rows of `release_ids` may empty: their
    /// albums, the works their tracks perform (and the works those are parts
    /// of), and every artist their album, tracks, roles and works credit.
    pub(super) fn of_releases<Q: bae_store::WriterRead>(
        conn: &Q,
        release_ids: &[&str],
    ) -> Result<Self, DbError> {
        let releases = serde_json::to_string(release_ids).expect("ids serialize as JSON");
        let albums = bae_store::statement_rows(
            conn,
            &AlbumsOfReleasesParams { releases: releases.clone() },
        )?
        .into_iter()
        .map(|row| row.album_id)
        .collect();
        let works = bae_store::statement_rows(
            conn,
            &PerformedWorksOfReleasesParams { releases: releases.clone() },
        )?
        .into_iter()
        .map(|row| row.id)
        .collect();
        let artists = bae_store::statement_rows(conn, &CreditedByReleasesParams { releases })?
            .into_iter()
            .map(|row| row.artist_id)
            .collect();
        Ok(Self { albums, works, artists })
    }

    /// Also what rewriting the credits of `album_id` may empty, an album the
    /// write lands a release in or edits beside the releases it removes.
    /// Only a release edit's save does, and that is desktop's.
    #[cfg(desktop)]
    pub(super) fn including_album<Q: bae_store::WriterRead>(
        mut self,
        conn: &Q,
        album_id: &str,
    ) -> Result<Self, DbError> {
        let statement = ArtistsOfAlbumParams { album: album_id.to_owned() };
        self.artists.extend(
            bae_store::statement_rows(conn, &statement)?.into_iter().map(|row| row.artist_id),
        );
        Ok(self)
    }

    /// Delete the albums that hold no release, the works nothing performs
    /// and the artists nothing credits, among those read. An album goes with
    /// its credits and ratings, a work with its credits and parts, an artist
    /// with its other names and picture.
    pub(super) fn remove_emptied(&self, conn: &WriteContext<'_>) -> Result<(), DbError> {
        for album in &self.albums {
            conn.run(&DeleteEmptiedAlbumParams { album })?;
        }
        for work in &self.works {
            conn.run(&DeleteUnperformedWorkParams { work })?;
        }
        for artist in &self.artists {
            conn.run(&DeleteUncreditedArtistParams { artist })?;
        }
        Ok(())
    }
}

// Whether the library holds the album `:id`.
#[cfg(test)]
coven::read!(
    writer AlbumIdHeld(id: String) { held: bool },
    "SELECT EXISTS (SELECT 1 FROM albums AS album WHERE album.id = :id) AS held"
);

// Whether the library holds the work `:id`.
#[cfg(test)]
coven::read!(
    writer WorkIdHeld(id: String) { held: bool },
    "SELECT EXISTS (SELECT 1 FROM works AS work WHERE work.id = :id) AS held"
);

// Whether the library holds the artist `:id`.
#[cfg(test)]
coven::read!(
    writer ArtistIdHeld(id: String) { held: bool },
    "SELECT EXISTS (SELECT 1 FROM artists AS artist WHERE artist.id = :id) AS held"
);

#[cfg(test)]
#[path = "emptied_tests.rs"]
mod tests;
