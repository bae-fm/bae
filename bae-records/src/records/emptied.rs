//! Deleting what a write empties: an album whose last release leaves, a work
//! nothing performs, and an artist nothing credits. A write that can empty
//! them reads their ids before it removes or rewrites the rows that
//! reference them ([`Emptiable::of_releases`]), and after its last change
//! deletes those that nothing references now ([`Emptiable::remove_emptied`]).
//! Only what the write could have emptied is looked at, and the check waits
//! for the end of the write because a save replaces lists, taking a credit
//! out and writing it again.

use super::*;

/// The ids of the albums, works and artists a write may leave empty.
pub(super) struct Emptiable {
    albums: Vec<String>,
    works: Vec<String>,
    artists: Vec<String>,
}

/// The works `$releases` (a JSON array of release ids) have tracks perform,
/// and the works those are parts of, however far up.
const PERFORMED_WORKS: &str = "WITH RECURSIVE performed(id) AS ( \
        SELECT tw.work_id FROM track_works tw JOIN tracks t ON t.id = tw.track_id \
        WHERE t.release_id IN (SELECT value FROM json_each(?1)) \
        UNION \
        SELECT p.parent_work_id FROM work_parts p JOIN performed ON p.child_work_id = performed.id \
    ) SELECT id FROM performed";

impl Emptiable {
    /// What removing or rewriting the rows of `release_ids` may empty: their
    /// albums, the works their tracks perform (and the works those are parts
    /// of), and every artist their album, tracks, roles and works credit.
    pub(super) fn of_releases<Q: coven::SqlRead>(
        conn: &Q,
        release_ids: &[&str],
    ) -> Result<Self, DbError> {
        let releases = serde_json::to_string(release_ids).expect("ids serialize as JSON");
        let strings = |sql: &str, params: &[&dyn rusqlite::ToSql]| {
            conn.query(sql, params, |row| row.get::<_, String>(0))
        };
        let albums = strings(
            "SELECT DISTINCT album_id FROM releases \
             WHERE id IN (SELECT value FROM json_each(?1))",
            &[&releases],
        )?;
        let works = strings(PERFORMED_WORKS, &[&releases])?;
        let works_json = serde_json::to_string(&works).expect("ids serialize as JSON");
        let artists = strings(
            "SELECT artist_id FROM album_artists WHERE album_id IN ( \
                 SELECT album_id FROM releases WHERE id IN (SELECT value FROM json_each(?1))) \
             UNION SELECT artist_id FROM track_artists WHERE track_id IN ( \
                 SELECT id FROM tracks WHERE release_id IN (SELECT value FROM json_each(?1))) \
             UNION SELECT artist_id FROM track_artist_roles WHERE track_id IN ( \
                 SELECT id FROM tracks WHERE release_id IN (SELECT value FROM json_each(?1))) \
             UNION SELECT artist_id FROM release_artist_roles \
                 WHERE release_id IN (SELECT value FROM json_each(?1)) \
             UNION SELECT artist_id FROM work_artists \
                 WHERE work_id IN (SELECT value FROM json_each(?2))",
            &[&releases, &works_json],
        )?;
        Ok(Self { albums, works, artists })
    }

    /// Also what rewriting the credits of `album_id` may empty, an album the
    /// write lands a release in or edits beside the releases it removes.
    pub(super) fn including_album<Q: coven::SqlRead>(
        mut self,
        conn: &Q,
        album_id: &str,
    ) -> Result<Self, DbError> {
        self.artists.extend(conn.query(
            "SELECT artist_id FROM album_artists WHERE album_id = ?",
            params![album_id],
            |row| row.get::<_, String>(0),
        )?);
        Ok(self)
    }

    /// Delete the albums that hold no release, the works nothing performs
    /// and the artists nothing credits, among those read. An album goes with
    /// its credits and ratings, a work with its credits and parts, an artist
    /// with its other names and picture.
    pub(super) fn remove_emptied(&self, conn: &CovenWrite<'_>) -> Result<(), DbError> {
        for album in &self.albums {
            conn.execute(
                "DELETE FROM albums WHERE id = ?1 \
                 AND NOT EXISTS (SELECT 1 FROM releases WHERE album_id = ?1)",
                params![album],
            )?;
        }
        for work in &self.works {
            conn.execute(
                "DELETE FROM works WHERE id = ?1 AND NOT EXISTS ( \
                     WITH RECURSIVE parts(id) AS ( \
                         SELECT ?1 \
                         UNION SELECT p.child_work_id FROM work_parts p JOIN parts ON p.parent_work_id = parts.id \
                     ) SELECT 1 FROM track_works tw WHERE tw.work_id IN (SELECT id FROM parts))",
                params![work],
            )?;
        }
        // An artist a merge names stays: its credits resolve through the merge.
        for artist in &self.artists {
            conn.execute(
                "DELETE FROM artists WHERE id = ?1 \
                 AND NOT EXISTS (SELECT 1 FROM album_artists WHERE artist_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM track_artists WHERE artist_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM work_artists WHERE artist_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM release_artist_roles WHERE artist_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM track_artist_roles WHERE artist_id = ?1) \
                 AND NOT EXISTS (SELECT 1 FROM artist_merges WHERE id = ?1 OR into_artist_id = ?1)",
                params![artist],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "emptied_tests.rs"]
mod tests;
