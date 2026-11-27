//! The downloads list's reads: its window, how many entries it has, how many
//! come before an entry, and the key of the entry at a position. The list's
//! order is the order queued, and an entry's key is its place in it
//! ([`DownloadQueuePlace`]).

use crate::download_snapshot::DownloadRow;
use bae_store::list_windows::{sql_bound, OrderKey, SortValue};
use bae_store::{
    statement_optional, statement_row, statement_rows, DbError, ListWindow, WindowRead,
};
use coven::SqlReadContext;

// The downloads list's order: each entry's place, in the order queued.
sqlive_compose::fragment!(
    DownloadQueuePlace,
    r#"
    SELECT kept.position
    FROM download_queue AS kept
"#
);

// The entries at or after the place `from`, at most `limit` of them, each
// beside its album's title. Coven narrows a full window to the writes at or
// before its last row only when the statement returns its order's column
// ahead of any `*`, which no column of a spread can be: the order is read from
// the table.
sqlive_compose::read!(
    writer DownloadWindow(from: i64, limit: i64),
    r#"
    SELECT queued.position, entry.*, release.*, album.*
    FROM download_queue AS queued
    JOIN @crate::queued_releases::rows::DownloadQueueRow AS entry
      ON entry.position = queued.position
    LEFT JOIN @bae_records::records::fragments::ReleaseAlbum AS release
           ON release.release_id = entry.release_id
    LEFT JOIN @bae_records::records::fragments::AlbumTitle AS album
           ON album.album_id = release.album_id
    WHERE queued.position >= :from
    ORDER BY queued.position
    LIMIT :limit
"#
);

// How many entries the downloads list has.
sqlive_compose::read!(
    writer DownloadListLength { entries: u64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM @crate::queued_releases::download_list::DownloadQueuePlace AS entry
"#
);

// How many entries come before the place `position`.
sqlive_compose::read!(
    writer DownloadsBefore(position: i64) { entries: u64 },
    r#"
    SELECT COUNT(*) AS entries
    FROM @crate::queued_releases::download_list::DownloadQueuePlace AS entry
    WHERE entry.position < :position
"#
);

// The place of the entry at `index`, counted from the list's top, or of the
// last entry when the list ends before it. SQLite reads the offset -1 of an
// empty list as 0, where there is no row.
sqlive_compose::read!(
    writer DownloadPlaceAt(index: i64),
    r#"
    SELECT entry.*
    FROM @crate::queued_releases::download_list::DownloadQueuePlace AS entry
    ORDER BY entry.position
    LIMIT 1 OFFSET min(
        :index,
        (SELECT COUNT(*)
         FROM @crate::queued_releases::download_list::DownloadQueuePlace AS counted) - 1)
"#
);

/// The key of the entry at the place `position`.
fn key_of(position: i64) -> OrderKey {
    OrderKey::new(vec![SortValue::Integer(position)])
}

/// The place `key` names. Only [`key_of`] makes the downloads list's keys.
fn place_of(key: &OrderKey) -> Result<i64, DbError> {
    match key.values() {
        [SortValue::Integer(position)] => Ok(*position),
        values => Err(DbError::unreadable("a download list key", values)),
    }
}

/// The downloads list's `window`: its entries from the window's key on, or
/// from the top, each with its key.
pub fn download_window_on(
    sql: &SqlReadContext<'_>,
    window: &ListWindow,
) -> Result<WindowRead<DownloadRow>, DbError> {
    let from = window.from.as_ref().map(place_of).transpose()?.unwrap_or(i64::MIN);
    let rows = statement_rows(sql, &DownloadWindowParams { from, limit: sql_bound(window.limit) })?
        .into_iter()
        .map(|row| {
            let key = key_of(row.position);
            Ok((row.entry.into_row(row.release, row.album)?, key))
        })
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// How many entries the downloads list has.
pub(crate) fn download_list_length_on(sql: &SqlReadContext<'_>) -> Result<u64, DbError> {
    Ok(statement_row(sql, &DownloadListLengthParams {})?.entries)
}

/// How many entries of the downloads list come before the entry at `key`.
pub fn downloads_before_on(sql: &SqlReadContext<'_>, key: &OrderKey) -> Result<u64, DbError> {
    Ok(statement_row(sql, &DownloadsBeforeParams { position: place_of(key)? })?.entries)
}

/// The key of the entry at `index`, or of the last one when the list ends
/// before it; `None` for an empty list.
pub fn download_key_at_on(
    sql: &SqlReadContext<'_>,
    index: u64,
) -> Result<Option<OrderKey>, DbError> {
    Ok(statement_optional(sql, &DownloadPlaceAtParams { index: sql_bound(index) })?
        .map(|row| key_of(row.entry.position)))
}

#[cfg(test)]
#[path = "download_list_tests.rs"]
mod tests;
