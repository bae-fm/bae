//! Reading and writing a release's records — every catalog's description of
//! it.

use super::*;

impl Records {
    /// Test seam: write a release's records outside the atomic import path,
    /// replacing any it had.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn insert_release_records(
        &self,
        release_id: &str,
        records: &[bae_catalogs::records::ReleaseRecord],
    ) -> Result<(), DbError> {
        let release_id = release_id.to_string();
        let records = records.to_vec();
        self.store.call_sql(move |sql| write_release_records(&sql, &release_id, &records)).await
    }

    /// Every record for a release. Empty if no catalog describes it.
    pub async fn get_release_records(
        &self,
        release_id: &str,
    ) -> Result<Vec<bae_catalogs::records::ReleaseRecord>, DbError> {
        let release_id = release_id.to_string();
        self.store.read(move |sql| get_release_records_on(&sql, &release_id)).await
    }

    /// The library album `records` file a release under, leaving out the
    /// releases `excluding`, as [`album_records_name_sql`] names it.
    pub async fn album_records_name(
        &self,
        records: &[bae_catalogs::records::ReleaseRecord],
        excluding: &[String],
    ) -> Result<Option<String>, DbError> {
        let records = records.to_vec();
        let excluding = excluding.to_vec();
        self.store.read(move |sql| album_records_name_on(&sql, &records, &excluding)).await
    }

    /// The album a save files the release `release_id` under once its
    /// records are `records`, and the album it is in now, as
    /// [`records_target_album_sql`] names them; `None` when the library holds
    /// no such release.
    pub async fn records_target_album(
        &self,
        release_id: &str,
        records: &[bae_catalogs::records::ReleaseRecord],
    ) -> Result<Option<RecordsTargetAlbum>, DbError> {
        let (release_id, records) = (release_id.to_string(), records.to_vec());
        self.store.read(move |sql| records_target_album_on(&sql, &release_id, &records)).await
    }

    /// Replace a release's record rows and move the release between albums
    /// when the target differs from the source, in one transaction (see
    /// `set_records_on`).
    #[cfg(all(desktop, any(test, feature = "test-utils")))]
    pub async fn set_records_atomic(&self, change: RecordsChange) -> Result<(), DbError> {
        let now = self.store.now_utc();
        let ids = self.ids.clone();
        self.store
            .call_sql(move |sql| {
                let emptiable = Emptiable::of_releases(&sql, &[change.release_id.as_str()])?;
                set_records_on(&sql, &change, now, &*ids)?;
                emptiable.remove_emptied(&sql)
            })
            .await
    }

    /// Check, for each candidate in `checks`, whether the library already
    /// holds the same pressing or the same album (group), for an automation
    /// client that names the releases it asks about. The import pane's rows
    /// read whether the library holds them in their own statement
    /// ([`pressing_held_sql`]).
    ///
    /// [`library_status_sql`] says which albums each check finds.
    pub async fn check_releases_in_library(
        &self,
        checks: &[LibraryCheck],
    ) -> Result<Vec<LibraryStatus>, DbError> {
        let checks = checks.to_vec();
        self.store.read(move |sql| check_releases_in_library_on(&sql, &checks)).await
    }
}

/// A release's new records and the album they file it under: what
/// `set_records_on` writes.
#[cfg(desktop)]
pub struct RecordsChange {
    pub release_id: String,
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
    /// The album the release is in now.
    pub current_album_id: String,
    /// The album the release lands in.
    pub target_album_id: String,
    /// The target album's row, when the release is the first to land in it.
    pub new_album: Option<DbAlbum>,
}

/// Whether the library holds the album `album_id`, as the write `tx` finds
/// it.
#[cfg(desktop)]
pub(super) fn album_held_on(tx: &CovenWrite<'_>, album_id: &str) -> Result<bool, DbError> {
    Ok(tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM albums WHERE id = ?)",
        params![album_id],
        |row| row.get(0),
    )?)
}

/// Write `change`:
///
/// 1. Write the destination album when `new_album` is `Some` and the library
///    does not hold it, with a copy of `current_album_id`'s credit list (so it
///    lands credited as the source was, not a bare row with no artist).
/// 2. Replace the release's records.
/// 3. UPDATE the release's `album_id`; a release that moves is ordered after
///    the destination album's others.
///
/// The source album, emptied by the move, is deleted by the caller's
/// `Emptiable::remove_emptied` at the end of its write, once the rest of the
/// write has had its say.
///
/// Metadata columns (pressing fields, album fields, tracks) are deliberately
/// untouched. Caller decides whether to reseed the metadata.
///
/// Nothing is done to the stored catalog releases: they are keyed by the
/// *catalog's* release, so re-pointing the records at a different one already
/// reads a different row. There is nothing stale to wipe, and the rows this
/// release used may be another candidate's.
#[cfg(desktop)]
pub(super) fn set_records_on(
    tx: &CovenWrite<'_>,
    change: &RecordsChange,
    now_dt: time::UtcDateTime,
    ids: &dyn coven::IdSource,
) -> Result<(), DbError> {
    use bae_util::timestamp::StoredTimestamp as _;

    let now = now_dt.to_rfc3339();
    let RecordsChange { release_id, records, current_album_id, target_album_id, new_album } =
        change;

    // 1. Write the destination album when the library does not hold it yet.
    //    Must come before the release UPDATE so the FK on `releases.album_id`
    //    points at an existing row. The album takes the source's fields and
    //    credits; one the library holds by the time this transaction runs is
    //    left as it is.
    if let Some(album) = new_album {
        if !album_held_on(tx, &album.id)? {
            insert_album_row(tx, album)?;
            let source_artists: Vec<DbAlbumArtist> = tx.query(
                &format!(
                    "SELECT aa.artist_id, aa.position, aa.join_phrase, ac.name_key, ac.name \
                     FROM album_artists aa {credit_name} \
                     WHERE aa.album_id = ? ORDER BY aa.position",
                    credit_name = super::read::credit_name_join("aa", "ac"),
                ),
                params![current_album_id],
                |row| {
                    Ok(DbAlbumArtist {
                        credit: super::read::credit_name_column(row, "name_key", "name")?,
                        join_phrase: row.get("join_phrase")?,
                        ..DbAlbumArtist::new(
                            ids,
                            &album.id,
                            &row.get::<_, String>("artist_id")?,
                            row.get::<_, u32>("position")?,
                            now_dt,
                        )
                    })
                },
            )?;
            replace_album_artists(tx, &album.id, &source_artists, &now)?;
        }
    }

    // 2. Replace the records and the catalog the draft was read from.
    write_release_records(tx, release_id, records)?;

    // 3. Update release: album and where its draft was read. A release
    //    joining another album goes after that album's others.
    if target_album_id != current_album_id {
        tx.execute(
            &format!(
                "UPDATE releases SET album_order = {} WHERE id = ?2",
                next_album_order_sql("?1")
            ),
            params![target_album_id, release_id],
        )?;
    }
    tx.execute(
        "UPDATE releases SET album_id = ? WHERE id = ?",
        params![target_album_id, release_id],
    )?;

    Ok(())
}

/// The library album `records` file a release under, leaving out the
/// releases `excluding`, as [`album_records_name_sql`] names it.
pub fn album_records_name_on(
    sql: &impl coven::SqlRead,
    records: &[bae_catalogs::records::ReleaseRecord],
    excluding: &[String],
) -> Result<Option<String>, DbError> {
    let excluding = serde_json::Value::from(excluding.to_vec()).to_string();
    Ok(sql
        .query_row(
            &album_records_name_sql(RECORDS_OF_JSON, ":excluding"),
            rusqlite::named_params! { ":records": records_json(records), ":excluding": excluding },
            |row| row.get(0),
        )
        .optional()?)
}

/// The album a save files the release `release_id` under once its records
/// are `records`, and the album it is in now, as
/// [`records_target_album_sql`] names them; `None` when the library holds no
/// such release.
pub fn records_target_album_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
    records: &[bae_catalogs::records::ReleaseRecord],
) -> Result<Option<RecordsTargetAlbum>, DbError> {
    Ok(sql
        .query_row(
            &records_target_album_sql(RECORDS_OF_JSON, ":release"),
            rusqlite::named_params! { ":records": records_json(records), ":release": release_id },
            |row| {
                Ok(RecordsTargetAlbum {
                    current_album_id: row.get("current_album_id")?,
                    target_album_id: row.get("target_album_id")?,
                })
            },
        )
        .optional()?)
}

/// The album a release is in and the album its new records file it under,
/// as [`records_target_album_sql`] names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordsTargetAlbum {
    pub current_album_id: String,
    /// The library album the save files the release under; `None` for a new
    /// album, which takes the id of the group the records name
    /// ([`crate::identity::album_id_for_records`]), else a fresh id.
    pub target_album_id: Option<String>,
}

/// The records a JSON array of `:records` holds, as `(position, catalog,
/// kind, key, album_key)`.
pub const RECORDS_OF_JSON: &str = "SELECT key, value ->> 0, value ->> 1, value ->> 2, value ->> 3 \
     FROM json_each(:records)";

/// `records` as the JSON array [`RECORDS_OF_JSON`] reads.
pub fn records_json(records: &[bae_catalogs::records::ReleaseRecord]) -> String {
    use bae_catalogs::records::ReleaseRecord;
    serde_json::Value::from(
        records
            .iter()
            .map(|record| match record {
                ReleaseRecord::Pressing { release, album_key } => {
                    serde_json::json!([
                        release.catalog.as_str(),
                        "pressing",
                        release.key,
                        album_key
                    ])
                }
                ReleaseRecord::Album { album } => {
                    serde_json::json!([album.catalog.as_str(), "album", album.key, null])
                }
            })
            .collect::<Vec<_>>(),
    )
    .to_string()
}

/// The statement that finds the album a save files a release under once its
/// records are the ones `records` selects (as [`album_records_name_sql`]
/// reads them), for the release whose id the SQL expression `release` gives.
/// The statement returns one row, holding the album the release is in now
/// (`current_album_id`) and the library album the save files it under
/// (`target_album_id`), NULL for a new album; no row when the library holds
/// no such release. In order:
///
/// 1. No records: a new album holding only the release.
/// 2. The album the records name ([`album_records_name_sql`], leaving out the
///    release's own records), even when the current album would also fit.
/// 3. With the release alone in its album, the album of the group the
///    records name ([`crate::identity::album_id_for_records`]), new when the
///    library does not hold it; the current album when they name no group or
///    the current album is the group's.
/// 4. The current album when none of its other releases names another album
///    than the records do on a catalog both name.
/// 5. Else the group's album, new when the library does not hold it, or a new
///    album when the records name no group.
pub fn records_target_album_sql(records: &str, release: &str) -> String {
    format!(
        "WITH target_records (position, catalog, kind, key, album_key) AS ({records}), \
         target_album_refs AS ( \
             SELECT position, catalog, \
                    CASE WHEN kind = 'album' THEN key ELSE album_key END AS album_key \
             FROM target_records WHERE kind = 'album' OR album_key IS NOT NULL), \
         current AS (SELECT album_id FROM releases WHERE id = {release}), \
         group_album AS ( \
             SELECT bae_album_id(catalog, album_key) AS id FROM target_album_refs \
             WHERE catalog IN ('musicbrainz', 'discogs') \
             ORDER BY CASE catalog WHEN 'musicbrainz' THEN 0 ELSE 1 END, position LIMIT 1), \
         held_group_album AS ( \
             SELECT a.id FROM group_album JOIN albums a ON a.id = group_album.id), \
         named_album AS ({named}), \
         beside AS ( \
             SELECT EXISTS (SELECT 1 FROM releases r JOIN current ON r.album_id = current.album_id \
                            WHERE r.id <> {release}) AS holds_others, \
                    EXISTS (SELECT 1 FROM releases r JOIN current ON r.album_id = current.album_id \
                            JOIN release_records rr ON rr.release_id = r.id \
                            JOIN target_album_refs new ON rr.catalog = new.catalog \
                            WHERE r.id <> {release} \
                              AND (CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END) \
                                  <> new.album_key) AS disagrees) \
         SELECT current.album_id AS current_album_id, \
                CASE \
                    WHEN NOT EXISTS (SELECT 1 FROM target_records) THEN NULL \
                    WHEN EXISTS (SELECT 1 FROM named_album) THEN (SELECT id FROM named_album) \
                    WHEN NOT beside.holds_others THEN \
                        CASE WHEN NOT EXISTS (SELECT 1 FROM group_album) \
                                  OR (SELECT id FROM group_album) = current.album_id \
                             THEN current.album_id \
                             ELSE (SELECT id FROM held_group_album) END \
                    WHEN NOT beside.disagrees THEN current.album_id \
                    ELSE (SELECT id FROM held_group_album) \
                END AS target_album_id \
         FROM current CROSS JOIN beside",
        named = album_records_name_sql(
            "SELECT position, catalog, kind, key, album_key FROM target_records",
            &format!("json_array({release})"),
        ),
    )
}

/// The statement that finds the library album to file a release under, from
/// the records that `records` selects. It returns one row holding the album's
/// id, or no row for none.
/// `records` selects each record as `(position, catalog, kind, key,
/// album_key)`: `kind` 'pressing' or 'album', `key` the pressing's or the
/// album's, and `album_key` the album a pressing record files it under, NULL
/// for none. `excluding` is an SQL expression for a JSON array of the
/// releases whose records the statement ignores.
///
/// The statement reads the applied record first, which is the record at the
/// lowest `position`, then each other record in order. The first record that
/// names an album the library holds decides. A record names an album through
/// a copy of its pressing, which is a pressing record that names the same
/// release (on `idx_release_records_catalog_key`), or else through its album,
/// which is a record that names the same release group or master (on
/// `idx_release_records_catalog_album`). Library albums are the curator's:
/// two albums the records name stay two, and the applied record's wins over a
/// partner's. Where one record names several albums, the statement picks the
/// record's own group album ([`crate::identity::album_id`]) if the library
/// holds it, and otherwise the oldest album (ties by id), so the answer is one
/// album.
pub fn album_records_name_sql(records: &str, excluding: &str) -> String {
    format!(
        "WITH named_records (position, catalog, kind, key, album_key) AS ({records}), \
         refs AS ( \
             SELECT position, \
                    CASE WHEN kind = 'pressing' THEN catalog END AS pressing_catalog, \
                    CASE WHEN kind = 'pressing' THEN key END AS pressing_key, \
                    CASE WHEN kind = 'album' OR album_key IS NOT NULL THEN catalog END \
                        AS album_catalog, \
                    CASE WHEN kind = 'album' THEN key ELSE album_key END AS album_key \
             FROM named_records), \
         owned AS ( \
             SELECT *, \
                    CASE WHEN album_catalog IN ('musicbrainz', 'discogs') \
                         THEN bae_album_id(album_catalog, album_key) END AS own \
             FROM refs) \
         SELECT a.id FROM ( \
             SELECT o.position, o.own, r.album_id, 0 AS match_rank FROM owned AS o \
             JOIN release_records rr \
                 ON rr.catalog = o.pressing_catalog AND rr.kind = 'pressing' \
                AND rr.key = o.pressing_key \
             JOIN releases r ON r.id = rr.release_id \
             WHERE rr.release_id NOT IN (SELECT value FROM json_each({excluding})) \
             UNION ALL \
             SELECT o.position, o.own, r.album_id, 1 AS match_rank FROM owned AS o \
             JOIN release_records rr \
                 ON rr.catalog = o.album_catalog \
                AND (CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END) \
                    = o.album_key \
             JOIN releases r ON r.id = rr.release_id \
             WHERE rr.release_id NOT IN (SELECT value FROM json_each({excluding})) \
         ) AS named \
         JOIN albums a ON a.id = named.album_id \
         ORDER BY named.position, named.match_rank, a.id IS named.own DESC, a.created_at, a.id \
         LIMIT 1"
    )
}

/// One statement answers every check: the caller passes the checks as one
/// JSON array, and the statement that [`library_status_sql`] builds returns
/// one row for each check, in check order.
fn check_releases_in_library_on(
    sql: &impl coven::SqlRead,
    checks: &[LibraryCheck],
) -> Result<Vec<LibraryStatus>, DbError> {
    if checks.is_empty() {
        return Ok(Vec::new());
    }
    let checks_json = serde_json::Value::Array(
        checks
            .iter()
            .map(|check| {
                serde_json::json!([check.release_id, check.source.as_str(), check.source_group_id])
            })
            .collect(),
    )
    .to_string();
    Ok(sql.query(
        &library_status_sql("SELECT key, value ->> 0, value ->> 1, value ->> 2 FROM json_each(?1)"),
        params![checks_json],
        library_status_of,
    )?)
}

/// The statement that says which library albums hold each release that
/// `checks` selects, as `(position, release_id, catalog, group_id)`: a
/// catalog's release and the album (MusicBrainz release group, Discogs
/// master) that the catalog files the release under, `group_id` NULL for
/// none.
///
/// The statement matches each check by plain equality on the index its kind
/// of record has (its pressing on `idx_release_records_catalog_key`, its
/// album on `idx_release_records_catalog_album`) and returns one row for each
/// check, in `position` order, holding its albums as a JSON array, those
/// holding its pressing first, then by age. [`library_status_of`] reads a
/// row.
pub fn library_status_sql(checks: &str) -> String {
    format!(
        r#"
            WITH checks(position, release_id, catalog, group_id) AS ({checks}),
            matches(position, album_id, holds_release) AS (
                SELECT checks.position, r.album_id, 1
                FROM checks
                JOIN release_records rr
                  ON rr.catalog = checks.catalog
                 AND rr.kind = 'pressing'
                 AND rr.key = checks.release_id
                JOIN releases r ON r.id = rr.release_id
                UNION ALL
                SELECT checks.position, r.album_id, 0
                FROM checks
                JOIN release_records rr
                  ON rr.catalog = checks.catalog
                 AND (CASE WHEN rr.kind = 'album' THEN rr.key ELSE rr.album_key END) = checks.group_id
                JOIN releases r ON r.id = rr.release_id
                WHERE checks.group_id IS NOT NULL
            ),
            found AS MATERIALIZED (
                SELECT matches.position, a.id AS album_id, a.title AS album_title,
                       a.created_at, MAX(matches.holds_release) AS holds_release
                FROM matches
                JOIN albums a ON a.id = matches.album_id
                GROUP BY matches.position, a.id
            )
            SELECT checks.release_id,
                   (SELECT json_group_array(json_object(
                        'album_id', found.album_id,
                        'album_title', found.album_title,
                        'holds_release', found.holds_release)
                        ORDER BY found.holds_release DESC, found.created_at, found.album_id)
                    FROM found WHERE found.position = checks.position) AS albums
            FROM checks
            ORDER BY checks.position
        "#
    )
}

/// The SQL of a condition that holds when the library holds the release
/// `key` of `catalog` as a pressing: a release whose records name that
/// catalog's release as the pressing it is, matched on
/// `idx_release_records_catalog_key`. Deleting a release deletes its records,
/// so the library does not hold a deleted release.
pub fn pressing_held_sql(catalog: &str, key: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM release_records AS held \
                 WHERE held.catalog = {catalog} AND held.kind = 'pressing' \
                   AND held.key = {key})"
    )
}

/// The library status one row of [`library_status_sql`] holds.
pub fn library_status_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryStatus> {
    let albums: Vec<StoredAlbumMatch> = bae_store::read::json_column(row, "albums")?;
    Ok(LibraryStatus {
        release_id: row.get("release_id")?,
        albums: albums
            .into_iter()
            .map(|album| crate::LibraryAlbumMatch {
                album_id: album.album_id,
                album_title: album.album_title,
                holds_release: album.holds_release != 0,
            })
            .collect(),
    })
}

/// One album match as the library check's statement writes it into its
/// `albums` array: `holds_release` is 1 for an album that holds the release
/// itself and 0 for one that holds only another release of its album.
#[derive(serde::Deserialize)]
struct StoredAlbumMatch {
    album_id: String,
    album_title: String,
    holds_release: i64,
}

#[cfg(test)]
#[path = "release_records_tests.rs"]
mod tests;
