//! The playback queue's tables. Up Next is `playback_manual`, the context lane
//! a play fills is `playback_context`; in both each row is an entry of its own,
//! named by its entry number and ordered by its position, so a track queued
//! twice is two rows. The one `playback_queue` row says what the context lane
//! plays from, which of its rows is playing, how the queue repeats and
//! whether it shuffles, and the additions to Up Next since playback started with the library. The
//! playback service is the only writer, each change one transaction; the queue
//! screens read the same tables through live queries. Its tables, `playback_state`
//! included, are `queue.sql`.

/// The queue's part of the library's schema.
pub(crate) const SCHEMA: bae_store::SchemaPart =
    bae_store::SchemaPart::new(&[include_str!("queue.sql")]);

use super::queue_fill::{fill_on, ContextFill, ContextFilled};
use super::*;
use crate::{QueueEntry, QueueEntryId, RepeatMode};

/// The lane a queue entry is in, and the table holding it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lane {
    Manual,
    Context,
}

impl Lane {
    /// The statement that counts a write changing this lane in its version on
    /// the queue row: Up Next's entries put in, taken out or moved, or the
    /// context lane's tracks after the playing one changed.
    fn count_change(self) -> &'static str {
        match self {
            Lane::Manual => MANUAL_CHANGED,
            Lane::Context => CONTEXT_CHANGED,
        }
    }

    fn table(self) -> &'static str {
        match self {
            Lane::Manual => "playback_manual",
            Lane::Context => "playback_context",
        }
    }

    fn entry_id(self, entry: i64) -> QueueEntryId {
        match self {
            Lane::Manual => QueueEntryId::Manual(entry),
            Lane::Context => QueueEntryId::Context(entry),
        }
    }
}

/// Counts a write that changed Up Next's entries.
pub(super) const MANUAL_CHANGED: &str =
    "UPDATE playback_queue SET manual_version = manual_version + 1";
/// Counts a write that changed the context lane's tracks after its playing
/// one, or which track is playing.
pub(super) const CONTEXT_CHANGED: &str =
    "UPDATE playback_queue SET context_version = context_version + 1";

/// The lane an entry id names and the entry number of its row there.
fn row_key(id: &QueueEntryId) -> (Lane, i64) {
    match id {
        QueueEntryId::Manual(entry) => (Lane::Manual, *entry),
        QueueEntryId::Context(entry) => (Lane::Context, *entry),
    }
}

/// Where manual-lane additions go.
#[derive(Debug, Clone, Copy)]
pub enum ManualPlace {
    End,
    Front,
    /// Before the entry at this index of the lane; past its end is the end.
    At(usize),
}

/// The playback queue's tables, as the playback service changes them.
#[derive(Clone)]
pub struct PlaybackQueueTables {
    store: Store,
}

impl PlaybackQueueTables {
    /// The tables over `store`.
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

/// Empty the queue and the resume row unless the resume row is kept: what
/// starting playback with a library does when it will not resume what it
/// played. A library with nothing to resume opens with an empty queue, and
/// one opened with "restore on launch" off forgets what it played. Either
/// way `playback_now` starts stopped, at the resumed volume or at full
/// volume, `playback_timeline` holds no line, and the queue's count of
/// additions starts at none. A bulk write, so emptying a library-sized lane
/// takes one step.
pub async fn settle_playback_queue(store: &Store, resume: bool) -> Result<(), DbError> {
    store
        .call_bulk(move |sql| {
            let stored =
                sql.query_row("SELECT 1 FROM playback_state", [], |_| Ok(())).optional()?.is_some();
            super::now::reset_playback_now_on(sql, resume && stored)?;
            super::timeline::reset_playback_timeline_on(sql)?;
            // No addition outlives the playback that made it, so a resumed
            // queue confirms none of its earlier ones again.
            sql.execute("UPDATE playback_queue SET additions = 0, added_tracks = NULL", [])?;
            if resume && stored {
                return Ok(());
            }
            sql.execute("DELETE FROM playback_state", [])?;
            if sql.execute("DELETE FROM playback_manual", [])? > 0 {
                sql.execute(MANUAL_CHANGED, [])?;
            }
            clear_context_on(sql)?;
            sql.execute("UPDATE playback_queue SET repeat = 'off', shuffled = 0", [])?;
            Ok(())
        })
        .await
}

impl PlaybackQueueTables {
    /// How the queue repeats.
    pub async fn repeat(&self) -> Result<RepeatMode, DbError> {
        self.store
            .read(|sql| {
                Ok(sql.query_row("SELECT repeat FROM playback_queue", [], |row| {
                    repeat_from_row(row, 0)
                })?)
            })
            .await
    }

    /// How many tracks the context lane holds, played or not.
    pub async fn context_track_count(&self) -> Result<usize, DbError> {
        self.store
            .read(|sql| {
                let count: i64 =
                    sql.query_row("SELECT count(*) FROM playback_context", [], |row| row.get(0))?;
                Ok(count as usize)
            })
            .await
    }

    /// Replace the whole queue with what `fill` names: Up Next empties, and the
    /// context lane holds the tracks in play order, playing the first. A bulk
    /// write: a play takes out and puts in up to a library's worth of rows.
    pub async fn fill(&self, fill: ContextFill) -> Result<ContextFilled, DbError> {
        self.store.call_bulk(move |sql| fill_on(sql, fill)).await
    }

    /// Add tracks to Up Next where `place` says, leaving out any the library
    /// does not hold, and say how many went in. Putting any in counts one
    /// addition on the queue row, with how many.
    pub async fn add_manual(
        &self,
        track_ids: Vec<String>,
        place: ManualPlace,
    ) -> Result<usize, DbError> {
        self.store.call(move |sql| add_manual_on(sql, &track_ids, place)).await
    }

    /// Empty Up Next, as a bulk write.
    pub async fn clear_manual(&self) -> Result<(), DbError> {
        self.store
            .call_bulk(|sql| {
                if sql.execute("DELETE FROM playback_manual", [])? > 0 {
                    sql.execute(MANUAL_CHANGED, [])?;
                }
                Ok(())
            })
            .await
    }

    /// Empty the context lane, as a bulk write; `false` when there was none.
    pub async fn clear_context(&self) -> Result<bool, DbError> {
        self.store
            .call_bulk(|sql| {
                if queue_row_on(sql)?.cursor.is_none() {
                    return Ok(false);
                }
                clear_context_on(sql)?;
                Ok(true)
            })
            .await
    }

    /// Turn shuffle on or off: the mode is kept with nothing playing, and a
    /// context lane puts its tracks after the playing one in shuffled order,
    /// or back in the order they had before it was shuffled. The playing track
    /// and the ones before it stay where they are.
    pub async fn set_shuffle(&self, on: bool) -> Result<(), DbError> {
        self.store.call(move |sql| set_shuffle_on(sql, on)).await
    }

    pub async fn set_repeat(&self, mode: RepeatMode) -> Result<(), DbError> {
        self.store
            .call(move |sql| {
                sql.execute("UPDATE playback_queue SET repeat = ?", [repeat_column(mode)])?;
                Ok(())
            })
            .await
    }

    /// Take the entry out of whichever lane holds it; `None` when no lane does.
    pub async fn remove(&self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        self.store
            .call(move |sql| {
                let (lane, entry) = row_key(&id);
                let removed = sql
                    .query_row(
                        &format!(
                            "DELETE FROM {table} WHERE entry = ? RETURNING track_id",
                            table = lane.table()
                        ),
                        [entry],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                let Some(track_id) = removed else {
                    return Ok(None);
                };
                sql.execute(lane.count_change(), [])?;
                if lane == Lane::Context {
                    // The lane's last row takes what it played from with it,
                    // as a deleted track does (`tracks_leave_playback_queue`).
                    sql.execute(
                        &format!(
                            "{CONTEXT_SOURCE_CLEARED} WHERE source_kind IS NOT NULL \
                             AND NOT EXISTS (SELECT 1 FROM playback_context)"
                        ),
                        [],
                    )?;
                }
                Ok(Some(QueueEntry { id, track_id }))
            })
            .await
    }

    /// Move the entry to sit just before `before`, or to the end of its lane
    /// without one, and answer the lane's version the move made. Both must be
    /// in the same lane; `None` when either is in none or they are in
    /// different ones. The context lane's playing track stays the playing
    /// track wherever it moves.
    pub async fn reorder(
        &self,
        id: QueueEntryId,
        before: Option<QueueEntryId>,
    ) -> Result<Option<u64>, DbError> {
        self.store.call(move |sql| reorder_on(sql, id, before)).await
    }

    /// Make the entry the one playing: an Up Next entry leaves the lane with
    /// the ones ahead of it, a context entry becomes the lane's playing track.
    /// `None` when no lane holds it.
    pub async fn skip_to(&self, id: QueueEntryId) -> Result<Option<QueueEntry>, DbError> {
        self.store
            .call(move |sql| {
                let Some(located) = locate_on(sql, id)? else {
                    return Ok(None);
                };
                match located.lane {
                    Lane::Manual => {
                        sql.execute(
                            "DELETE FROM playback_manual WHERE position <= ?",
                            [located.position],
                        )?;
                    }
                    Lane::Context => set_cursor_on(sql, located.position)?,
                }
                sql.execute(located.lane.count_change(), [])?;
                Ok(Some(located.entry))
            })
            .await
    }

    /// Play on: the first Up Next entry leaves the lane, or the context lane's
    /// next track becomes its playing one. `None` when neither lane has a
    /// track after the playing one.
    pub async fn advance(&self) -> Result<Option<QueueEntry>, DbError> {
        self.store.call(advance_on).await
    }

    /// Play the context lane again from its first track, in a new shuffled
    /// order when it is shuffled. `None` without a context lane.
    pub async fn wrap(&self) -> Result<Option<QueueEntry>, DbError> {
        self.store.call(wrap_on).await
    }

    /// Step the context lane back: to the track before its playing one when
    /// `current` is that playing track, otherwise to the playing track itself,
    /// or the one before it when it has left the lane. `None` when there is
    /// nothing to step back to.
    pub async fn step_back(
        &self,
        current: Option<QueueEntryId>,
    ) -> Result<Option<QueueEntry>, DbError> {
        self.store.call(move |sql| step_back_on(sql, current)).await
    }

    /// The track that plays next: Up Next's first, or else the context lane's
    /// track after the playing one.
    pub async fn front(&self) -> Result<Option<String>, DbError> {
        self.store
            .read(|sql| {
                let manual = sql
                    .query_row(
                        "SELECT track_id FROM playback_manual ORDER BY position LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?;
                if manual.is_some() {
                    return Ok(manual);
                }
                Ok(sql
                    .query_row(
                        "SELECT track_id FROM playback_context \
                         WHERE position > (SELECT cursor FROM playback_queue) \
                         ORDER BY position LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await
    }

    /// The context lane's next track when it plays straight on from the
    /// playing one: Up Next empty and the lane in its own order. The only
    /// track a side or disc pause can fall before.
    pub async fn next_in_order(&self) -> Result<Option<String>, DbError> {
        self.store
            .read(|sql| {
                Ok(sql
                    .query_row(
                        "SELECT c.track_id FROM playback_queue q \
                         JOIN playback_context c ON c.position > q.cursor \
                         WHERE q.shuffled = 0 \
                           AND NOT EXISTS (SELECT 1 FROM playback_manual) \
                         ORDER BY c.position LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()?)
            })
            .await
    }

    /// Every track of the context lane, played or not, in lane order.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn context_order(&self) -> Result<Vec<String>, DbError> {
        self.store
            .read(|sql| {
                Ok(sql.query(
                    "SELECT track_id FROM playback_context ORDER BY position",
                    [],
                    |row| row.get(0),
                )?)
            })
            .await
    }
}

/// The `playback_queue` row's columns the lane changes read.
pub(super) struct QueueRow {
    cursor: Option<i64>,
    pub(super) shuffled: bool,
    pub(super) next_entry: i64,
}

pub(super) fn queue_row_on(sql: &impl coven::SqlRead) -> rusqlite::Result<QueueRow> {
    sql.query_row("SELECT cursor, shuffled, next_entry FROM playback_queue", [], |row| {
        Ok(QueueRow {
            cursor: row.get("cursor")?,
            shuffled: row.get("shuffled")?,
            next_entry: row.get("next_entry")?,
        })
    })
}

fn set_cursor_on(sql: &CovenWrite<'_>, position: i64) -> rusqlite::Result<()> {
    sql.execute("UPDATE playback_queue SET cursor = ?", [position])?;
    Ok(())
}

/// What the context lane plays from cleared: its source and its cursor, which
/// the schema keeps NULL together exactly while the lane is empty. The shuffle
/// mode stays. The schema's `tracks_leave_playback_queue` trigger writes the same
/// clear when a deleted track was the lane's last.
const CONTEXT_SOURCE_CLEARED: &str = "UPDATE playback_queue \
     SET source_kind = NULL, source_release_id = NULL, cursor = NULL";

/// Empty the context lane and clear what it played from, in the same write.
fn clear_context_on(sql: &coven::BulkSqlContext<'_>) -> rusqlite::Result<()> {
    if sql.execute("DELETE FROM playback_context", [])? > 0 {
        sql.execute(CONTEXT_CHANGED, [])?;
    }
    sql.execute(CONTEXT_SOURCE_CLEARED, [])?;
    Ok(())
}

/// An entry and where it sits.
struct Located {
    entry: QueueEntry,
    lane: Lane,
    position: i64,
}

fn locate_on(sql: &CovenWrite<'_>, id: QueueEntryId) -> rusqlite::Result<Option<Located>> {
    let (lane, entry) = row_key(&id);
    sql.query_row(
        &format!("SELECT position, track_id FROM {table} WHERE entry = ?", table = lane.table()),
        [entry],
        |row| {
            Ok(Located {
                lane,
                position: row.get("position")?,
                entry: QueueEntry { id, track_id: row.get("track_id")? },
            })
        },
    )
    .optional()
}

/// A row of `lane` read with its `entry`, `track_id` and `position`.
fn entry_from_row(lane: Lane, row: &Row<'_>) -> rusqlite::Result<(QueueEntry, i64)> {
    Ok((
        QueueEntry { id: lane.entry_id(row.get("entry")?), track_id: row.get("track_id")? },
        row.get("position")?,
    ))
}

/// A context row read with its `entry`, `track_id` and `position`.
pub(super) fn context_from_row(row: &Row<'_>) -> rusqlite::Result<(QueueEntry, i64)> {
    entry_from_row(Lane::Context, row)
}

/// The highest position in `lane`, `None` when it is empty.
fn lane_top_on(sql: &CovenWrite<'_>, lane: Lane) -> rusqlite::Result<Option<i64>> {
    sql.query_row(&format!("SELECT max(position) FROM {}", lane.table()), [], |row| row.get(0))
}

/// Move `lane`'s rows at positions `low..=high` by `delta`. Positions are
/// unique in a lane at every statement, so the rows first go above every
/// position the lane holds, then come down to where they belong. The caller
/// has left the positions they come down to free.
fn shift_on(
    sql: &CovenWrite<'_>,
    lane: Lane,
    low: i64,
    high: i64,
    delta: i64,
) -> rusqlite::Result<()> {
    if low > high {
        return Ok(());
    }
    let Some(top) = lane_top_on(sql, lane)? else {
        return Ok(());
    };
    let lift = top - low + delta.abs() + 1;
    sql.execute(
        &format!(
            "UPDATE {} SET position = position + ?1 WHERE position BETWEEN ?2 AND ?3",
            lane.table()
        ),
        params![lift, low, high],
    )?;
    sql.execute(
        &format!("UPDATE {} SET position = position - ?1 + ?2 WHERE position >= ?3", lane.table()),
        params![lift, delta, low + lift],
    )?;
    Ok(())
}

/// Give the context lane's rows after `after` new positions above every one
/// it holds, in the order `order` sorts them by.
fn reorder_context_after_on(sql: &CovenWrite<'_>, after: i64, order: &str) -> rusqlite::Result<()> {
    let Some(top) = lane_top_on(sql, Lane::Context)? else {
        return Ok(());
    };
    sql.execute(
        &format!(
            "UPDATE playback_context SET position = ?1 + placed.k \
             FROM (SELECT entry, row_number() OVER (ORDER BY {order}) AS k \
                   FROM playback_context WHERE position > ?2) AS placed \
             WHERE playback_context.entry = placed.entry"
        ),
        params![top, after],
    )?;
    Ok(())
}

fn add_manual_on(
    sql: &CovenWrite<'_>,
    track_ids: &[String],
    place: ManualPlace,
) -> Result<usize, DbError> {
    let track_ids =
        serde_json::to_string(track_ids).expect("serializing a Vec<String> to JSON cannot fail");
    let count: i64 = sql.query_row(
        "SELECT count(*) FROM json_each(?) j JOIN tracks t ON t.id = j.value",
        [&track_ids],
        |row| row.get(0),
    )?;
    if count == 0 {
        return Ok(0);
    }
    let bottom: Option<i64> =
        sql.query_row("SELECT min(position) FROM playback_manual", [], |row| row.get(0))?;
    let top = lane_top_on(sql, Lane::Manual)?;
    let end = top.map_or(1, |top| top + 1);
    let start = match place {
        ManualPlace::End => end,
        ManualPlace::Front => bottom.map_or(1, |bottom| bottom - count),
        ManualPlace::At(index) => {
            let at: Option<i64> = sql
                .query_row(
                    "SELECT position FROM playback_manual ORDER BY position LIMIT 1 OFFSET ?",
                    [index as i64],
                    |row| row.get(0),
                )
                .optional()?;
            match at {
                Some(at) => {
                    shift_on(sql, Lane::Manual, at, end - 1, count)?;
                    at
                }
                None => end,
            }
        }
    };
    let base = queue_row_on(sql)?.next_entry;
    sql.execute(
        "INSERT INTO playback_manual (entry, position, track_id) \
         SELECT ?2 + k, ?3 + k, track_id FROM ( \
             SELECT t.id AS track_id, row_number() OVER (ORDER BY j.key) - 1 AS k \
             FROM json_each(?1) j JOIN tracks t ON t.id = j.value)",
        params![track_ids, base, start],
    )?;
    // The addition is counted in the write that makes it, so the screens
    // confirming additions read exactly the ones that happened.
    sql.execute(
        "UPDATE playback_queue \
         SET next_entry = ?, additions = additions + 1, added_tracks = ?, \
             manual_version = manual_version + 1",
        params![base + count, count],
    )?;
    Ok(count as usize)
}

fn set_shuffle_on(sql: &CovenWrite<'_>, on: bool) -> Result<(), DbError> {
    let row = queue_row_on(sql)?;
    if row.shuffled == on {
        return Ok(());
    }
    // With nothing playing the mode is all there is to change.
    let Some(cursor) = row.cursor else {
        sql.execute("UPDATE playback_queue SET shuffled = ?", [on])?;
        return Ok(());
    };
    if on {
        // The order shuffle returns to is the lane's order now.
        sql.execute("UPDATE playback_context SET unshuffled = position", [])?;
        reorder_context_after_on(sql, cursor, "random()")?;
    } else {
        reorder_context_after_on(sql, cursor, "unshuffled, position")?;
    }
    sql.execute(
        "UPDATE playback_queue SET shuffled = ?, context_version = context_version + 1",
        [on],
    )?;
    Ok(())
}

fn reorder_on(
    sql: &CovenWrite<'_>,
    id: QueueEntryId,
    before: Option<QueueEntryId>,
) -> Result<Option<u64>, DbError> {
    let Some(moving) = locate_on(sql, id.clone())? else {
        warn!("reorder: queue entry id not found: {id}");
        return Ok(None);
    };
    let to = match before {
        None => None,
        Some(before) => match locate_on(sql, before.clone())? {
            Some(target) if target.lane == moving.lane => Some(target.position),
            Some(_) => {
                warn!(
                    "reorder: entry {id} and the entry {before} it goes before are in different lanes"
                );
                return Ok(None);
            }
            None => {
                warn!("reorder: before id not found: {before}");
                return Ok(None);
            }
        },
    };
    let lane = moving.lane;
    let from = moving.position;
    let top = lane_top_on(sql, lane)?.expect("the lane holds the entry being moved");
    let (_, entry) = row_key(&id);
    let park = |position: i64| {
        sql.execute(
            &format!("UPDATE {table} SET position = ? WHERE entry = ?", table = lane.table()),
            params![position, entry],
        )
    };
    // Where the entry lands, and how a context cursor between the two places
    // moves so it stays on the same track or between the same two.
    let (landing, cursor_shift): (i64, Option<(i64, i64, i64)>) = match to {
        None => {
            park(top + 1)?;
            (top + 1, None)
        }
        Some(to) if to == from => (from, None),
        Some(to) if to < from => {
            park(top + 1)?;
            shift_on(sql, lane, to, from - 1, 1)?;
            park(to)?;
            (to, Some((to, from - 1, 1)))
        }
        Some(to) => {
            park(top + 1)?;
            shift_on(sql, lane, from + 1, to - 1, -1)?;
            park(to - 1)?;
            (to - 1, Some((from + 1, to - 1, -1)))
        }
    };
    if lane == Lane::Context {
        let cursor = queue_row_on(sql)?.cursor.expect("a context lane with an entry has a cursor");
        let moved_cursor = if cursor == from {
            landing
        } else {
            match cursor_shift {
                Some((low, high, delta)) if (low..=high).contains(&cursor) => cursor + delta,
                _ => cursor,
            }
        };
        if moved_cursor != cursor {
            set_cursor_on(sql, moved_cursor)?;
        }
    }
    sql.execute(lane.count_change(), [])?;
    let version: i64 = sql.query_row(
        &format!(
            "SELECT {column} FROM playback_queue",
            column = match lane {
                Lane::Manual => "manual_version",
                Lane::Context => "context_version",
            }
        ),
        [],
        |row| row.get(0),
    )?;
    Ok(Some(version as u64))
}

fn advance_on(sql: &CovenWrite<'_>) -> Result<Option<QueueEntry>, DbError> {
    let manual_front = sql
        .query_row(
            "SELECT entry, track_id, position FROM playback_manual ORDER BY position LIMIT 1",
            [],
            |row| entry_from_row(Lane::Manual, row),
        )
        .optional()?;
    if let Some((entry, position)) = manual_front {
        sql.execute("DELETE FROM playback_manual WHERE position = ?", [position])?;
        sql.execute(MANUAL_CHANGED, [])?;
        return Ok(Some(entry));
    }
    let next = sql
        .query_row(
            "SELECT entry, track_id, position FROM playback_context \
             WHERE position > (SELECT cursor FROM playback_queue) \
             ORDER BY position LIMIT 1",
            [],
            context_from_row,
        )
        .optional()?;
    let Some((entry, position)) = next else {
        return Ok(None);
    };
    set_cursor_on(sql, position)?;
    sql.execute(CONTEXT_CHANGED, [])?;
    Ok(Some(entry))
}

fn wrap_on(sql: &CovenWrite<'_>) -> Result<Option<QueueEntry>, DbError> {
    let row = queue_row_on(sql)?;
    if row.cursor.is_none() {
        return Ok(None);
    }
    if row.shuffled {
        // A new pass in a new order over the rows the lane holds now. The
        // order unshuffling returns to stays the one the lane had when it was
        // shuffled.
        reorder_context_after_on(sql, i64::MIN, "random()")?;
    }
    let (first, position) = sql.query_row(
        "SELECT entry, track_id, position FROM playback_context ORDER BY position LIMIT 1",
        [],
        context_from_row,
    )?;
    set_cursor_on(sql, position)?;
    sql.execute(CONTEXT_CHANGED, [])?;
    Ok(Some(first))
}

fn step_back_on(
    sql: &CovenWrite<'_>,
    current: Option<QueueEntryId>,
) -> Result<Option<QueueEntry>, DbError> {
    let Some(cursor) = queue_row_on(sql)?.cursor else {
        return Ok(None);
    };
    let at_cursor: Option<i64> = sql
        .query_row("SELECT entry FROM playback_context WHERE position = ?", [cursor], |row| {
            row.get(0)
        })
        .optional()?;
    let playing_cursor = matches!(
        (current, at_cursor),
        (Some(QueueEntryId::Context(current)), Some(at_cursor)) if current == at_cursor
    );
    let comparison = if playing_cursor { "<" } else { "<=" };
    let target = sql
        .query_row(
            &format!(
                "SELECT entry, track_id, position FROM playback_context \
                 WHERE position {comparison} ? ORDER BY position DESC LIMIT 1"
            ),
            [cursor],
            context_from_row,
        )
        .optional()?;
    let Some((entry, position)) = target else {
        return Ok(None);
    };
    set_cursor_on(sql, position)?;
    sql.execute(CONTEXT_CHANGED, [])?;
    Ok(Some(entry))
}

pub(crate) fn repeat_column(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::Track => "track",
        RepeatMode::Context => "context",
    }
}

/// The repeat mode `row`'s column `column` stores.
pub(super) fn repeat_from_row<I: rusqlite::RowIndex + Copy>(
    row: &Row<'_>,
    column: I,
) -> rusqlite::Result<RepeatMode> {
    match row.get::<_, String>(column)?.as_str() {
        "off" => Ok(RepeatMode::Off),
        "track" => Ok(RepeatMode::Track),
        "context" => Ok(RepeatMode::Context),
        other => {
            Err(column_conversion_error(row, column, format!("unknown repeat mode {other:?}")))
        }
    }
}

/// The context lane holding the tracks `start..end`, track `i` at place `i`
/// and named by `track_id_sql`, an SQL expression over `i`, and the queue
/// playing the library from before the lane's first track. The lane goes in
/// by one set-based statement, at the sizes the lists' window costs are
/// measured at.
#[cfg(any(test, feature = "test-utils"))]
pub fn fill_context_lane_on(
    sql: &CovenWrite<'_>,
    start: u64,
    end: u64,
    track_id_sql: &str,
) -> Result<(), DbError> {
    let rows = bae_store::test_fixtures::window_cost::numbered(start, end);
    sql.execute_batch(&format!(
        "{rows} INSERT INTO playback_context (entry, position, track_id, unshuffled) \
             SELECT i, i, {track_id_sql}, i FROM k;
         UPDATE playback_queue SET source_kind = 'library', shuffled = 0, cursor = -1;"
    ))?;
    Ok(())
}

impl PlaybackQueueTables {
    /// Write the single device-local `playback_state` row (id = 'current'),
    /// replacing any existing one. Never synced.
    pub async fn save_playback_state(&self, state: &PlaybackResumeRow) -> Result<(), DbError> {
        let state = state.clone();
        self.store
            .call(move |conn| {
                let (current_entry, current_track_id) = match &state.current {
                    Some(current) => (Some(current.id.to_string()), Some(current.track_id.clone())),
                    None => (None, None),
                };
                conn.execute(
                    "INSERT OR REPLACE INTO playback_state \
                     (id, current_entry, current_track_id, position_ms) \
                     VALUES ('current', ?, ?, ?)",
                    params![
                        current_entry,
                        current_track_id,
                        state.position_ms.map(|position| position as i64),
                    ],
                )
                .map(|_| ())
                .map_err(DbError::from)
            })
            .await
    }

    /// Delete the device-local `playback_state` row (playback stopped).
    ///
    /// Stopping is persisted on every queue change and every stop, so most
    /// calls arrive with the row already gone. That case is read, not written:
    /// a delete over an empty table is a transaction that changes nothing, and
    /// those belong off the sync journal.
    pub async fn clear_playback_state(&self) -> Result<(), DbError> {
        let stored = self
            .store
            .read(move |sql| {
                Ok(sql
                    .query_row("SELECT 1 FROM playback_state", [], |_| Ok(()))
                    .optional()?
                    .is_some())
            })
            .await?;
        if !stored {
            return Ok(());
        }
        self.store
            .call(move |conn| {
                conn.execute("DELETE FROM playback_state", []).map(|_| ()).map_err(DbError::from)
            })
            .await
    }
}

/// Plant damage for a test: every write of the queue row of the library
/// stored under `directory` fails, as a full disk fails it. The queue row
/// counts every change to either lane and holds the repeat mode and the
/// context lane's cursor, so each queue command that writes fails.
#[cfg(any(test, feature = "test-utils"))]
pub fn fail_queue_writes_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "playback_queue",
        "CREATE TRIGGER fail_playback_queue_write BEFORE UPDATE ON playback_queue \
         BEGIN SELECT RAISE(ABORT, 'the disk is full'); END;",
    );
}

/// Plant damage for a test: the table of Up Next is gone from the library
/// stored under `directory`, so the queue can't say which track follows.
#[cfg(any(test, feature = "test-utils"))]
pub fn lose_up_next_for_test(directory: &std::path::Path) {
    bae_store::test_fixtures::planted_damage::plant_in_library_file(
        directory,
        "playback_manual",
        "DROP TABLE playback_manual;",
    );
}

#[cfg(test)]
#[path = "queue_tests.rs"]
mod tests;
