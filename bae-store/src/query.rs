//! The queries every domain's reads and writes share: the statement-size limit,
//! integer conversions, and the traits that let one reader run in coven's read
//! and write contexts.

use crate::DbError;
use coven::{CovenWrite, SqlReadContext};
#[cfg(any(test, feature = "test-utils"))]
use rusqlite::Connection;
use rusqlite::{Params, Row};

pub const SQL_MAX_IN_VARS: usize = 900;

/// `value` (a count, a size, a position) as the integer SQLite stores, or the
/// failure naming `what` when it is past SQLite's signed range: the write
/// fails rather than store a wrapped number.
pub fn to_i64<V>(value: V, what: &str) -> Result<i64, DbError>
where
    V: TryInto<i64, Error = std::num::TryFromIntError> + std::fmt::Display + Copy,
{
    value.try_into().map_err(|error| {
        DbError::Message(format!("{what} {value} exceeds SQLite's integer range: {error}"))
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

pub fn in_clause_placeholders(count: usize) -> String {
    (0..count).map(|_| "?").collect::<Vec<_>>().join(",")
}

/// One-row SELECT shared by coven's write [`CovenWrite`] and its read
/// [`SqlReadContext`], used when artist resolution runs inside either context.
pub trait QueryOne {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T>;
}

/// Multi-row SELECT shared by coven's read and write SQL contexts. Kept
/// separate from [`QueryOne`] so callers require only the queries they use.
pub trait QueryRows {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>>;
}

/// The resolvers' unit tests run them against a bare seeded connection, which is
/// the only place bae still holds one — coven owns every production connection.
#[cfg(any(test, feature = "test-utils"))]
impl QueryOne for Connection {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T> {
        Connection::query_row(self, sql, params, f)
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl QueryRows for Connection {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>> {
        let mut statement = self.prepare(sql)?;
        let values = statement.query_map(params, f)?.collect();
        values
    }
}

impl QueryOne for SqlReadContext<'_> {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T> {
        SqlReadContext::query_row(self, sql, params, f)
    }
}

impl QueryRows for SqlReadContext<'_> {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>> {
        SqlReadContext::query(self, sql, params, f)
    }
}

impl QueryOne for CovenWrite<'_> {
    fn query_row<T, P: Params, F: FnOnce(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<T> {
        CovenWrite::query_row(self, sql, params, f)
    }
}

impl QueryRows for CovenWrite<'_> {
    fn query<T, P: Params, F: FnMut(&Row<'_>) -> rusqlite::Result<T>>(
        &self,
        sql: &str,
        params: P,
        f: F,
    ) -> rusqlite::Result<Vec<T>> {
        CovenWrite::query(self, sql, params, f)
    }
}
