//! The queue screens' reads of the playback queue tables: the queue value,
//! with Up Next whole and what the context lane plays from, and the last
//! addition to Up Next. The context lane's upcoming tracks are read as a list
//! (`queue_upcoming`).

use super::track_display::{track_display_columns, track_queue_meta_from_row, TRACK_DISPLAY_JOINS};
use super::*;
use crate::{ContextSource, QueueItem};

/// Follow the queue as its screens show it: Up Next whole, what the context
/// lane plays from, and whether the transport can step either way.
pub fn subscribe_queue_value(
    store: &Store,
) -> coven::LiveQuery<crate::ResolvedQueueSnapshot, bae_store::DbError> {
    store.subscribe(|sql| queue_value_on(&sql))
}

/// Follow the queue's last addition to Up Next; `None` until there is one
/// since playback started with the library.
pub fn subscribe_queue_addition(
    store: &Store,
) -> coven::LiveQuery<Option<crate::QueueAddition>, bae_store::DbError> {
    store.subscribe(|sql| queue_addition_on(&sql))
}

/// The queue's last addition to Up Next, as `subscribe_queue_addition` reads
/// it.
#[cfg(any(test, feature = "test-utils"))]
pub async fn queue_addition(store: &Store) -> Result<Option<crate::QueueAddition>, DbError> {
    store.read(|sql| queue_addition_on(&sql)).await
}

/// The queue as its screens show it now.
pub async fn queue_value(store: &Store) -> Result<crate::ResolvedQueueSnapshot, DbError> {
    store.read(|sql| queue_value_on(&sql)).await
}

/// The statement that reads the rows `rows` picks — a `SELECT entry,
/// track_id AS lane_track, position …` in lane order, already cut to the page
/// asked for — each with its track as the queue rows show it. The page is cut
/// before the join, so the rows an offset passes over are never joined to
/// their tracks. Every column `rows` picks comes through beside the track's.
pub(super) fn queue_items_sql(rows: &str) -> String {
    format!(
        "SELECT e.*, {columns} \
         FROM ({rows}) e \
         JOIN tracks t ON t.id = e.lane_track \
         {joins} \
         ORDER BY e.position",
        columns = track_display_columns(),
        joins = TRACK_DISPLAY_JOINS,
    )
}

/// One queue row as [`queue_items_sql`] reads it.
pub(super) fn queue_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
    let meta = track_queue_meta_from_row(row)?;
    Ok(QueueItem {
        entry_id: row.get("entry")?,
        track_id: row.get("track_id")?,
        display: meta.display,
        duration_ms: meta.duration_ms,
    })
}

fn queue_items_on(sql: &SqlReadContext<'_>, rows: &str) -> Result<Vec<QueueItem>, DbError> {
    Ok(sql.query(&queue_items_sql(rows), [], queue_item_from_row)?)
}

/// The `playback_queue` row as the queue screens read it.
struct QueueHead {
    shuffled: bool,
    context: Option<(ContextSource, i64)>,
}

fn queue_head_on(sql: &SqlReadContext<'_>) -> Result<QueueHead, DbError> {
    type Head = (bool, Option<String>, Option<String>, Option<i64>);
    let (shuffled, source_kind, source_release_id, cursor): Head = sql.query_row(
        "SELECT shuffled, source_kind, source_release_id, \
                CASE shuffled WHEN 1 THEN shuffled_cursor ELSE ordered_cursor END \
         FROM playback_queue",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    let context = match (source_kind, cursor) {
        (Some(source_kind), Some(cursor)) => Some((
            ContextSource::from_columns(&source_kind, source_release_id.clone()).map_err(
                |_unwritten| {
                    DbError::unreadable(
                        "playback_queue.source_kind and source_release_id",
                        (&source_kind, &source_release_id),
                    )
                },
            )?,
            cursor,
        )),
        // The table's CHECK keeps the two NULL together.
        _ => None,
    };
    Ok(QueueHead { shuffled, context })
}

fn queue_addition_on(sql: &SqlReadContext<'_>) -> Result<Option<crate::QueueAddition>, DbError> {
    let (number, added_tracks): (u64, Option<u32>) =
        sql.query_row("SELECT additions, added_tracks FROM playback_queue", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    // The table's CHECK sets the count exactly when there was an addition.
    Ok(added_tracks.map(|tracks| crate::QueueAddition { number, tracks }))
}

fn queue_value_on(sql: &SqlReadContext<'_>) -> Result<crate::ResolvedQueueSnapshot, DbError> {
    let head = queue_head_on(sql)?;
    // The entry ids are `QueueEntryId`'s text: 'm' for Up Next or 'c' for the
    // context lane, then the entry number.
    let manual = queue_items_on(
        sql,
        "SELECT 'm' || entry AS entry, track_id AS lane_track, position FROM playback_manual \
         ORDER BY position",
    )?;
    // The context lane's playing place is its cursor in the order it reads
    // in; with no cursor nothing comes before or after it.
    let (has_next, has_previous): (bool, bool) = sql.query_row(
        "SELECT q.repeat <> 'off' \
                OR EXISTS (SELECT 1 FROM playback_manual) \
                OR EXISTS (SELECT 1 FROM playback_context c \
                           WHERE CASE q.shuffled WHEN 1 THEN c.shuffled ELSE c.ordered END \
                               > CASE q.shuffled WHEN 1 THEN q.shuffled_cursor \
                                                 ELSE q.ordered_cursor END), \
                EXISTS (SELECT 1 FROM playback_context c \
                        WHERE CASE q.shuffled WHEN 1 THEN c.shuffled ELSE c.ordered END \
                            < CASE q.shuffled WHEN 1 THEN q.shuffled_cursor \
                                              ELSE q.ordered_cursor END) \
         FROM playback_queue q",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let context = match head.context {
        None => None,
        Some((source, _cursor)) => {
            let source_title = match &source {
                ContextSource::Release(release_id) => sql
                    .query_row(
                        "SELECT a.title FROM releases r JOIN albums a ON a.id = r.album_id \
                         WHERE r.id = ?",
                        [release_id],
                        |row| row.get(0),
                    )
                    .optional()?,
                ContextSource::Releases | ContextSource::Tracks | ContextSource::Library => None,
            };
            Some(crate::ResolvedContext { source, source_title })
        }
    };
    Ok(crate::ResolvedQueueSnapshot {
        manual,
        context,
        shuffled: head.shuffled,
        has_next,
        has_previous,
    })
}

#[cfg(test)]
#[path = "queue_reads_tests.rs"]
mod tests;
