//! The queue screens' reads of the playback queue tables: the queue value,
//! with Up Next whole and what the context lane plays from, the windows of
//! the context lane's upcoming tracks with their length, and the last
//! addition to Up Next.

use super::queue::context_order_column;
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

/// Every one of the context lane's tracks after the playing one, as the queue
/// rows show them.
#[cfg(any(test, feature = "test-utils"))]
pub async fn queue_upcoming(store: &Store) -> Result<Vec<QueueItem>, DbError> {
    let all = crate::QueueWindows::from([crate::QueueWindow::at(0, u64::MAX)]);
    let read = store.read(move |sql| queue_upcoming_on(&sql, &all)).await?;
    Ok(read.windows.into_iter().flat_map(|window| window.rows).collect())
}

/// The context lane's version, as the upcoming windows' reads carry it.
#[cfg(any(test, feature = "test-utils"))]
pub async fn queue_context_version(store: &Store) -> Result<u64, DbError> {
    store.read(|sql| Ok(queue_head_on(&sql)?.context_version)).await
}

/// Follow the windows `initial` names of the context lane's tracks after the
/// playing one. The windows change by pointing the same query at new ones
/// through its request handle.
pub fn subscribe_queue_upcoming(
    store: &Store,
    initial: crate::QueueWindows,
) -> coven::ReconfigurableLiveQuery<crate::QueueWindows, QueueUpcomingRead, bae_store::DbError> {
    store
        .subscribe_reconfigurable(initial, |windows, sql| queue_upcoming_on(&sql, windows))
        .process(Ok)
}

/// The rows `rows` picks — a `SELECT entry, track_id, position …` in lane
/// order, already cut to the page asked for — each with its track as the
/// queue rows show it. The page is cut before the join, so the rows an
/// offset passes over are never joined to their tracks.
fn queue_items_on(
    sql: &SqlReadContext<'_>,
    rows: &str,
    params: impl rusqlite::Params,
) -> Result<Vec<QueueItem>, DbError> {
    Ok(sql.query(
        &format!(
            "SELECT e.entry, {columns} \
             FROM ({rows}) e \
             JOIN tracks t ON t.id = e.track_id \
             {joins} \
             ORDER BY e.position",
            columns = track_display_columns(),
            joins = TRACK_DISPLAY_JOINS,
        ),
        params,
        |row| {
            let meta = track_queue_meta_from_row(row)?;
            Ok(QueueItem {
                entry_id: row.get("entry")?,
                track_id: row.get("track_id")?,
                display: meta.display,
                duration_ms: meta.duration_ms,
            })
        },
    )?)
}

/// The `playback_queue` row as the queue screens read it.
struct QueueHead {
    shuffled: bool,
    context: Option<(ContextSource, i64)>,
    manual_version: u64,
    context_version: u64,
}

fn queue_head_on(sql: &SqlReadContext<'_>) -> Result<QueueHead, DbError> {
    type Head = (bool, Option<String>, Option<String>, Option<i64>, i64, i64);
    let (shuffled, source_kind, source_release_id, cursor, manual_version, context_version): Head =
        sql.query_row(
            "SELECT shuffled, source_kind, source_release_id, \
                    CASE shuffled WHEN 1 THEN shuffled_cursor ELSE ordered_cursor END, \
                    manual_version, context_version \
             FROM playback_queue",
            [],
            |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
            },
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
    // The table's CHECKs keep both versions at zero or above.
    Ok(QueueHead {
        shuffled,
        context,
        manual_version: manual_version as u64,
        context_version: context_version as u64,
    })
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
        "SELECT 'm' || entry AS entry, track_id, position FROM playback_manual \
         ORDER BY position",
        [],
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
        manual_version: head.manual_version,
    })
}

fn queue_upcoming_on(
    sql: &SqlReadContext<'_>,
    windows: &crate::QueueWindows,
) -> Result<QueueUpcomingRead, DbError> {
    let head = queue_head_on(sql)?;
    let windows = windows
        .iter()
        .map(|window| {
            let items = match head.context {
                None => Vec::new(),
                Some((_, cursor)) => queue_items_on(
                    sql,
                    &format!(
                        "SELECT 'c' || entry AS entry, track_id, {column} AS position \
                         FROM playback_context \
                         WHERE {column} > ?1 ORDER BY {column} LIMIT ?2 OFFSET ?3",
                        column = context_order_column(head.shuffled)
                    ),
                    params![
                        cursor,
                        i64::try_from(window.limit).unwrap_or(i64::MAX),
                        i64::try_from(window.offset).unwrap_or(i64::MAX)
                    ],
                )?,
            };
            Ok(crate::QueueWindowRead { window: *window, rows: items })
        })
        .collect::<Result<_, DbError>>()?;
    Ok(QueueUpcomingRead {
        context_version: head.context_version,
        upcoming_total: match head.context {
            None => 0,
            Some((_, cursor)) => upcoming_total_on(sql, head.shuffled, cursor)?,
        },
        windows,
    })
}

/// How many of the context lane's tracks come after its playing one in the
/// order it reads in.
fn upcoming_total_on(
    sql: &SqlReadContext<'_>,
    shuffled: bool,
    cursor: i64,
) -> Result<u64, DbError> {
    let total: i64 = sql.query_row(
        &format!(
            "SELECT count(*) FROM playback_context WHERE {} > ?",
            context_order_column(shuffled)
        ),
        [cursor],
        |row| row.get(0),
    )?;
    Ok(total as u64)
}

/// One read of the context lane's upcoming windows: the lane's version, the
/// tail's length then, and each window's entries.
#[derive(Debug, Clone, PartialEq)]
pub struct QueueUpcomingRead {
    /// How many writes have changed the context lane's tracks after its
    /// playing one, as the queue row counts them: the same windows read at
    /// another version may hold other rows.
    pub context_version: u64,
    pub upcoming_total: u64,
    pub windows: Vec<crate::QueueWindowRead<QueueItem>>,
}

#[cfg(test)]
#[path = "queue_reads_tests.rs"]
mod tests;
