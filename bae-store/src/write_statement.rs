//! Running a generated `write!` statement in a write's transaction or a bulk
//! write's, by the write itself or by the before-commit function coven runs
//! last in it.

use crate::read_statement::{at_most_one, row_count};
use crate::DbError;
use coven::{BulkSqlContext, CommitSql, CovenWrite, JoiningWrite, MemberWrite};
use sqlive_compose::{Write, WriteReturning};

/// A context that writes, which runs generated writes.
pub trait WriteStatements {
    /// Run `statement`, and return how many rows it changed.
    fn run<Q: Write>(&self, statement: &Q) -> rusqlite::Result<usize>;

    /// Run `statement`, and return every row its RETURNING gives.
    fn returning<Q: WriteReturning>(&self, statement: &Q) -> rusqlite::Result<Vec<Q::Row>>;

    /// Run `statement`, whose RETURNING gives exactly one row, and return
    /// it; any other number of rows is an error.
    fn returning_one<Q: WriteReturning>(&self, statement: &Q) -> Result<Q::Row, DbError> {
        self.returning_optional(statement)?.ok_or_else(|| row_count::<Q>(0))
    }

    /// Run `statement`, which changes at most one row, and return the row
    /// its RETURNING gives; a second row is an error.
    fn returning_optional<Q: WriteReturning>(
        &self,
        statement: &Q,
    ) -> Result<Option<Q::Row>, DbError> {
        at_most_one::<Q, _>(self.returning(statement)?)
    }
}

/// [`WriteStatements`] for each of coven's contexts, which run SQL by the
/// same methods and share no trait for them.
macro_rules! write_statements {
    ($($context:ident),+) => {$(
        impl WriteStatements for $context<'_> {
            fn run<Q: Write>(&self, statement: &Q) -> rusqlite::Result<usize> {
                self.execute(Q::SQL, statement.params().as_slice())
            }

            fn returning<Q: WriteReturning>(&self, statement: &Q) -> rusqlite::Result<Vec<Q::Row>> {
                self.query(Q::SQL, statement.params().as_slice(), Q::read)
            }
        }
    )+};
}

write_statements!(BulkSqlContext, CommitSql, CovenWrite, JoiningWrite, MemberWrite);
