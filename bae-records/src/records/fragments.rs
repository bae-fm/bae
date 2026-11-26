//! What the library's records offer to the reads of other crates: the album a
//! release is filed under, an album's credits, which albums hold a release,
//! and which album a release's records name. A read composes them as
//! fragments, so no other crate reads a records table.

use super::release_records::{library_status_from, StoredAlbumMatch};
use crate::LibraryStatus;
use bae_store::DbError;

sqlive_compose::fragment!(
    ReleaseAlbum,
    r#"
    SELECT release.id AS release_id, release.album_id
    FROM releases AS release
"#
);

impl ReleaseAlbum {
    /// The release and the album the library files it under.
    pub fn into_ids(self) -> (String, String) {
        (self.release_id, self.album_id)
    }
}

sqlive_compose::fragment!(
    AlbumCredits on albums { credits: String },
    r#"
    SELECT COALESCE(
        (SELECT json_group_array(json_object(
                    'id', credited.id, 'name', credited.name, 'sort_name', credited.sort_name,
                    'discogs_artist_id', credited.discogs_artist_id,
                    'musicbrainz_artist_id', credited.musicbrainz_artist_id,
                    'created_at', credited.created_at,
                    'credit_key', credit_name.name_key, 'credit_name', credit_name.name,
                    'join', credit.join_phrase)
                    ORDER BY credit.position)
         FROM album_artists AS credit
         JOIN artists AS credited
           ON credited.id = (SELECT shown_artist_id FROM artist_shown_as
                             WHERE artist_id = credit.artist_id)
         LEFT JOIN artist_credits AS credit_name
                ON credit_name.artist_id = credit.artist_id
               AND credit_name.name_key = credit.credit_key
         WHERE credit.album_id = @parent.id
         HAVING COUNT(*) > 0),
        '[]') AS credits
"#);

sqlive_compose::fragment!(
    LibraryStatusRows with checks(position INTEGER, release_id TEXT, catalog TEXT, group_id TEXT)
        { position: i64, release_id: String, albums: String },
    r#"
    WITH matches(position, album_id, holds_release) AS (
        SELECT checks.position, release.album_id, 1
        FROM checks
        JOIN release_records AS record
          ON record.catalog = checks.catalog
         AND record.kind = 'pressing'
         AND record.key = checks.release_id
        JOIN releases AS release ON release.id = record.release_id
        UNION ALL
        SELECT checks.position, release.album_id, 0
        FROM checks
        JOIN release_records AS record
          ON record.catalog = checks.catalog
         AND (CASE WHEN record.kind = 'album' THEN record.key ELSE record.album_key END)
             = checks.group_id
        JOIN releases AS release ON release.id = record.release_id
        WHERE checks.group_id IS NOT NULL
    ),
    found AS MATERIALIZED (
        SELECT matches.position, album.id AS album_id, album.title AS album_title,
               album.created_at, MAX(matches.holds_release) AS holds_release
        FROM matches
        JOIN albums AS album ON album.id = matches.album_id
        GROUP BY matches.position, album.id
    )
    SELECT checks.position, checks.release_id,
           COALESCE((SELECT json_group_array(json_object(
                                'album_id', found.album_id,
                                'album_title', found.album_title,
                                'holds_release', found.holds_release)
                                ORDER BY found.holds_release DESC, found.created_at,
                                         found.album_id)
                     FROM found WHERE found.position = checks.position), '[]') AS albums
    FROM checks
    ORDER BY checks.position
"#);

/// Which library albums hold the release `release_id`, from the JSON array
/// of albums that [`LibraryStatusRows`] returns.
pub fn library_status_of_columns(
    release_id: String,
    albums: &str,
) -> Result<LibraryStatus, DbError> {
    let albums: Vec<StoredAlbumMatch> = serde_json::from_str(albums)
        .map_err(|error| DbError::unreadable_json("a library status", albums, error))?;
    Ok(library_status_from(release_id, albums))
}

sqlive_compose::fragment!(
    AlbumRecordsName with records(
        position INTEGER, catalog TEXT, kind TEXT, key TEXT, album_key TEXT
    ), excluding(release_id TEXT) { album_id: String },
    r#"
    WITH refs AS (
        SELECT position,
               CASE WHEN kind = 'pressing' THEN catalog END AS pressing_catalog,
               CASE WHEN kind = 'pressing' THEN key END AS pressing_key,
               CASE WHEN kind = 'album' OR album_key IS NOT NULL THEN catalog END
                   AS album_catalog,
               CASE WHEN kind = 'album' THEN key ELSE album_key END AS album_key
        FROM records),
    owned AS (
        SELECT *,
               CASE WHEN album_catalog IN ('musicbrainz', 'discogs')
                    THEN bae_album_id(album_catalog, album_key) END AS own
        FROM refs)
    SELECT a.id AS album_id
    FROM (
        SELECT o.position, o.own, r.album_id, 0 AS match_rank FROM owned AS o
        JOIN release_records AS rr
          ON rr.catalog = o.pressing_catalog AND rr.kind = 'pressing'
         AND rr.key = o.pressing_key
        JOIN releases AS r ON r.id = rr.release_id
        WHERE rr.release_id NOT IN (SELECT excluding.release_id FROM excluding)
        UNION ALL
        SELECT o.position, o.own, r.album_id, 1 AS match_rank FROM owned AS o
        JOIN release_records AS rr
          ON rr.catalog = o.album_catalog
         AND (CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END) = o.album_key
        JOIN releases AS r ON r.id = rr.release_id
        WHERE rr.release_id NOT IN (SELECT excluding.release_id FROM excluding)
    ) AS named
    JOIN albums AS a ON a.id = named.album_id
    ORDER BY named.position, named.match_rank, a.id IS named.own DESC, a.created_at, a.id
    LIMIT 1
"#);

sqlive_compose::fragment!(
    LibraryAlbum,
    r#"
    SELECT album.id AS album_id, album.title,
           @crate::records::fragments::AlbumCredits(parent: album) AS credits
    FROM albums AS album
"#
);

impl LibraryAlbum {
    /// The album's id, title and credits as the JSON array that
    /// [`super::read::credit_line_of_json`] reads. The credits' own SQL yields
    /// a value for every album, so a missing one is a broken composition.
    pub fn into_parts(self) -> Result<(String, String, String), DbError> {
        let credits = self.credits.ok_or_else(|| {
            DbError::unreadable(
                format!("the credits of library album {}", self.album_id),
                None::<String>,
            )
        })?;
        Ok((self.album_id, self.title, credits.credits))
    }
}
