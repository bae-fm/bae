//! What the library's records offer to the reads of other crates: the album a
//! release is filed under, an album's credits, which albums hold a release,
//! and which album a release's records name. A read composes them as
//! fragments, so no other crate reads a records table.

use super::query::AlbumSummaryRow;
use super::read::album_year;
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

// Each release's catalog records, in the order surfaces list catalogs, as a
// JSON array for [`crate::records::read::records_of_json`] that is empty for
// a release no catalog describes; and what they identify the release as:
// 'pressing' when a catalog's record names its pressing, 'album' when the
// records name only its album, and NULL when no catalog describes it.
sqlive_compose::fragment!(
    ReleaseRecords {
        records: String,
        records_kind: Option<String>,
    },
    r#"
    SELECT release.id AS records_release_id,
        (SELECT json_group_array(json_object(
                    'catalog', record.catalog, 'key', record.key, 'kind', record.kind,
                    'album_key', record.album_key)
                    ORDER BY bae_catalog_rank(record.catalog), record.catalog)
         FROM release_records AS record
         WHERE record.release_id = release.id) AS records,
        CASE WHEN EXISTS (SELECT 1 FROM release_records AS record
                          WHERE record.release_id = release.id AND record.kind = 'pressing')
                 THEN 'pressing'
             WHEN EXISTS (SELECT 1 FROM release_records AS record
                          WHERE record.release_id = release.id)
                 THEN 'album' END AS records_kind
    FROM releases AS release
"#
);

impl ReleaseRecords {
    /// The release's records, as the JSON array
    /// [`crate::records::read::records_of_json`] reads.
    pub(crate) fn records_json(&self) -> &str {
        &self.records
    }

    /// What the records identify the release as; `None` when no catalog
    /// describes it.
    pub(crate) fn records_kind(&self) -> Option<&str> {
        self.records_kind.as_deref()
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

// The ids of the album's releases in album order (the primary release first),
// as a JSON array.
sqlive_compose::fragment!(
    AlbumReleaseIds on albums { release_ids: String },
    r#"
    SELECT json_group_array(held.id ORDER BY held.album_order, held.id) AS release_ids
    FROM releases AS held
    WHERE held.album_id = @parent.id
"#
);

// What a list shows of each album: its title, year and whether it is a
// compilation, its primary release with that release's cover version, its
// credits and its releases.
sqlive_compose::fragment!(
    AlbumSummary { is_compilation: bool, cover_version: Option<String> },
    r#"
    SELECT album.id AS summary_album_id, album.title, album.year, album.is_compilation,
           @crate::records::fragments::AlbumPrimaryRelease(parent: album) AS primary_release,
           (SELECT cover.version
            FROM @crate::records::files::CoverVersion AS cover
            WHERE cover.id = @crate::records::fragments::AlbumPrimaryRelease(parent: album))
               AS cover_version,
           @crate::records::fragments::AlbumCredits(parent: album) AS credits,
           @crate::records::fragments::AlbumReleaseIds(parent: album) AS releases
    FROM albums AS album
"#
);

impl AlbumSummary {
    /// The summary as the row [`AlbumSummaryRow::process`] decodes. The
    /// library lists an album only while it holds a release, and the credits'
    /// own SQL yields a value for every album, so a missing one of either is
    /// unreadable.
    pub fn into_row(self) -> Result<AlbumSummaryRow, DbError> {
        let missing = |what: &str| {
            DbError::unreadable(
                format!("{what} of album {}", self.summary_album_id),
                None::<String>,
            )
        };
        let primary_release_id =
            self.primary_release.as_ref().ok_or_else(|| missing("the primary release"))?;
        let primary_release_id = primary_release_id.release_id.clone();
        let credits = self.credits.as_ref().ok_or_else(|| missing("the credits"))?.credits.clone();
        let year = album_year(self.year)?;
        Ok(AlbumSummaryRow {
            id: self.summary_album_id,
            title: self.title,
            year,
            is_compilation: self.is_compilation,
            album_credits: credits,
            release_ids_json: self.releases.release_ids,
            primary_release_id,
            primary_cover_version: self.cover_version,
        })
    }
}

// For each album and each artist, the album's credits that show another artist
// than that one, as a JSON array in list order, NULL for none: what the row of
// the album under that artist names beside it, in a list grouped by artist.
sqlive_compose::fragment!(
    AlbumCreditsBeside { credits: Option<String> },
    r#"
    SELECT album.id AS beside_album_id, grouping.id AS beside_artist_id,
        (SELECT json_group_array(json(other.value) ORDER BY other.key)
         FROM json_each(@crate::records::fragments::AlbumCredits(parent: album)) AS other
         WHERE other.value ->> 'id' <> grouping.id
         HAVING COUNT(*) > 0) AS credits
    FROM albums AS album
    CROSS JOIN artists AS grouping
"#
);

impl AlbumCreditsBeside {
    /// The line the other credits print; `None` when the album has none.
    pub fn into_line(self) -> Result<Option<bae_types::credits::CreditLine>, DbError> {
        self.credits.as_deref().map(crate::records::read::credit_line_of_json).transpose()
    }
}

// Each album's row.
sqlive_compose::fragment!(
    AlbumRow { is_compilation: bool },
    r#"
    SELECT album.id AS row_album_id, album.title, album.year, album.is_compilation,
           album.created_at
    FROM albums AS album
"#
);

impl AlbumRow {
    /// The album the row holds.
    pub fn into_album(self) -> Result<crate::DbAlbum, DbError> {
        let year = album_year(self.year)?;
        let created_at = bae_util::timestamp::parse_rfc3339(&self.created_at)
            .map_err(|_unparsed| DbError::unreadable("an album's created_at", &self.created_at))?;
        Ok(crate::DbAlbum {
            id: self.row_album_id,
            title: self.title,
            year,
            is_compilation: self.is_compilation,
            created_at,
        })
    }
}

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

// The library album the records of `records` file a release under, one row
// with its id or no row for none, leaving out the records of the releases
// `excluding`. Each record is a `(position, catalog, kind, key, album_key)`:
// `kind` 'pressing' or 'album', `key` the pressing's or the album's, and
// `album_key` the album a pressing record files it under, NULL for none.
//
// The applied record, which is the record at the lowest `position`, comes
// first, then each other record in order, and the first record that names an
// album the library holds decides. A record names an album through a copy of
// its pressing, which is a pressing record that names the same release (on
// `idx_release_records_catalog_key`), or else through its album, which is a
// record that names the same release group or master (on
// `idx_release_records_catalog_album`). Library albums are the curator's: two
// albums the records name stay two, and the applied record's wins over a
// partner's. Where one record names several albums, the record's own group
// album ([`crate::identity::album_id`]) wins if the library holds it, and
// otherwise the oldest album (ties by id), so the answer is one album.
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

// The album's primary release: its lowest-ordered release, ties broken by
// release id, so every device that holds the same rows names the same one. No
// row for an album that holds no release.
sqlive_compose::fragment!(
    AlbumPrimaryRelease on albums { release_id: String },
    r#"
    SELECT lowest.id AS release_id
    FROM releases AS lowest
    WHERE lowest.album_id = @parent.id
    ORDER BY lowest.album_order, lowest.id
    LIMIT 1
"#
);

// The album the release `release` is in (`current_album_id`) and the library
// album a save files it under once its records are `records`
// (`target_album_id`, NULL for a new album), each record as
// [`AlbumRecordsName`] reads it; no row when the library holds no such
// release. In order:
//
// 1. No records: a new album holding only the release.
// 2. The album the records name ([`AlbumRecordsName`], leaving out the
//    release's own records), even when the current album would also fit.
// 3. With the release alone in its album, the album of the group the records
//    name ([`crate::identity::album_id_for_records`]), new when the library
//    does not hold it; the current album when they name no group or the
//    current album is the group's.
// 4. The current album when none of its other releases names another album
//    than the records do on a catalog both name.
// 5. Else the group's album, new when the library does not hold it, or a new
//    album when the records name no group.
sqlive_compose::fragment!(
    FiledByRecords(release: String) with records(
        position INTEGER, catalog TEXT, kind TEXT, key TEXT, album_key TEXT
    ) { current_album_id: String, target_album_id: Option<String> },
    r#"
    WITH album_refs AS (
        SELECT position, catalog,
               CASE WHEN kind = 'album' THEN key ELSE album_key END AS album_key
        FROM records WHERE kind = 'album' OR album_key IS NOT NULL),
    current AS (
        SELECT moving.album_id FROM releases AS moving WHERE moving.id = :release),
    group_album AS (
        SELECT bae_album_id(catalog, album_key) AS id FROM album_refs
        WHERE catalog IN ('musicbrainz', 'discogs')
        ORDER BY CASE catalog WHEN 'musicbrainz' THEN 0 ELSE 1 END, position LIMIT 1),
    held_group_album AS (
        SELECT held.id FROM group_album JOIN albums AS held ON held.id = group_album.id),
    named_album AS (
        SELECT named.*
        FROM @crate::records::fragments::AlbumRecordsName(
            records: (SELECT position, catalog, kind, key, album_key FROM records),
            excluding: (SELECT :release)) AS named),
    beside AS (
        SELECT EXISTS (SELECT 1 FROM releases AS other
                       JOIN current ON other.album_id = current.album_id
                       WHERE other.id <> :release) AS holds_others,
               EXISTS (SELECT 1 FROM releases AS other
                       JOIN current ON other.album_id = current.album_id
                       JOIN release_records AS held ON held.release_id = other.id
                       JOIN album_refs AS new ON held.catalog = new.catalog
                       WHERE other.id <> :release
                         AND (CASE WHEN held.kind = 'album' THEN held.key
                                   ELSE held.album_key END) <> new.album_key) AS disagrees)
    SELECT current.album_id AS current_album_id,
           CASE
               WHEN NOT EXISTS (SELECT 1 FROM records) THEN NULL
               WHEN EXISTS (SELECT 1 FROM named_album)
                   THEN (SELECT album_id FROM named_album)
               WHEN NOT beside.holds_others THEN
                   CASE WHEN NOT EXISTS (SELECT 1 FROM group_album)
                             OR (SELECT id FROM group_album) = current.album_id
                        THEN current.album_id
                        ELSE (SELECT id FROM held_group_album) END
               WHEN NOT beside.disagrees THEN current.album_id
               ELSE (SELECT id FROM held_group_album)
           END AS target_album_id
    FROM current CROSS JOIN beside
"#
);

sqlive_compose::fragment!(
    AlbumTitle,
    r#"
    SELECT titled.id AS album_id, titled.title
    FROM albums AS titled
"#
);

impl AlbumTitle {
    /// The album's title.
    pub fn into_title(self) -> String {
        self.title
    }
}

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

// Each track's release and title, and the credits it shows (its own, else its
// album's), as the JSON array [`super::read::credit_line_of_json`] reads.
sqlive_compose::fragment!(
    ShownTrack,
    r#"
    SELECT track.id AS track_id, track.release_id, track.title,
           @crate::records::credit_fragments::TrackCredits(parent: track) AS credits
    FROM tracks AS track
"#
);

impl ShownTrack {
    /// The track's release, title and shown credits as the JSON array that
    /// [`super::read::credit_line_of_json`] reads. Every track's release has
    /// an album, so the credits' own SQL yields a value for every track: a
    /// missing one is a broken composition.
    pub fn into_parts(self) -> Result<(String, String, String), DbError> {
        let shown = self.credits.ok_or_else(|| {
            DbError::unreadable(format!("the credits of track {}", self.track_id), None::<String>)
        })?;
        Ok((self.release_id, self.title, shown.credits))
    }
}
