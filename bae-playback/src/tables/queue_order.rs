//! The places of the queue's lanes: moving an entry within its lane, which
//! shifts the places between where it left and where it lands, and a context
//! lane's cursor with them.

use super::queue::{locate_on, queue_row_on, row_key};
use super::*;
use crate::{QueueEntryId, QueueLane};
use bae_store::WriteStatements;

/// An order a lane's rows have places in: Up Next's one order, or one of the
/// context lane's two.
#[derive(Clone, Copy)]
pub(super) enum LaneOrder {
    UpNext,
    Ordered,
    Shuffled,
}

impl LaneOrder {
    fn lane(self) -> QueueLane {
        match self {
            Self::UpNext => QueueLane::Manual,
            Self::Ordered | Self::Shuffled => QueueLane::Context,
        }
    }

    /// The column of the lane's table that holds a row's place in the order.
    fn column(self) -> &'static str {
        match self {
            Self::UpNext => "position",
            Self::Ordered => "ordered",
            Self::Shuffled => "shuffled",
        }
    }

    /// The queue row's column of the playing row's place in the order; none
    /// for Up Next, which has no playing row.
    fn cursor_column(self) -> Option<&'static str> {
        match self {
            Self::UpNext => None,
            Self::Ordered => Some("ordered_cursor"),
            Self::Shuffled => Some("shuffled_cursor"),
        }
    }

    /// Move the rows at places `low..=high` up by `lift`.
    fn lift_on(self, sql: &CovenWrite<'_>, lift: i64, low: i64, high: i64) -> rusqlite::Result<()> {
        match self {
            Self::UpNext => sql.run(&LiftUpNextParams { lift, low, high })?,
            Self::Ordered => sql.run(&LiftOrderedParams { lift, low, high })?,
            Self::Shuffled => sql.run(&LiftShuffledParams { lift, low, high })?,
        };
        Ok(())
    }

    /// Move the rows at places from `from` up down by `lift`, and then by
    /// `delta`.
    fn lower_on(
        self,
        sql: &CovenWrite<'_>,
        lift: i64,
        delta: i64,
        from: i64,
    ) -> rusqlite::Result<()> {
        match self {
            Self::UpNext => sql.run(&LowerUpNextParams { lift, delta, from })?,
            Self::Ordered => sql.run(&LowerOrderedParams { lift, delta, from })?,
            Self::Shuffled => sql.run(&LowerShuffledParams { lift, delta, from })?,
        };
        Ok(())
    }

    /// Put the row `entry` at `place`.
    fn park_on(self, sql: &CovenWrite<'_>, entry: i64, place: i64) -> rusqlite::Result<()> {
        match self {
            Self::UpNext => sql.run(&PlaceInUpNextParams { entry, place })?,
            Self::Ordered => sql.run(&PlaceInOrderedParams { entry, place })?,
            Self::Shuffled => sql.run(&PlaceInShuffledParams { entry, place })?,
        };
        Ok(())
    }

    /// Put the order's cursor at `place`; Up Next has no cursor to put.
    fn move_cursor_on(self, sql: &CovenWrite<'_>, place: i64) -> rusqlite::Result<()> {
        match self {
            Self::UpNext => {}
            Self::Ordered => {
                sql.run(&SetOrderedCursorParams { place })?;
            }
            Self::Shuffled => {
                sql.run(&SetShuffledCursorParams { place })?;
            }
        }
        Ok(())
    }
}

coven::write!(
    LiftUpNext(lift: i64, low: i64, high: i64),
    r#"
    UPDATE playback_manual SET position = position + :lift
    WHERE position BETWEEN :low AND :high
"#
);

coven::write!(
    LiftOrdered(lift: i64, low: i64, high: i64),
    r#"
    UPDATE playback_context SET ordered = ordered + :lift
    WHERE ordered BETWEEN :low AND :high
"#
);

coven::write!(
    LiftShuffled(lift: i64, low: i64, high: i64),
    r#"
    UPDATE playback_context SET shuffled = shuffled + :lift
    WHERE shuffled BETWEEN :low AND :high
"#
);

coven::write!(
    LowerUpNext(lift: i64, delta: i64, from: i64),
    r#"
    UPDATE playback_manual SET position = position - :lift + :delta
    WHERE position >= :from
"#
);

coven::write!(
    LowerOrdered(lift: i64, delta: i64, from: i64),
    r#"
    UPDATE playback_context SET ordered = ordered - :lift + :delta
    WHERE ordered >= :from
"#
);

coven::write!(
    LowerShuffled(lift: i64, delta: i64, from: i64),
    r#"
    UPDATE playback_context SET shuffled = shuffled - :lift + :delta
    WHERE shuffled >= :from
"#
);

coven::write!(
    PlaceInUpNext(entry: i64, place: i64),
    r#"UPDATE playback_manual SET position = :place WHERE entry = :entry"#
);

coven::write!(
    PlaceInOrdered(entry: i64, place: i64),
    r#"UPDATE playback_context SET ordered = :place WHERE entry = :entry"#
);

coven::write!(
    PlaceInShuffled(entry: i64, place: i64),
    r#"UPDATE playback_context SET shuffled = :place WHERE entry = :entry"#
);

coven::write!(SetOrderedCursor(place: i64), r#"UPDATE playback_queue SET ordered_cursor = :place"#);

coven::write!(
    SetShuffledCursor(place: i64),
    r#"UPDATE playback_queue SET shuffled_cursor = :place"#
);

/// The highest place in `order`, `None` when its lane is empty.
pub(super) fn lane_top_on(sql: &CovenWrite<'_>, order: LaneOrder) -> rusqlite::Result<Option<i64>> {
    let (table, column) = (order.lane().table(), order.column());
    sql.query_row(&format!("SELECT max({column}) FROM {table}"), [], |row| row.get(0))
}

/// Move the rows at places `low..=high` of `order` by `delta`. Places are
/// unique in a lane at every statement, so the rows first go above every place
/// the lane holds, then come down to where they belong. The caller has left
/// the places they come down to free.
pub(super) fn shift_on(
    sql: &CovenWrite<'_>,
    order: LaneOrder,
    low: i64,
    high: i64,
    delta: i64,
) -> rusqlite::Result<()> {
    if low > high {
        return Ok(());
    }
    let Some(top) = lane_top_on(sql, order)? else {
        return Ok(());
    };
    let lift = top - low + delta.abs() + 1;
    order.lift_on(sql, lift, low, high)?;
    order.lower_on(sql, lift, delta, low + lift)
}

pub(super) fn reorder_on(
    sql: &CovenWrite<'_>,
    id: QueueEntryId,
    target: QueueLane,
    position: u64,
) -> Result<bool, DbError> {
    let Some(moving) = locate_on(sql, id.clone())? else {
        warn!("reorder: queue entry id not found: {id}");
        return Ok(false);
    };
    let lane = moving.lane;
    if lane != target {
        warn!("reorder: entry {id} is not in the {target:?} lane the caller asked to move it in");
        return Ok(false);
    }
    let (_, entry) = row_key(&id);
    let before = entry_after_place_on(sql, lane, entry, position)?;
    match lane {
        QueueLane::Manual => move_entry_on(sql, LaneOrder::UpNext, entry, before)?,
        QueueLane::Context => {
            // The row moves in its own order, and also in the shuffled one
            // while shuffle is on, so a move made while shuffled is still
            // there when shuffle is off.
            move_entry_on(sql, LaneOrder::Ordered, entry, before)?;
            if queue_row_on(sql)?.shuffled {
                move_entry_on(sql, LaneOrder::Shuffled, entry, before)?;
            }
        }
    }
    Ok(true)
}

/// Find the entry that a row moved to `position` goes before. The function
/// counts `lane`'s rows in the order the screens show them, skips the moved
/// row `entry`, and returns none when `position` is past the lane's end.
fn entry_after_place_on(
    sql: &CovenWrite<'_>,
    lane: QueueLane,
    entry: i64,
    position: u64,
) -> Result<Option<i64>, DbError> {
    let offset = i64::try_from(position).unwrap_or(i64::MAX);
    let shuffled = queue_row_on(sql)?.shuffled;
    // The context lane's rows as the screens show them start after its
    // playing track; Up Next shows every row.
    match lane {
        QueueLane::Manual => Ok(sql
            .query_row(
                "SELECT entry FROM playback_manual WHERE entry <> ?1 \
                 ORDER BY position LIMIT 1 OFFSET ?2",
                params![entry, offset],
                |row| row.get(0),
            )
            .optional()?),
        QueueLane::Context => {
            super::queue_upcoming::upcoming_entry_at_on(sql, shuffled, entry, offset)
        }
    }
}

/// Move the entry to just before `before`, or to the end of its lane without
/// one, in the order `order`. A context order's cursor moves so it stays on the
/// same track or between the same two.
fn move_entry_on(
    sql: &CovenWrite<'_>,
    order: LaneOrder,
    entry: i64,
    before: Option<i64>,
) -> rusqlite::Result<()> {
    let (table, column) = (order.lane().table(), order.column());
    let place_of = |entry: i64| {
        sql.query_row(&format!("SELECT {column} FROM {table} WHERE entry = ?"), [entry], |row| {
            row.get::<_, i64>(0)
        })
    };
    let from = place_of(entry)?;
    let to = before.map(place_of).transpose()?;
    let top = lane_top_on(sql, order)?.expect("the lane holds the entry being moved");
    let park = |place: i64| order.park_on(sql, entry, place);
    // Where the entry lands, and how a cursor between the two places moves.
    let (landing, cursor_shift): (i64, Option<(i64, i64, i64)>) = match to {
        None => {
            park(top + 1)?;
            (top + 1, None)
        }
        Some(to) if to == from => (from, None),
        Some(to) if to < from => {
            park(top + 1)?;
            shift_on(sql, order, to, from - 1, 1)?;
            park(to)?;
            (to, Some((to, from - 1, 1)))
        }
        Some(to) => {
            park(top + 1)?;
            shift_on(sql, order, from + 1, to - 1, -1)?;
            park(to - 1)?;
            (to - 1, Some((from + 1, to - 1, -1)))
        }
    };
    if let Some(cursor_column) = order.cursor_column() {
        let cursor: i64 =
            sql.query_row(&format!("SELECT {cursor_column} FROM playback_queue"), [], |row| {
                row.get(0)
            })?;
        let moved_cursor = if cursor == from {
            landing
        } else {
            match cursor_shift {
                Some((low, high, delta)) if (low..=high).contains(&cursor) => cursor + delta,
                _ => cursor,
            }
        };
        if moved_cursor != cursor {
            order.move_cursor_on(sql, moved_cursor)?;
        }
    }
    Ok(())
}
