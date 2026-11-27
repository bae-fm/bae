//! Running a generated `read!` statement in any read context, with the number
//! of rows its caller expects: a one-shot read, a live query and a write's
//! transaction run the same statement.

use crate::DbError;
use coven::SqlRead;
use sqlive_compose::Read;
use sqlive_reads::Writer;

/// A context that reads on the store's writer, as every one of bae's does: a
/// read's, a write's or a bulk write's. A generated read runs on one when its
/// call marks it `writer`.
pub trait WriterRead: SqlRead<Handle = Writer> {}

impl<T: SqlRead<Handle = Writer>> WriterRead for T {}

/// Every row each of two reads returned.
pub type RowsOfBoth<A, B> =
    (Vec<<A as sqlive_compose::Returning>::Row>, Vec<<B as sqlive_compose::Returning>::Row>);

/// Every row `statement` returns on `sql`, run with the scope plan bae's build
/// compiled for it, so a live query's first run builds none. It fails with
/// SQLite's own error, which a caller that answers coven and one that answers
/// the store both take.
pub fn statement_rows<Q: Read<Handle = Writer>>(
    sql: &impl WriterRead,
    statement: &Q,
) -> rusqlite::Result<Vec<Q::Row>> {
    sql.query_compiled(Q::compiled(), statement.params().as_slice(), Q::read)
}

/// The row `statement` returns on `sql`, for a statement that returns at most
/// one; a second row is an error.
pub fn statement_optional<Q: Read<Handle = Writer>>(
    sql: &impl WriterRead,
    statement: &Q,
) -> Result<Option<Q::Row>, DbError> {
    at_most_one::<Q, _>(statement_rows(sql, statement)?)
}

/// The row among `rows`, all that the statement `Q` returned, where it
/// returns at most one; a second row is an error.
pub(crate) fn at_most_one<Q, Row>(rows: Vec<Row>) -> Result<Option<Row>, DbError> {
    let mut rows = rows.into_iter();
    let first = rows.next();
    match rows.count() {
        0 => Ok(first),
        more => Err(row_count::<Q>(1 + more)),
    }
}

/// The row `statement` returns on `sql`, for a statement that returns exactly
/// one; any other number of rows is an error.
pub fn statement_row<Q: Read<Handle = Writer>>(
    sql: &impl WriterRead,
    statement: &Q,
) -> Result<Q::Row, DbError> {
    statement_optional(sql, statement)?.ok_or_else(|| row_count::<Q>(0))
}

/// A statement returned `rows` rows, a number it never returns.
pub(crate) fn row_count<Q>(rows: usize) -> DbError {
    DbError::unreadable(format!("the result of {}", std::any::type_name::<Q>()), rows)
}

#[cfg(test)]
#[path = "read_statement_tests.rs"]
mod tests;
