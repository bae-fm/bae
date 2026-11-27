//! Running a generated `write!` statement in a write's transaction, by the
//! write itself or by the before-commit function coven runs last in it.

use crate::read_statement::row_count;
use crate::DbError;
use coven::{CommitSql, CovenWrite, JoiningWrite};
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
        let mut rows = self.returning(statement)?.into_iter();
        match (rows.next(), rows.count()) {
            (Some(row), 0) => Ok(row),
            (first, more) => Err(row_count::<Q>(usize::from(first.is_some()) + more)),
        }
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

write_statements!(CommitSql, CovenWrite, JoiningWrite);
