//! The context lane's upcoming tracks as a list: the tracks after the playing
//! one, in the order the lane reads in, read as one window from a place on,
//! with the lane's own count and the position of a key in it. The lane's
//! places are dense integers, so a place is the key.
//!
//! The lane has two orders, and SQL fixes which column a statement sorts by,
//! so each read is one statement for each order, and the request picks one.

use super::queue_reads::queue_item;
use super::*;
use crate::{QueueEntryId, QueueItem};
use bae_store::list_windows::{sql_bound, Keyed, OrderKey, SortValue};
use bae_store::{ListWindow, WindowRead};

// The context lane's rows after its playing one in its own order: those
// placed past that order's cursor. When no track is playing, the cursor is
// NULL and the fragment gives no rows.
sqlive_compose::fragment!(
    UpcomingOrdered,
    r#"
    SELECT lane.entry, lane.ordered AS place
    FROM playback_context AS lane
    WHERE lane.ordered > (SELECT queue.ordered_cursor FROM playback_queue AS queue)
"#
);

// The context lane's rows after its playing one in the shuffled order: those
// placed past that order's cursor. When no track is playing, the cursor is
// NULL and the fragment gives no rows.
sqlive_compose::fragment!(
    UpcomingShuffled,
    r#"
    SELECT lane.entry, lane.shuffled AS place
    FROM playback_context AS lane
    WHERE lane.shuffled > (SELECT queue.shuffled_cursor FROM playback_queue AS queue)
"#
);

sqlive_compose::read!(
    writer WindowTopOrdered(limit: i64),
    r#"
    SELECT lane.ordered, lane.entry, track.*
    FROM playback_context AS lane
    JOIN @bae_records::records::track_display::DisplayedTrack AS track
      ON track.display_track_id = lane.track_id
    WHERE lane.ordered > (SELECT queue.ordered_cursor FROM playback_queue AS queue)
    ORDER BY lane.ordered
    LIMIT :limit
"#
);

sqlive_compose::read!(
    writer WindowFromOrdered(from: i64, limit: i64),
    r#"
    SELECT lane.ordered, lane.entry, track.*
    FROM playback_context AS lane
    JOIN @bae_records::records::track_display::DisplayedTrack AS track
      ON track.display_track_id = lane.track_id
    WHERE lane.ordered > (SELECT queue.ordered_cursor FROM playback_queue AS queue)
      AND lane.ordered >= :from
    ORDER BY lane.ordered
    LIMIT :limit
"#
);

sqlive_compose::read!(
    writer WindowTopShuffled(limit: i64),
    r#"
    SELECT lane.shuffled, lane.entry, track.*
    FROM playback_context AS lane
    JOIN @bae_records::records::track_display::DisplayedTrack AS track
      ON track.display_track_id = lane.track_id
    WHERE lane.shuffled > (SELECT queue.shuffled_cursor FROM playback_queue AS queue)
    ORDER BY lane.shuffled
    LIMIT :limit
"#
);

sqlive_compose::read!(
    writer WindowFromShuffled(from: i64, limit: i64),
    r#"
    SELECT lane.shuffled, lane.entry, track.*
    FROM playback_context AS lane
    JOIN @bae_records::records::track_display::DisplayedTrack AS track
      ON track.display_track_id = lane.track_id
    WHERE lane.shuffled > (SELECT queue.shuffled_cursor FROM playback_queue AS queue)
      AND lane.shuffled >= :from
    ORDER BY lane.shuffled
    LIMIT :limit
"#
);

sqlive_compose::read!(
    writer CountOrdered { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
"#
);

sqlive_compose::read!(
    writer CountBeforeOrdered(place: i64) { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
    WHERE upcoming.place < :place
"#
);

// The place of the last of the first `:count` upcoming rows, NULL when there
// are none.
sqlive_compose::read!(
    writer LastPlaceOfFirstOrdered(count: i64) { place: Option<i64> },
    r#"
    SELECT MAX(first.place) AS place
    FROM (SELECT upcoming.*
          FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
          ORDER BY upcoming.place
          LIMIT :count) AS first
"#
);

sqlive_compose::read!(
    writer CountShuffled { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingShuffled AS upcoming
"#
);

sqlive_compose::read!(
    writer CountBeforeShuffled(place: i64) { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingShuffled AS upcoming
    WHERE upcoming.place < :place
"#
);

// The place of the last of the first `:count` upcoming rows, NULL when there
// are none.
sqlive_compose::read!(
    writer LastPlaceOfFirstShuffled(count: i64) { place: Option<i64> },
    r#"
    SELECT MAX(first.place) AS place
    FROM (SELECT upcoming.*
          FROM @crate::tables::queue_upcoming::UpcomingShuffled AS upcoming
          ORDER BY upcoming.place
          LIMIT :count) AS first
"#
);

// The entry that a row moved to place `:offset` of the upcoming list goes
// before: the entry there once the moved row `:moved` is left out.
sqlive_compose::read!(
    writer EntryAtPlaceOrdered(moved: i64, offset: i64),
    r#"
    SELECT upcoming.*
    FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
    WHERE upcoming.entry <> :moved
    ORDER BY upcoming.place
    LIMIT 1 OFFSET :offset
"#
);

sqlive_compose::read!(
    writer EntryAtPlaceShuffled(moved: i64, offset: i64),
    r#"
    SELECT upcoming.*
    FROM @crate::tables::queue_upcoming::UpcomingShuffled AS upcoming
    WHERE upcoming.entry <> :moved
    ORDER BY upcoming.place
    LIMIT 1 OFFSET :offset
"#
);

/// The entry among the upcoming rows of the `shuffled` order that the row
/// `moved`, moved to `offset` among them, goes before; `None` when the offset
/// is past their end.
pub(super) fn upcoming_entry_at_on(
    sql: &impl bae_store::WriterRead,
    shuffled: bool,
    moved: i64,
    offset: i64,
) -> Result<Option<i64>, DbError> {
    Ok(if shuffled {
        statement_optional(sql, &EntryAtPlaceShuffledParams { moved, offset })?
            .map(|row| row.upcoming.entry)
    } else {
        statement_optional(sql, &EntryAtPlaceOrderedParams { moved, offset })?
            .map(|row| row.upcoming.entry)
    })
}

/// The key of the upcoming row at `place`: the place is the whole of it.
fn key_of(place: i64) -> OrderKey {
    OrderKey::new(vec![SortValue::Integer(place)])
}

/// The place that `key` of the upcoming list names.
fn place_of(key: &OrderKey) -> Result<i64, DbError> {
    match key.values() {
        [SortValue::Integer(place)] => Ok(*place),
        other => Err(DbError::unreadable("a key of the upcoming list", format!("{other:?}"))),
    }
}

/// The upcoming row at `place`, entry `entry` of the context lane, as the
/// queue rows show it, with its key.
fn keyed_item(
    place: i64,
    entry: i64,
    track: bae_records::records::track_display::DisplayedTrack,
) -> Result<Keyed<QueueItem>, DbError> {
    let item = queue_item(QueueEntryId::Context(entry).to_string(), track)?;
    Ok((item, key_of(place)))
}

/// `window`'s rows of the upcoming list in the `shuffled` order.
fn upcoming_window_on(
    sql: &SqlReadContext<'_>,
    shuffled: bool,
    window: &ListWindow,
) -> Result<WindowRead<QueueItem>, DbError> {
    let limit = sql_bound(window.limit);
    let from = window.from.as_ref().map(place_of).transpose()?;
    macro_rules! rows {
        ($statement:expr, $column:ident) => {
            statement_rows(sql, &$statement)?
                .into_iter()
                .map(|row| keyed_item(row.$column, row.entry, row.track))
                .collect::<Result<Vec<_>, DbError>>()?
        };
    }
    let rows = match (shuffled, from) {
        (false, None) => rows!(WindowTopOrderedParams { limit }, ordered),
        (false, Some(from)) => rows!(WindowFromOrderedParams { from, limit }, ordered),
        (true, None) => rows!(WindowTopShuffledParams { limit }, shuffled),
        (true, Some(from)) => rows!(WindowFromShuffledParams { from, limit }, shuffled),
    };
    Ok(WindowRead { window: window.clone(), rows })
}

/// Every one of the context lane's tracks after the playing one, as the queue
/// rows show them.
#[cfg(any(test, feature = "test-utils"))]
pub async fn queue_upcoming(store: &Store) -> Result<Vec<QueueItem>, DbError> {
    store
        .read(|sql| {
            let shuffled = super::queue_reads::queue_value_on(&sql)?.shuffled;
            let window = upcoming_window_on(&sql, shuffled, &ListWindow::top(u64::MAX))?;
            Ok(window.rows.into_iter().map(|(item, _key)| item).collect())
        })
        .await
}

/// Follow one window of the upcoming tracks, the tracks from a place on,
/// each with its track as the queue rows show it. Coven reruns it for a write
/// at or before its last row, and when the playing track moves.
pub fn subscribe_upcoming_window(
    store: &Store,
    shuffled: bool,
    initial: ListWindow,
) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<QueueItem>, bae_store::DbError> {
    store.subscribe_reconfigurable(initial, move |window, sql| {
        upcoming_window_on(&sql, shuffled, window)
    })
}

/// Follow how many tracks come after the playing one. The count runs as its
/// own query, apart from the window's rows.
pub fn subscribe_upcoming_count(
    store: &Store,
    shuffled: bool,
) -> coven::LiveQuery<u64, bae_store::DbError> {
    store.subscribe(move |sql| {
        Ok(if shuffled {
            statement_row(&sql, &CountShuffledParams {})?.count
        } else {
            statement_row(&sql, &CountOrderedParams {})?.count
        })
    })
}

/// Follow how many upcoming tracks come before the row at `initial`, a count
/// over an index range; when the caller changes the key, the same live query
/// counts again for the new key.
pub fn subscribe_upcoming_position(
    store: &Store,
    shuffled: bool,
    initial: OrderKey,
) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
    store.subscribe_reconfigurable(initial, move |key, sql| {
        let place = place_of(key)?;
        Ok(if shuffled {
            statement_row(&sql, &CountBeforeShuffledParams { place })?.count
        } else {
            statement_row(&sql, &CountBeforeOrderedParams { place })?.count
        })
    })
}

/// The key of the upcoming track at `position`, or of the last one when the
/// lane ends before it, read on `sql`; `None` when nothing is upcoming.
pub(super) fn upcoming_key_at_on(
    sql: &impl bae_store::WriterRead,
    shuffled: bool,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    let count = sql_bound(position.saturating_add(1));
    let place = if shuffled {
        statement_row(sql, &LastPlaceOfFirstShuffledParams { count })?.place
    } else {
        statement_row(sql, &LastPlaceOfFirstOrderedParams { count })?.place
    };
    Ok(place.map(key_of))
}

/// The key of the upcoming track at `position`, or of the last one when the
/// lane ends before it; `None` when nothing is upcoming.
pub async fn upcoming_key_at(
    store: &Store,
    shuffled: bool,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    store.read(move |sql| upcoming_key_at_on(&sql, shuffled, position)).await
}
