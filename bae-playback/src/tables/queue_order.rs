//! The places of the queue's lanes, read and changed in each order a lane has:
//! the highest place, an entry's place, and moving an entry within its lane,
//! which shifts the places between where it left and where it lands, and a
//! context lane's cursor with them.

use super::queue::{locate_on, queue_row_on, row_key, QueueCursors};
use super::*;
use crate::{QueueEntryId, QueueLane};
use bae_store::{statement_optional, statement_row, WriteStatements, WriterRead};

/// An order a lane's rows have places in: Up Next's one order, or one of the
/// context lane's two.
#[derive(Clone, Copy)]
pub(super) enum LaneOrder {
    UpNext,
    Ordered,
    Shuffled,
}

impl LaneOrder {
    /// The order `lane`'s rows read in: Up Next's, or the context lane's
    /// shuffled one when `shuffled` says so and its own otherwise.
    pub(super) fn of(lane: QueueLane, shuffled: bool) -> Self {
        match (lane, shuffled) {
            (QueueLane::Manual, _) => Self::UpNext,
            (QueueLane::Context, false) => Self::Ordered,
            (QueueLane::Context, true) => Self::Shuffled,
        }
    }

    /// The highest place in the order, `None` when its lane is empty.
    pub(super) fn top_on(self, sql: &impl WriterRead) -> Result<Option<i64>, DbError> {
        Ok(match self {
            Self::UpNext => statement_row(sql, &UpNextTopParams {})?.top,
            Self::Ordered => statement_row(sql, &OrderedTopParams {})?.top,
            Self::Shuffled => statement_row(sql, &ShuffledTopParams {})?.top,
        })
    }

    /// The row `entry`'s place in the order and its track; none when the lane
    /// holds no such row.
    pub(super) fn entry_on(
        self,
        sql: &impl WriterRead,
        entry: i64,
    ) -> Result<Option<(i64, String)>, DbError> {
        Ok(match self {
            Self::UpNext => statement_optional(sql, &UpNextEntryPlaceParams { entry })?
                .map(|row| (row.place, row.track_id)),
            Self::Ordered => statement_optional(sql, &OrderedEntryPlaceParams { entry })?
                .map(|row| (row.place, row.track_id)),
            Self::Shuffled => statement_optional(sql, &ShuffledEntryPlaceParams { entry })?
                .map(|row| (row.place, row.track_id)),
        })
    }

    /// The playing row's place in the order, among `cursors`; Up Next has no
    /// playing row.
    fn cursor(self, cursors: &QueueCursors) -> Option<i64> {
        match self {
            Self::UpNext => None,
            Self::Ordered => Some(cursors.ordered),
            Self::Shuffled => Some(cursors.shuffled),
        }
    }

    /// Move the rows at places `low..=high` up by `lift`.
    fn lift_on(self, sql: &CovenWrite<'_>, lift: i64, low: i64, high: i64) -> Result<(), DbError> {
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
    ) -> Result<(), DbError> {
        match self {
            Self::UpNext => sql.run(&LowerUpNextParams { lift, delta, from })?,
            Self::Ordered => sql.run(&LowerOrderedParams { lift, delta, from })?,
            Self::Shuffled => sql.run(&LowerShuffledParams { lift, delta, from })?,
        };
        Ok(())
    }

    /// Put the row `entry` at `place`.
    fn park_on(self, sql: &CovenWrite<'_>, entry: i64, place: i64) -> Result<(), DbError> {
        match self {
            Self::UpNext => sql.run(&PlaceInUpNextParams { entry, place })?,
            Self::Ordered => sql.run(&PlaceInOrderedParams { entry, place })?,
            Self::Shuffled => sql.run(&PlaceInShuffledParams { entry, place })?,
        };
        Ok(())
    }

    /// Put the order's cursor at `place`; Up Next has no cursor to put.
    fn move_cursor_on(self, sql: &CovenWrite<'_>, place: i64) -> Result<(), DbError> {
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

// The highest place in each order, NULL when its lane is empty.
coven::read!(
    writer UpNextTop { top: Option<i64> },
    r#"SELECT max(manual.position) AS top FROM playback_manual AS manual"#
);

coven::read!(
    writer OrderedTop { top: Option<i64> },
    r#"SELECT max(lane.ordered) AS top FROM playback_context AS lane"#
);

coven::read!(
    writer ShuffledTop { top: Option<i64> },
    r#"SELECT max(lane.shuffled) AS top FROM playback_context AS lane"#
);

// The row `entry`'s place in each order, and its track.
coven::read!(
    writer UpNextEntryPlace(entry: i64),
    r#"
    SELECT manual.position AS place, manual.track_id
    FROM playback_manual AS manual
    WHERE manual.entry = :entry
"#
);

coven::read!(
    writer OrderedEntryPlace(entry: i64),
    r#"
    SELECT lane.ordered AS place, lane.track_id
    FROM playback_context AS lane
    WHERE lane.entry = :entry
"#
);

coven::read!(
    writer ShuffledEntryPlace(entry: i64),
    r#"
    SELECT lane.shuffled AS place, lane.track_id
    FROM playback_context AS lane
    WHERE lane.entry = :entry
"#
);

// The entry at `offset`, counted from 0, among Up Next's rows other than
// `moved`.
coven::read!(
    writer UpNextEntryAt(moved: i64, offset: i64),
    r#"
    SELECT manual.entry
    FROM playback_manual AS manual
    WHERE manual.entry <> :moved
    ORDER BY manual.position
    LIMIT 1 OFFSET :offset
"#
);

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
) -> Result<(), DbError> {
    if low > high {
        return Ok(());
    }
    let Some(top) = order.top_on(sql)? else {
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
        QueueLane::Manual => {
            Ok(statement_optional(sql, &UpNextEntryAtParams { moved: entry, offset })?
                .map(|row| row.entry))
        }
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
) -> Result<(), DbError> {
    let place_of = |entry: i64| -> Result<i64, DbError> {
        let (place, _) = order
            .entry_on(sql, entry)?
            .ok_or_else(|| DbError::unreadable("the place of a queue entry being moved", entry))?;
        Ok(place)
    };
    let from = place_of(entry)?;
    let to = before.map(place_of).transpose()?;
    let top = order.top_on(sql)?.expect("the lane holds the entry being moved");
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
    // A context lane that holds the moved entry has a playing row.
    let cursor = match order {
        LaneOrder::UpNext => None,
        LaneOrder::Ordered | LaneOrder::Shuffled => {
            let cursors = queue_row_on(sql)?.cursors.ok_or_else(|| {
                DbError::unreadable("the cursors of a context lane holding an entry", "NULL")
            })?;
            order.cursor(&cursors)
        }
    };
    if let Some(cursor) = cursor {
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
