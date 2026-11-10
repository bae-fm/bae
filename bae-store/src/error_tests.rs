use super::*;
use bae_util::database_failure::{DatabaseFailure, FailedDatabaseCall};

/// A write while another connection holds the database's write lock, with
/// no wait: SQLite's own busy refusal.
fn busy_write() -> rusqlite::Error {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("library.db");
    let holder = rusqlite::Connection::open(&path).expect("open the holder");
    holder.execute_batch("CREATE TABLE t (x); BEGIN EXCLUSIVE;").expect("hold the write lock");
    let writer = rusqlite::Connection::open(&path).expect("open the writer");
    writer.busy_timeout(std::time::Duration::ZERO).expect("no wait");
    writer.execute("INSERT INTO t VALUES (1)", []).expect_err("the holder keeps the lock")
}

/// A read of a file that isn't a database: SQLite's own damage refusal.
fn damaged_read() -> rusqlite::Error {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("library.db");
    std::fs::write(&path, vec![0x5a; 8192]).expect("write a file that isn't a database");
    let connection = rusqlite::Connection::open(&path).expect("open");
    connection
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))
        .expect_err("the file isn't a database")
}

/// A write that breaks the schema's rule.
fn refused_write() -> rusqlite::Error {
    let connection = rusqlite::Connection::open_in_memory().expect("open");
    connection.execute_batch("CREATE TABLE t (x NOT NULL)").expect("create");
    connection.execute("INSERT INTO t VALUES (NULL)", []).expect_err("NOT NULL refuses it")
}

/// Busy passes when made again; damage and a broken rule never do, whether
/// SQLite answered bae's call or coven's under it, and under a rollback that
/// failed too.
#[test]
fn sqlite_tells_busy_from_damaged_from_refused() {
    assert_eq!(DbError::from(busy_write()).database_failure(), DatabaseFailure::Busy);
    assert_eq!(DbError::from(damaged_read()).database_failure(), DatabaseFailure::Damaged);
    assert_eq!(DbError::from(refused_write()).database_failure(), DatabaseFailure::Refused);
    let busy_in_coven = coven::CovenError::Database(coven::DbError::from(busy_write()));
    assert_eq!(DbError::from(busy_in_coven).database_failure(), DatabaseFailure::Busy);
    let rolled = DbError::Rollback {
        operation: Box::new(DbError::from(busy_write())),
        rollback: refused_write(),
    };
    assert_eq!(rolled.database_failure(), DatabaseFailure::Busy);
}

/// A stored value nothing here writes is damage, which no retry reads past.
#[test]
fn a_value_nothing_writes_is_damage() {
    let unreadable = DbError::unreadable("a column", "a value");
    assert_eq!(unreadable.database_failure(), DatabaseFailure::Damaged);
}
