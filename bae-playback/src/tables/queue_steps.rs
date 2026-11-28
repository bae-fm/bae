//! The context lane read in the order it plays, its own or the shuffled one:
//! the entry after the playing one, the first, the one at a place and the one
//! before a place, each a read of its own for each order.

use super::*;
use crate::{QueueEntry, QueueEntryId};
use bae_store::{statement_optional, WriterRead};

/// A context lane row as a queue entry.
fn context_entry(entry: i64, track_id: String) -> QueueEntry {
    QueueEntry { id: QueueEntryId::Context(entry), track_id }
}

// The context lane's first entry after the place `cursor`, in each order; none
// without a cursor.
coven::read!(
    writer OrderedAfter(cursor: Option<i64>),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.ordered > :cursor
    ORDER BY lane.ordered
    LIMIT 1
"#
);

coven::read!(
    writer ShuffledAfter(cursor: Option<i64>),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.shuffled > :cursor
    ORDER BY lane.shuffled
    LIMIT 1
"#
);

// The context lane's first entry, in each order.
coven::read!(
    writer OrderedFirst,
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    ORDER BY lane.ordered
    LIMIT 1
"#
);

coven::read!(
    writer ShuffledFirst,
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    ORDER BY lane.shuffled
    LIMIT 1
"#
);

// The context lane's entry at the place `place`, in each order.
coven::read!(
    writer OrderedAt(place: i64),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.ordered = :place
"#
);

coven::read!(
    writer ShuffledAt(place: i64),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.shuffled = :place
"#
);

// The context lane's last entry before the place `place`, in each order.
coven::read!(
    writer OrderedBefore(place: i64),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.ordered < :place
    ORDER BY lane.ordered DESC
    LIMIT 1
"#
);

coven::read!(
    writer ShuffledBefore(place: i64),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.shuffled < :place
    ORDER BY lane.shuffled DESC
    LIMIT 1
"#
);

// The context lane's last entry at or before the place `place`, in each order.
coven::read!(
    writer OrderedThrough(place: i64),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.ordered <= :place
    ORDER BY lane.ordered DESC
    LIMIT 1
"#
);

coven::read!(
    writer ShuffledThrough(place: i64),
    r#"
    SELECT lane.entry, lane.track_id
    FROM playback_context AS lane
    WHERE lane.shuffled <= :place
    ORDER BY lane.shuffled DESC
    LIMIT 1
"#
);

/// The context lane's first entry after the place `cursor` in the shuffled
/// order when `shuffled` says so, else in its own; none without a cursor or a
/// later entry.
pub(super) fn after_on(
    sql: &impl WriterRead,
    shuffled: bool,
    cursor: Option<i64>,
) -> Result<Option<QueueEntry>, DbError> {
    Ok(if shuffled {
        statement_optional(sql, &ShuffledAfterParams { cursor })?
            .map(|row| context_entry(row.entry, row.track_id))
    } else {
        statement_optional(sql, &OrderedAfterParams { cursor })?
            .map(|row| context_entry(row.entry, row.track_id))
    })
}

/// The context lane's first entry in the order `shuffled` names; none when the
/// lane is empty.
pub(super) fn first_on(
    sql: &impl WriterRead,
    shuffled: bool,
) -> Result<Option<QueueEntry>, DbError> {
    Ok(if shuffled {
        statement_optional(sql, &ShuffledFirstParams {})?
            .map(|row| context_entry(row.entry, row.track_id))
    } else {
        statement_optional(sql, &OrderedFirstParams {})?
            .map(|row| context_entry(row.entry, row.track_id))
    })
}

/// The context lane's entry at `place` in the order `shuffled` names.
pub(super) fn at_on(
    sql: &impl WriterRead,
    shuffled: bool,
    place: i64,
) -> Result<Option<QueueEntry>, DbError> {
    Ok(if shuffled {
        statement_optional(sql, &ShuffledAtParams { place })?
            .map(|row| context_entry(row.entry, row.track_id))
    } else {
        statement_optional(sql, &OrderedAtParams { place })?
            .map(|row| context_entry(row.entry, row.track_id))
    })
}

/// The context lane's last entry before `place` in the order `shuffled`
/// names, or at it too when `through` says so.
pub(super) fn before_on(
    sql: &impl WriterRead,
    shuffled: bool,
    place: i64,
    through: bool,
) -> Result<Option<QueueEntry>, DbError> {
    Ok(match (shuffled, through) {
        (false, false) => statement_optional(sql, &OrderedBeforeParams { place })?
            .map(|row| context_entry(row.entry, row.track_id)),
        (false, true) => statement_optional(sql, &OrderedThroughParams { place })?
            .map(|row| context_entry(row.entry, row.track_id)),
        (true, false) => statement_optional(sql, &ShuffledBeforeParams { place })?
            .map(|row| context_entry(row.entry, row.track_id)),
        (true, true) => statement_optional(sql, &ShuffledThroughParams { place })?
            .map(|row| context_entry(row.entry, row.track_id)),
    })
}
