//! Running a generated `write!` statement in a write's transaction or a bulk
//! write's.

use coven::{BulkSqlContext, CovenWrite};
use sqlive_compose::{Write, WriteReturning};

/// A write's context, which runs generated writes.
pub trait WriteStatements {
    /// Run `statement`, and return how many rows it changed.
    fn run<Q: Write>(&self, statement: &Q) -> rusqlite::Result<usize>;

    /// Run `statement`, and return every row its RETURNING gives.
    fn returning<Q: WriteReturning>(&self, statement: &Q) -> rusqlite::Result<Vec<Q::Row>>;
}

impl WriteStatements for CovenWrite<'_> {
    fn run<Q: Write>(&self, statement: &Q) -> rusqlite::Result<usize> {
        self.execute(Q::SQL, statement.params().as_slice())
    }

    fn returning<Q: WriteReturning>(&self, statement: &Q) -> rusqlite::Result<Vec<Q::Row>> {
        self.query(Q::SQL, statement.params().as_slice(), Q::read)
    }
}

impl WriteStatements for BulkSqlContext<'_> {
    fn run<Q: Write>(&self, statement: &Q) -> rusqlite::Result<usize> {
        self.execute(Q::SQL, statement.params().as_slice())
    }

    fn returning<Q: WriteReturning>(&self, statement: &Q) -> rusqlite::Result<Vec<Q::Row>> {
        self.query(Q::SQL, statement.params().as_slice(), Q::read)
    }
}
