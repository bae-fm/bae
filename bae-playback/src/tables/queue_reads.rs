//! The queue screens' reads of the playback queue tables: the queue value,
//! with Up Next whole and what the context lane plays from, and the head of
//! what plays next as a system media session lists it, with the fragments
//! they are composed of. The `queue_upcoming` module reads the context lane's
//! upcoming tracks as a list for the queue screens.

use super::*;
use crate::{ContextSource, QueueItem, RepeatMode};
use bae_records::records::track_display::DisplayedTrack;

// How the queue repeats.
sqlive_compose::fragment!(
    QueueRepeat { repeat: crate::RepeatMode },
    r#"
    SELECT queue.repeat
    FROM playback_queue AS queue
"#
);

impl QueueRepeat {
    /// How the queue repeats.
    pub(super) fn into_mode(self) -> RepeatMode {
        self.repeat
    }
}

// The queue's head: whether shuffle is on, and what the context lane plays
// from, with the title of its album when it plays one release.
sqlive_compose::fragment!(
    QueueHead { shuffled: bool },
    r#"
    SELECT queue.shuffled, queue.source_kind, queue.source_release_id,
           source_release.*, source_album.*
    FROM playback_queue AS queue
    LEFT JOIN @bae_records::records::fragments::ReleaseAlbum AS source_release
           ON source_release.release_id = queue.source_release_id
    LEFT JOIN @bae_records::records::fragments::AlbumTitle AS source_album
           ON source_album.album_id = source_release.album_id
"#
);

impl QueueHead {
    /// Whether shuffle is on, and what the context lane plays from. The
    /// table's CHECK keeps `source_kind` NULL exactly when the queue has no
    /// cursor, so a queue with a source kind has a context.
    fn into_parts(self) -> Result<(bool, Option<crate::ResolvedContext>), DbError> {
        let release_id = self.source_release_id;
        let context = match self.source_kind {
            Some(kind) => Some(crate::ResolvedContext {
                source: ContextSource::from_columns(&kind, release_id.clone()).map_err(
                    |_unwritten| {
                        DbError::unreadable(
                            "playback_queue.source_kind and source_release_id",
                            (&kind, &release_id),
                        )
                    },
                )?,
                source_title: self.source_album.map(|album| album.into_title()),
            }),
            None => None,
        };
        Ok((self.shuffled, context))
    }
}

// Whether the transport can step either way, how many entries Up Next holds,
// and whether nothing is to play after the playing track. The playing track
// is the one at the lane's cursor, in shuffled order when shuffle is on and in
// list order otherwise; with no cursor, no track comes before or after it.
// `follows` is true when a track plays after the playing track: a track in Up
// Next, or a track in the context lane past its cursor. `has_next` is true
// when `follows` is true or repeat is on, and `is_empty` is true when
// `follows` is false. The CTE is materialized, so a read that puts the flags
// beside each row of a list computes them once.
sqlive_compose::fragment!(
    QueueFlags { has_next: bool, has_previous: bool, up_next_count: u32, is_empty: bool },
    r#"
    WITH flagged AS MATERIALIZED (
        SELECT queue.repeat,
               EXISTS (SELECT 1 FROM playback_manual)
               OR EXISTS (SELECT 1 FROM playback_context AS later
                          WHERE CASE queue.shuffled
                                    WHEN 1 THEN later.shuffled ELSE later.ordered END
                                > CASE queue.shuffled
                                    WHEN 1 THEN queue.shuffled_cursor
                                    ELSE queue.ordered_cursor END) AS follows,
               EXISTS (SELECT 1 FROM playback_context AS earlier
                       WHERE CASE queue.shuffled
                                 WHEN 1 THEN earlier.shuffled ELSE earlier.ordered END
                             < CASE queue.shuffled
                                 WHEN 1 THEN queue.shuffled_cursor
                                 ELSE queue.ordered_cursor END) AS before,
               (SELECT COUNT(*) FROM playback_manual) AS up_next_count
        FROM playback_queue AS queue)
    SELECT flagged.repeat <> 'off' OR flagged.follows AS has_next,
           flagged.before AS has_previous,
           flagged.up_next_count AS up_next_count,
           NOT flagged.follows AS is_empty
    FROM flagged
"#
);

// The rows of a lane that `lane_rows` gives, each with its track as the queue
// rows show it: the entry's id, which is `QueueEntryId`'s text ('m' for Up
// Next or 'c' for the context lane, then the entry number), the track and the
// place the rows are in order by.
sqlive_compose::fragment!(
    QueueItems
    with lane_rows(entry TEXT, track_id TEXT, position INTEGER)
    { entry: String, position: i64 },
    r#"
    SELECT queued.entry, queued.position, track.*
    FROM lane_rows AS queued
    JOIN @bae_records::records::track_display::DisplayedTrack AS track
      ON track.display_track_id = queued.track_id
"#
);

/// The queue row of the entry `entry_id`, which plays `track`, as the queue
/// screens show it.
pub(super) fn queue_item(entry_id: String, track: DisplayedTrack) -> Result<QueueItem, DbError> {
    Ok(QueueItem {
        entry_id,
        track_id: track.track_id().to_owned(),
        duration_ms: track.duration_ms(),
        display: track.into_display()?,
    })
}

// The rows of the session's up-next list, at most `:limit`: the Up Next rows,
// then the context lane's rows after the playing track in the queue's current
// order, numbered in that order. Each part reads at most `:limit` rows through
// the index of its order, from where the part starts. The fragment has one
// context part for each order, and each reads its cursor from the queue row
// only while the queue reads in that order, so the other part reads no rows.
sqlive_compose::fragment!(
    SessionUpNext(limit: i64) { entry: String, track_id: String, position: i64 },
    r#"
    SELECT merged.entry, merged.track_id,
           ROW_NUMBER() OVER (ORDER BY merged.part, merged.place) AS position
    FROM (
        SELECT * FROM (
            SELECT 'm' || manual.entry AS entry, manual.track_id, 0 AS part,
                   manual.position AS place
            FROM playback_manual AS manual
            ORDER BY manual.position LIMIT :limit)
        UNION ALL SELECT * FROM (
            SELECT 'c' || lane.entry, lane.track_id, 1, lane.ordered
            FROM playback_context AS lane
            WHERE lane.ordered > (SELECT queue.ordered_cursor FROM playback_queue AS queue
                                  WHERE queue.shuffled = 0)
            ORDER BY lane.ordered LIMIT :limit)
        UNION ALL SELECT * FROM (
            SELECT 'c' || lane.entry, lane.track_id, 1, lane.shuffled
            FROM playback_context AS lane
            WHERE lane.shuffled > (SELECT queue.shuffled_cursor FROM playback_queue AS queue
                                   WHERE queue.shuffled = 1)
            ORDER BY lane.shuffled LIMIT :limit)
        ORDER BY part, place LIMIT :limit) AS merged
"#
);

// The first row of the session's up-next list: the track that plays next.
sqlive_compose::read!(
    writer QueueFront(limit: i64),
    r#"
    SELECT front.*
    FROM @crate::tables::queue_reads::SessionUpNext(limit: :limit) AS front
"#
);

// The context lane's track after the playing one, while the lane reads in its
// own order and Up Next is empty; no row otherwise.
sqlive_compose::read!(
    writer NextInOrder,
    r#"
    SELECT later.track_id
    FROM playback_queue AS queue
    JOIN playback_context AS later ON later.ordered > queue.ordered_cursor
    WHERE queue.shuffled = 0
      AND NOT EXISTS (SELECT 1 FROM playback_manual)
    ORDER BY later.ordered
    LIMIT 1
"#
);

// The queue as its screens show it: a row for each Up Next entry, in order,
// each beside the queue's head and flags, and one row with no entry when Up
// Next is empty.
sqlive_compose::read!(
    writer QueueValue,
    r#"
    SELECT head.*, flags.*, item.*
    FROM @crate::tables::queue_reads::QueueHead AS head
    JOIN @crate::tables::queue_reads::QueueFlags AS flags ON 1
    LEFT JOIN @crate::tables::queue_reads::QueueItems(
        lane_rows: (SELECT 'm' || manual.entry, manual.track_id, manual.position
                    FROM playback_manual AS manual)) AS item ON 1
    ORDER BY item.position
"#
);

// The session's up-next list, each row with its track.
sqlive_compose::read!(
    writer SessionUpNextItems(limit: i64),
    r#"
    SELECT item.*
    FROM @crate::tables::queue_reads::QueueItems(
        lane_rows: @crate::tables::queue_reads::SessionUpNext(limit: :limit)) AS item
    ORDER BY item.position
"#
);

/// Follow the queue as its screens show it: Up Next whole, what the context
/// lane plays from, and whether the transport can step either way.
pub fn subscribe_queue_value(
    store: &Store,
) -> coven::LiveQuery<crate::ResolvedQueueSnapshot, bae_store::DbError> {
    store.subscribe(|sql| queue_value_on(&sql))
}

/// The queue as its screens show it now.
#[cfg(any(test, feature = "test-utils"))]
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

fn session_up_next_on(sql: &SqlReadContext<'_>, limit: u64) -> Result<Vec<QueueItem>, DbError> {
    let statement = SessionUpNextItemsParams { limit: bae_store::list_windows::sql_bound(limit) };
    statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| queue_item(row.item.entry, row.item.track))
        .collect()
}

/// The track that plays next, as the session's up-next list starts.
pub(super) fn queue_front_on(sql: &SqlReadContext<'_>) -> Result<Option<String>, DbError> {
    Ok(statement_optional(sql, &QueueFrontParams { limit: 1 })?.map(|row| row.front.track_id))
}

/// The track [`NextInOrder`] reads.
pub(super) fn next_in_order_on(sql: &SqlReadContext<'_>) -> Result<Option<String>, DbError> {
    Ok(statement_optional(sql, &NextInOrderParams {})?.map(|row| row.track_id))
}

pub(super) fn queue_value_on(
    sql: &SqlReadContext<'_>,
) -> Result<crate::ResolvedQueueSnapshot, DbError> {
    let rows = statement_rows(sql, &QueueValueParams {})?;
    // Every row repeats the one head and its flags.
    let mut head = None;
    let mut manual = Vec::new();
    for row in rows {
        if let Some(item) = row.item {
            manual.push(queue_item(item.entry, item.track)?);
        }
        head.get_or_insert((row.head, row.flags));
    }
    let (head, flags) =
        head.ok_or_else(|| DbError::unreadable("the queue's row", None::<String>))?;
    let (shuffled, context) = head.into_parts()?;
    Ok(crate::ResolvedQueueSnapshot {
        manual,
        context,
        shuffled,
        has_next: flags.has_next,
        has_previous: flags.has_previous,
        up_next_count: flags.up_next_count,
        is_empty: flags.is_empty,
    })
}

#[cfg(test)]
#[path = "queue_reads_tests.rs"]
mod tests;
