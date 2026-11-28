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
coven::fragment!(
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
coven::fragment!(
    UpcomingShuffled,
    r#"
    SELECT lane.entry, lane.shuffled AS place
    FROM playback_context AS lane
    WHERE lane.shuffled > (SELECT queue.shuffled_cursor FROM playback_queue AS queue)
"#
);

coven::read!(
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

coven::read!(
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

coven::read!(
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

coven::read!(
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

coven::read!(
    writer CountOrdered { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
"#
);

coven::read!(
    writer CountBeforeOrdered(place: i64) { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
    WHERE upcoming.place < :place
"#
);

// The place of the last of the first `:count` upcoming rows, NULL when there
// are none.
coven::read!(
    writer LastPlaceOfFirstOrdered(count: i64) { place: Option<i64> },
    r#"
    SELECT MAX(first.place) AS place
    FROM (SELECT upcoming.*
          FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
          ORDER BY upcoming.place
          LIMIT :count) AS first
"#
);

coven::read!(
    writer CountShuffled { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingShuffled AS upcoming
"#
);

coven::read!(
    writer CountBeforeShuffled(place: i64) { count: u64 },
    r#"
    SELECT COUNT(*) AS count
    FROM @crate::tables::queue_upcoming::UpcomingShuffled AS upcoming
    WHERE upcoming.place < :place
"#
);

// The place of the last of the first `:count` upcoming rows, NULL when there
// are none.
coven::read!(
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
coven::read!(
    writer EntryAtPlaceOrdered(moved: i64, offset: i64),
    r#"
    SELECT upcoming.*
    FROM @crate::tables::queue_upcoming::UpcomingOrdered AS upcoming
    WHERE upcoming.entry <> :moved
    ORDER BY upcoming.place
    LIMIT 1 OFFSET :offset
"#
);

coven::read!(
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

/// A row of the upcoming list as its window's read returns it: its place in
/// the order read, its entry of the context lane, and its track.
type UpcomingRow = (i64, i64, bae_records::records::track_display::DisplayedTrack);

/// The read of `window`'s rows of the upcoming list in the `shuffled` order.
fn upcoming_window_read(
    shuffled: bool,
    window: &ListWindow,
) -> Result<coven::ReadAs<UpcomingRow>, DbError> {
    let limit = sql_bound(window.limit);
    let from = window.from.as_ref().map(place_of).transpose()?;
    macro_rules! rows {
        ($statement:expr, $column:ident) => {
            coven::ReadAs::new($statement, |row| (row.$column, row.entry, row.track))
        };
    }
    Ok(match (shuffled, from) {
        (false, None) => rows!(WindowTopOrderedParams { limit }, ordered),
        (false, Some(from)) => rows!(WindowFromOrderedParams { from, limit }, ordered),
        (true, None) => rows!(WindowTopShuffledParams { limit }, shuffled),
        (true, Some(from)) => rows!(WindowFromShuffledParams { from, limit }, shuffled),
    })
}

/// `window`'s rows of the upcoming list as its read returned them, as the
/// queue rows show them.
fn upcoming_window_of(
    window: &ListWindow,
    rows: Vec<UpcomingRow>,
) -> Result<WindowRead<QueueItem>, DbError> {
    let rows = rows
        .into_iter()
        .map(|(place, entry, track)| keyed_item(place, entry, track))
        .collect::<Result<_, DbError>>()?;
    Ok(WindowRead { window: window.clone(), rows })
}

/// Every one of the context lane's tracks after the playing one, as the queue
/// rows show them.
#[cfg(any(test, feature = "test-utils"))]
pub async fn queue_upcoming(store: &Store) -> Result<Vec<QueueItem>, DbError> {
    store
        .read(|sql| {
            let shuffled = super::queue_reads::queue_value_on(&sql)?.shuffled;
            let top = ListWindow::top(u64::MAX);
            let window =
                upcoming_window_of(&top, sql.reads(&upcoming_window_read(shuffled, &top)?)?)?;
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
    store
        .subscribe_reconfigurable(initial, move |window| upcoming_window_read(shuffled, window))
        .process(upcoming_window_of)
}

/// Follow how many tracks come after the playing one. The count runs as its
/// own query, apart from the window's rows.
pub fn subscribe_upcoming_count(
    store: &Store,
    shuffled: bool,
) -> coven::LiveQuery<u64, bae_store::DbError> {
    match shuffled {
        true => store.subscribe_row(CountShuffledParams {}).process(|row| Ok(row.count)),
        false => store.subscribe_row(CountOrderedParams {}).process(|row| Ok(row.count)),
    }
}

/// Follow how many upcoming tracks come before the row at `initial`, a count
/// over an index range; when the caller changes the key, the same live query
/// counts again for the new key.
pub fn subscribe_upcoming_position(
    store: &Store,
    shuffled: bool,
    initial: OrderKey,
) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
    store
        .subscribe_reconfigurable(initial, move |key| {
            let place = place_of(key)?;
            Ok(match shuffled {
                true => coven::ReadAs::new(CountBeforeShuffledParams { place }, |row| row.count),
                false => coven::ReadAs::new(CountBeforeOrderedParams { place }, |row| row.count),
            })
        })
        .process(|_, counts| bae_store::exactly_one::<coven::ReadAs<u64>, _>(counts))
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
