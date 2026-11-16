//! The queue screens' reads of the playback queue tables: the queue value,
//! with Up Next whole and what the context lane plays from, and the head of
//! what plays next as a system media session lists it. The `queue_upcoming`
//! module reads the context lane's upcoming tracks as a list for the queue
//! screens.

use super::queue::context_order_column;
use super::queue_upcoming::after_the_playing_track_in_order;
use super::track_display::{track_display_columns, track_display_from_row, TRACK_DISPLAY_JOINS};
use super::*;
use crate::{ContextSource, QueueItem};

/// Follow the queue as its screens show it: Up Next whole, what the context
/// lane plays from, and whether the transport can step either way.
pub fn subscribe_queue_value(
    store: &Store,
) -> coven::LiveQuery<crate::ResolvedQueueSnapshot, bae_store::DbError> {
    store.subscribe(|sql| queue_value_on(&sql))
}

/// The queue as its screens show it now.
pub async fn queue_value(store: &Store) -> Result<crate::ResolvedQueueSnapshot, DbError> {
    store.read(|sql| queue_value_on(&sql)).await
}

/// Follow the tracks after the playing one as a system media session lists
/// them: the Up Next tracks, then the context lane's tracks after the playing
/// one in the lane's current order, at most `limit` in all. The statement
/// reads each lane through the index of its order, so coven reruns it for a
/// write among the rows it read.
pub fn subscribe_session_up_next(
    store: &Store,
    limit: u64,
) -> coven::LiveQuery<Vec<QueueItem>, bae_store::DbError> {
    store.subscribe(move |sql| session_up_next_on(&sql, limit))
}

/// The rows of the session's up-next list, at most `?1`: the Up Next rows,
/// then the lane's rows after the playing track in the queue's current
/// order. Each part reads at most `?1` rows through the index of its order,
/// from where the part starts. The statement has one lane part for each
/// order, and [`after_the_playing_track_in_order`] keeps the rows of only the
/// queue's current one. The statement joins the parts in order, keeps the
/// first `?1`, and numbers them so that the track join keeps their order.
pub(super) fn session_up_next_rows() -> String {
    let lane = |shuffled: bool| {
        format!(
            "SELECT * FROM ( \
                 SELECT 'c' || c.entry, c.track_id, 1, c.{place} FROM playback_context c \
                 WHERE {after} \
                 ORDER BY c.{place} LIMIT ?1)",
            place = context_order_column(shuffled),
            after = after_the_playing_track_in_order(shuffled),
        )
    };
    format!(
        "SELECT entry, lane_track, ROW_NUMBER() OVER (ORDER BY lane, place) AS position \
         FROM ( \
             SELECT * FROM ( \
                 SELECT 'm' || entry AS entry, track_id AS lane_track, 0 AS lane, \
                        position AS place \
                 FROM playback_manual ORDER BY position LIMIT ?1) \
             UNION ALL {ordered} \
             UNION ALL {shuffled} \
             ORDER BY lane, place LIMIT ?1)",
        ordered = lane(false),
        shuffled = lane(true),
    )
}

fn session_up_next_on(sql: &SqlReadContext<'_>, limit: u64) -> Result<Vec<QueueItem>, DbError> {
    Ok(sql.query(
        &queue_items_sql(&session_up_next_rows()),
        [i64::try_from(limit).unwrap_or(i64::MAX)],
        queue_item_from_row,
    )?)
}

/// The statement that reads the rows `rows` picks — a `SELECT entry,
/// track_id AS lane_track, position …` in lane order, already cut to the page
/// asked for — each with its track as the queue rows show it. The `rows`
/// query applies its LIMIT and OFFSET before this statement joins the tracks,
/// so SQLite never joins a skipped row to its track. The statement returns
/// every column `rows` selects, then the track's columns.
pub(super) fn queue_items_sql(rows: &str) -> String {
    format!(
        "SELECT e.*, t.id AS track_id, t.duration_ms, {columns} \
         FROM ({rows}) e \
         JOIN tracks t ON t.id = e.lane_track \
         {joins} \
         ORDER BY e.position",
        columns = track_display_columns(""),
        joins = TRACK_DISPLAY_JOINS,
    )
}

/// One queue row as [`queue_items_sql`] reads it.
pub(super) fn queue_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueueItem> {
    Ok(QueueItem {
        entry_id: row.get("entry")?,
        track_id: row.get("track_id")?,
        display: track_display_from_row(row, "")?,
        duration_ms: row.get("duration_ms")?,
    })
}

fn queue_items_on(sql: &SqlReadContext<'_>, rows: &str) -> Result<Vec<QueueItem>, DbError> {
    Ok(sql.query(&queue_items_sql(rows), [], queue_item_from_row)?)
}

/// The `playback_queue` row as the queue screens read it.
struct QueueHead {
    shuffled: bool,
    context: Option<crate::ResolvedContext>,
}

fn queue_head_on(sql: &SqlReadContext<'_>) -> Result<QueueHead, DbError> {
    type Head = (bool, Option<String>, Option<String>, Option<String>);
    let (shuffled, source_kind, source_release_id, source_title): Head = sql.query_row(
        "SELECT q.shuffled, q.source_kind, q.source_release_id, a.title \
         FROM playback_queue q \
         LEFT JOIN releases r ON r.id = q.source_release_id \
         LEFT JOIN albums a ON a.id = r.album_id",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    // The table's CHECK keeps `source_kind` NULL exactly when the queue has
    // no cursor, so a queue with a source kind has a context.
    let context = match source_kind {
        Some(source_kind) => Some(crate::ResolvedContext {
            source: ContextSource::from_columns(&source_kind, source_release_id.clone()).map_err(
                |_unwritten| {
                    DbError::unreadable(
                        "playback_queue.source_kind and source_release_id",
                        (&source_kind, &source_release_id),
                    )
                },
            )?,
            source_title,
        }),
        None => None,
    };
    Ok(QueueHead { shuffled, context })
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
    // The playing track is the one at the lane's cursor, in shuffled order
    // when shuffle is on and in list order otherwise; with no cursor, no
    // track comes before or after it. `follows` is true when a track plays
    // after the playing track: a track in Up Next, or a track in the context
    // lane past its cursor.
    // `has_next` is true when `follows` is true or repeat is on, and
    // `is_empty` is true when `follows` is false.
    let place = "CASE q.shuffled WHEN 1 THEN c.shuffled ELSE c.ordered END";
    let cursor = "CASE q.shuffled WHEN 1 THEN q.shuffled_cursor ELSE q.ordered_cursor END";
    let (has_next, has_previous, up_next_count, is_empty): (bool, bool, u32, bool) = sql
        .query_row(
            &format!(
                "SELECT repeat <> 'off' OR follows, before, up_next_count, NOT follows \
                 FROM (SELECT q.repeat, \
                              EXISTS (SELECT 1 FROM playback_manual) \
                              OR EXISTS (SELECT 1 FROM playback_context c \
                                         WHERE {place} > {cursor}) AS follows, \
                              EXISTS (SELECT 1 FROM playback_context c \
                                      WHERE {place} < {cursor}) AS before, \
                              (SELECT COUNT(*) FROM playback_manual) AS up_next_count \
                       FROM playback_queue q)"
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    Ok(crate::ResolvedQueueSnapshot {
        manual,
        context: head.context,
        shuffled: head.shuffled,
        has_next,
        has_previous,
        up_next_count,
        is_empty,
    })
}

#[cfg(test)]
#[path = "queue_reads_tests.rs"]
mod tests;
