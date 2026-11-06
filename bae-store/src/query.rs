//! The queries every domain's reads and writes share: the statement-size limit
//! and integer conversions.

use crate::DbError;

pub const SQL_MAX_IN_VARS: usize = 900;

/// `value` (a count, a size, a position) as the integer SQLite stores, or the
/// failure naming `what` when it is past SQLite's signed range: the write
/// fails rather than store a wrapped number.
pub fn to_i64<V>(value: V, what: &str) -> Result<i64, DbError>
where
    V: TryInto<i64, Error = std::num::TryFromIntError> + std::fmt::Display + Copy,
{
    value.try_into().map_err(|_out_of_range| DbError::OutOfRange {
        what: what.to_string(),
        value: value.to_string(),
    })
}

/// A stored count, size or position read back: the schema keeps it at zero
/// or above, so a negative one is a stored value no writer produces.
pub fn to_u64(value: i64, what: &str) -> Result<u64, DbError> {
    u64::try_from(value).map_err(|_negative| DbError::unreadable(what, value))
}

/// [`to_u64`] of a count that fits a `u32`; one past it is a stored value no
/// writer produces.
pub fn to_u32(value: i64, what: &str) -> Result<u32, DbError> {
    u32::try_from(value).map_err(|_out_of_range| DbError::unreadable(what, value))
}

/// [`to_u64`] of a count that fits a `usize`.
pub fn to_usize(value: i64, what: &str) -> Result<usize, DbError> {
    usize::try_from(value).map_err(|_out_of_range| DbError::unreadable(what, value))
}

pub fn in_clause_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(",")
}
