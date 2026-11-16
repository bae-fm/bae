//! The context lane's upcoming tracks as a list: the tracks after the playing
//! one, in the order the lane reads in, read as windows that are ranges of
//! that order, each with the lane's own count and the position of a key in it.
//! The lane's places are dense integers, so a place is the key.

use super::queue::context_order_column;
use super::queue_reads::{queue_item_from_row, queue_items_sql};
use super::*;
use crate::QueueItem;
use bae_store::list_windows::{key_at, Keyed, ListOrder, OrderKey, Seek, SortTerm};
use bae_store::{ListWindow, WindowRead};

/// The upcoming tracks' order: the lane's place in the order it reads in,
/// which is its own while shuffle is off and the shuffled one while it is on.
pub fn upcoming_order(shuffled: bool) -> ListOrder {
    ListOrder::new(vec![SortTerm::ascending(format!("c.{}", context_order_column(shuffled)))])
}

/// The rows of the context lane after its playing one, as a condition over
/// `playback_context c`: those placed past the cursor of the order the lane
/// reads in. When no track is playing, the cursor is NULL and the condition
/// matches no rows.
pub(super) fn after_the_playing_track(shuffled: bool) -> String {
    format!(
        "c.{place} > (SELECT {cursor} FROM playback_queue)",
        place = context_order_column(shuffled),
        cursor = if shuffled { "shuffled_cursor" } else { "ordered_cursor" },
    )
}

/// The upcoming tracks `seek` reads in the lane's order, at most `limit` from
/// `offset`, each with its key. The inner query applies LIMIT and OFFSET
/// before the join, so SQLite never joins the skipped rows to their tracks.
pub(super) fn upcoming_rows_on(
    sql: &SqlReadContext<'_>,
    shuffled: bool,
    seek: &Seek,
    limit: u64,
    offset: u64,
) -> Result<Vec<Keyed<QueueItem>>, DbError> {
    let order = upcoming_order(shuffled);
    let statement = queue_items_sql(&format!(
        "SELECT 'c' || c.entry AS entry, c.track_id AS lane_track, {place} AS position, \
                {keys} \
         FROM playback_context c \
         WHERE {after} AND {seek} \
         ORDER BY {ordering} LIMIT :limit OFFSET :offset",
        place = format_args!("c.{}", context_order_column(shuffled)),
        keys = order.key_columns(),
        after = after_the_playing_track(shuffled),
        seek = order.condition(seek),
        ordering = order.ordering(seek),
    ));
    Ok(seek.bound(limit, offset, |params| {
        sql.query(&statement, params, |row| Ok((queue_item_from_row(row)?, order.key_of(row)?)))
    })?)
}

/// Every one of the context lane's tracks after the playing one, as the queue
/// rows show them.
#[cfg(any(test, feature = "test-utils"))]
pub async fn queue_upcoming(store: &Store) -> Result<Vec<QueueItem>, DbError> {
    store
        .read(|sql| {
            let shuffled: bool =
                sql.query_row("SELECT shuffled FROM playback_queue", [], |row| row.get(0))?;
            let order = upcoming_order(shuffled);
            Ok(upcoming_rows_on(&sql, shuffled, &order.everything(), u64::MAX, 0)?
                .into_iter()
                .map(|(item, _key)| item)
                .collect())
        })
        .await
}

/// Follow one window of the upcoming tracks, a range of the lane's order,
/// each with its track as the queue rows show it. Coven reruns it for a write
/// inside the range, and when the playing track moves.
pub fn subscribe_upcoming_window(
    store: &Store,
    shuffled: bool,
    initial: ListWindow,
) -> coven::ReconfigurableLiveQuery<ListWindow, WindowRead<QueueItem>, bae_store::DbError> {
    store.subscribe_list_window(
        initial,
        upcoming_order(shuffled),
        move |sql, seek, limit, offset| upcoming_rows_on(sql, shuffled, seek, limit, offset),
    )
}

/// Follow how many tracks come after the playing one. The count runs as its
/// own query, apart from the window's rows.
pub fn subscribe_upcoming_count(
    store: &Store,
    shuffled: bool,
) -> coven::LiveQuery<u64, bae_store::DbError> {
    store.subscribe(move |sql| {
        Ok(sql.query_row(
            &format!(
                "SELECT COUNT(*) FROM playback_context c WHERE {}",
                after_the_playing_track(shuffled)
            ),
            [],
            |row| row.get(0),
        )?)
    })
}

/// Follow how many upcoming tracks come before the row at `initial`, a count
/// over index ranges; when the caller changes the key, the same live query
/// counts again for the new key.
pub fn subscribe_upcoming_position(
    store: &Store,
    shuffled: bool,
    initial: OrderKey,
) -> coven::ReconfigurableLiveQuery<OrderKey, u64, bae_store::DbError> {
    let order = upcoming_order(shuffled);
    store.subscribe_reconfigurable(initial, move |key, sql| {
        bae_store::list_windows::rows_before_on(
            &sql,
            "FROM playback_context c",
            Some(&after_the_playing_track(shuffled)),
            &[],
            &order,
            key,
        )
    })
}

/// The key of the upcoming track at `position`, or of the last one when the
/// lane ends before it; `None` when nothing is upcoming.
pub async fn upcoming_key_at(
    store: &Store,
    shuffled: bool,
    position: u64,
) -> Result<Option<OrderKey>, DbError> {
    store
        .read(move |sql| {
            key_at(&upcoming_order(shuffled), position, &mut |seek, limit, offset| {
                upcoming_rows_on(&sql, shuffled, seek, limit, offset)
            })
        })
        .await
}
