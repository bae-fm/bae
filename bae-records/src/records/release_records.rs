//! Reading and writing a release's records — every catalog's description of
//! it.

use super::fragments::library_status_of_columns;
use super::*;
#[cfg(desktop)]
use bae_store::WriteStatements as _;
use bae_store::{statement_optional, statement_rows};

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
    /// releases `excluding`, as
    /// [`AlbumRecordsName`](super::fragments::AlbumRecordsName) names it.
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
    /// [`FiledByRecords`](super::fragments::FiledByRecords) names them;
    /// `None` when the library holds no such release.
    #[cfg(any(test, feature = "test-utils"))]
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
    /// read whether the library holds them in their own statement.
    ///
    /// [`LibraryStatusRows`](super::fragments::LibraryStatusRows) says which
    /// albums each check finds.
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

// Whether the library holds the album `album`.
#[cfg(desktop)]
coven::read!(
    writer AlbumHeld(album: String) { held: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM albums AS album WHERE album.id = :album) AS held
"#
);

// The album `album`'s credits in list order, each with the other printed
// name it names, if any.
#[cfg(desktop)]
coven::read!(
    writer AlbumCreditRows(album: String) { position: u32 },
    r#"
    SELECT credit.artist_id, credit.position, credit.join_phrase,
           credit_name.name_key, credit_name.name
    FROM album_artists AS credit
    LEFT JOIN artist_credits AS credit_name
           ON credit_name.artist_id = credit.artist_id
          AND credit_name.name_key = credit.credit_key
    WHERE credit.album_id = :album
    ORDER BY credit.position
"#
);

/// Whether the library holds the album `album_id`, as the write `tx` finds
/// it.
#[cfg(desktop)]
pub(super) fn album_held_on(tx: &CovenWrite<'_>, album_id: &str) -> Result<bool, DbError> {
    let statement = AlbumHeldParams { album: album_id.to_owned() };
    Ok(bae_store::statement_row(tx, &statement)?.held)
}

// Order the release `release_id` last among those of the album `album_id`.
#[cfg(desktop)]
coven::write!(
    OrderReleaseLast(release_id: &'p str, album_id: String),
    r#"
    UPDATE releases
    SET album_order = @crate::records::write::NextAlbumOrder(album: :album_id)
    WHERE id = :release_id
"#
);

#[cfg(desktop)]
coven::write!(
    SetReleaseAlbum(release_id: &'p str, album_id: &'p str),
    "UPDATE releases SET album_id = :album_id WHERE id = :release_id"
);

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
            let statement = AlbumCreditRowsParams { album: current_album_id.clone() };
            let source_artists: Vec<DbAlbumArtist> = bae_store::statement_rows(tx, &statement)?
                .into_iter()
                .map(|row| {
                    let credit = super::read::joined_credit_name(row.name_key, row.name)
                        .map_err(|reason| DbError::unreadable("an album credit's name", reason))?;
                    Ok(DbAlbumArtist {
                        credit,
                        join_phrase: row.join_phrase,
                        ..DbAlbumArtist::new(ids, &album.id, &row.artist_id, row.position, now_dt)
                    })
                })
                .collect::<Result<_, DbError>>()?;
            replace_album_artists(tx, &album.id, &source_artists, &now)?;
        }
    }

    // 2. Replace the records and the catalog the draft was read from.
    write_release_records(tx, release_id, records)?;

    // 3. Update release: album and where its draft was read. A release
    //    joining another album goes after that album's others.
    if target_album_id != current_album_id {
        tx.run(&OrderReleaseLastParams { release_id, album_id: target_album_id.clone() })?;
    }
    tx.run(&SetReleaseAlbumParams { release_id, album_id: target_album_id })?;

    Ok(())
}

/// The library album `records` file a release under, leaving out the
/// releases `excluding`, as
/// [`AlbumRecordsName`](super::fragments::AlbumRecordsName) names it.
pub fn album_records_name_on(
    sql: &impl bae_store::WriterRead,
    records: &[bae_catalogs::records::ReleaseRecord],
    excluding: &[String],
) -> Result<Option<String>, DbError> {
    let named = statement_optional(
        sql,
        &AlbumRecordsNameOfJsonParams {
            records: records_json(records),
            excluding: serde_json::Value::from(excluding.to_vec()).to_string(),
        },
    )?;
    Ok(named.map(|row| row.named.album_id))
}

// [`AlbumRecordsName`](super::fragments::AlbumRecordsName) over the records of
// the JSON array `records` ([`records_json`]), leaving out the releases of the
// JSON array `excluding`.
coven::read!(
    writer AlbumRecordsNameOfJson(records: String, excluding: String),
    r#"
    SELECT named.*
    FROM @crate::records::fragments::AlbumRecordsName(
        records: (SELECT record.key, record.value ->> 0, record.value ->> 1,
                         record.value ->> 2, record.value ->> 3
                  FROM json_each(:records) AS record),
        excluding: (SELECT left_out.value FROM json_each(:excluding) AS left_out)
    ) AS named
"#
);

// [`LibraryStatusRows`](super::fragments::LibraryStatusRows) over the checks
// of the JSON array `checks`, each a `[release_id, catalog, group_id]`, in
// its order.
coven::read!(
    writer LibraryStatusOfJson(checks: String),
    r#"
    SELECT status.*
    FROM @crate::records::fragments::LibraryStatusRows(checks: (
        SELECT asked.key, asked.value ->> 0, asked.value ->> 1, asked.value ->> 2
        FROM json_each(:checks) AS asked
    )) AS status
    ORDER BY status.position
"#
);

/// The album a save files the release `release_id` under once its records
/// are `records`, and the album it is in now, as
/// [`FiledByRecords`](super::fragments::FiledByRecords) names them; `None`
/// when the library holds no such release.
#[cfg(any(test, feature = "test-utils"))]
pub fn records_target_album_on(
    sql: &impl bae_store::WriterRead,
    release_id: &str,
    records: &[bae_catalogs::records::ReleaseRecord],
) -> Result<Option<RecordsTargetAlbum>, DbError> {
    let filed = statement_optional(
        sql,
        &RecordsTargetAlbumOfJsonParams {
            release: release_id.to_string(),
            records: records_json(records),
        },
    )?;
    Ok(filed.map(|row| RecordsTargetAlbum {
        current_album_id: row.filed.current_album_id,
        target_album_id: row.filed.target_album_id,
    }))
}

// [`FiledByRecords`](super::fragments::FiledByRecords) over the records of
// the JSON array `records` ([`records_json`]).
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer RecordsTargetAlbumOfJson(release: String, records: String),
    r#"
    SELECT filed.*
    FROM @crate::records::fragments::FiledByRecords(
        release: :release,
        records: (SELECT record.key, record.value ->> 0, record.value ->> 1,
                         record.value ->> 2, record.value ->> 3
                  FROM json_each(:records) AS record)
    ) AS filed
"#
);

/// The album a release is in and the album its new records file it under,
/// as [`FiledByRecords`](super::fragments::FiledByRecords) names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordsTargetAlbum {
    pub current_album_id: String,
    /// The library album the save files the release under; `None` for a new
    /// album, which takes the id of the group the records name
    /// ([`crate::identity::album_id_for_records`]), else a fresh id.
    pub target_album_id: Option<String>,
}

/// `records` as a JSON array of `[catalog, kind, key, album_key]`, in their
/// order, which the reads over a request's records bind.
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

/// One statement answers every check: the caller passes the checks as one
/// JSON array, and [`LibraryStatusOfJson`] returns one row for each check, in
/// check order.
fn check_releases_in_library_on(
    sql: &impl bae_store::WriterRead,
    checks: &[LibraryCheck],
) -> Result<Vec<LibraryStatus>, DbError> {
    if checks.is_empty() {
        return Ok(Vec::new());
    }
    let checks = serde_json::Value::Array(
        checks
            .iter()
            .map(|check| {
                serde_json::json!([check.release_id, check.source.as_str(), check.source_group_id])
            })
            .collect(),
    )
    .to_string();
    statement_rows(sql, &LibraryStatusOfJsonParams { checks })?
        .into_iter()
        .map(|row| library_status_of_columns(row.status.release_id, &row.status.albums))
        .collect()
}

/// The library status of `release_id` whose `albums` the check's statement
/// wrote as its JSON array.
pub(super) fn library_status_from(
    release_id: String,
    albums: Vec<StoredAlbumMatch>,
) -> LibraryStatus {
    LibraryStatus {
        release_id,
        albums: albums
            .into_iter()
            .map(|album| crate::LibraryAlbumMatch {
                album_id: album.album_id,
                album_title: album.album_title,
                holds_release: album.holds_release != 0,
            })
            .collect(),
    }
}

/// One album match as the library check's statement writes it into its
/// `albums` array: `holds_release` is 1 for an album that holds the release
/// itself and 0 for one that holds only another release of its album.
#[derive(serde::Deserialize)]
pub(super) struct StoredAlbumMatch {
    album_id: String,
    album_title: String,
    holds_release: i64,
}

#[cfg(test)]
#[path = "release_records_tests.rs"]
mod tests;
