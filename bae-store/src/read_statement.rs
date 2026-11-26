//! A read as the fragment compiler writes it: one SQL statement with named
//! parameters, the values its request binds to them, and the mapping of each
//! row it returns. A live query that runs a [`ReadStatement`] runs exactly
//! that statement.

use crate::DbError;
use coven::SqlReadContext;

/// The values a request binds to a statement's parameters, by the names the
/// statement gives them.
pub type Bound<'request> = Vec<(&'static str, &'request dyn rusqlite::ToSql)>;

/// One read: its SQL, how a request `P` binds the SQL's parameters, and how a
/// row of its result becomes an `R`. The three are the generated items of one
/// `read!` (`SQL`, the parameters struct's `params`, the row's `read`).
pub struct ReadStatement<P, R> {
    sql: &'static str,
    parameters: for<'request> fn(&'request P) -> Bound<'request>,
    row: fn(&rusqlite::Row<'_>) -> rusqlite::Result<R>,
}

impl<P, R> ReadStatement<P, R> {
    /// The statement `sql`, whose parameters `parameters` binds from a request
    /// and whose rows `row` reads.
    pub const fn new(
        sql: &'static str,
        parameters: for<'request> fn(&'request P) -> Bound<'request>,
        row: fn(&rusqlite::Row<'_>) -> rusqlite::Result<R>,
    ) -> Self {
        Self { sql, parameters, row }
    }

    /// Run the statement for `request` on `read`: every row it returns.
    pub(crate) fn run(&self, read: &SqlReadContext<'_>, request: &P) -> Result<Vec<R>, DbError> {
        let bound = (self.parameters)(request);
        Ok(read.query(self.sql, bound.as_slice(), self.row)?)
    }
}

#[cfg(test)]
#[path = "read_statement_tests.rs"]
mod tests;
