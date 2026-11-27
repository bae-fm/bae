//! Running a generated `write!` statement in a write's transaction.

use coven::CovenWrite;
use sqlive_compose::Write;

/// A write's context, which runs generated writes.
pub trait WriteStatements {
    /// Run `statement`, and return how many rows it changed.
    fn run<Q: Write>(&self, statement: &Q) -> rusqlite::Result<usize>;
}

impl WriteStatements for CovenWrite<'_> {
    fn run<Q: Write>(&self, statement: &Q) -> rusqlite::Result<usize> {
        self.execute(Q::SQL, statement.params().as_slice())
    }
}
