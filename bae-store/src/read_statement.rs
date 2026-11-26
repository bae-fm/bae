//! Running a generated `read!` statement in any read context, with the number
//! of rows its caller expects: a one-shot read, a live query and a write's
//! transaction run the same statement.

use crate::DbError;
use coven::SqlRead;
use sqlive_compose::Read;

/// Every row each of two reads returned.
pub type RowsOfBoth<A, B> =
    (Vec<<A as sqlive_compose::Returning>::Row>, Vec<<B as sqlive_compose::Returning>::Row>);

/// Every row `statement` returns on `sql`.
pub fn statement_rows<Q: Read>(sql: &impl SqlRead, statement: &Q) -> Result<Vec<Q::Row>, DbError> {
    Ok(sql.query(Q::SQL, statement.params().as_slice(), Q::read)?)
}

/// The row `statement` returns on `sql`, for a statement that returns at most
/// one; a second row is an error.
pub fn statement_optional<Q: Read>(
    sql: &impl SqlRead,
    statement: &Q,
) -> Result<Option<Q::Row>, DbError> {
    let mut rows = statement_rows(sql, statement)?.into_iter();
    let first = rows.next();
    match rows.count() {
        0 => Ok(first),
        more => Err(row_count::<Q>(1 + more)),
    }
}

/// The row `statement` returns on `sql`, for a statement that returns exactly
/// one; any other number of rows is an error.
pub fn statement_row<Q: Read>(sql: &impl SqlRead, statement: &Q) -> Result<Q::Row, DbError> {
    statement_optional(sql, statement)?.ok_or_else(|| row_count::<Q>(0))
}

/// A read returned `rows` rows, a number its statement never returns.
fn row_count<Q: Read>(rows: usize) -> DbError {
    DbError::unreadable(format!("the result of {}", std::any::type_name::<Q>()), rows)
}

#[cfg(test)]
#[path = "read_statement_tests.rs"]
mod tests;
