//! Closing a store deterministically and copying its directory, for tests
//! that read a store's files directly.

use crate::db::Database;
use std::sync::Arc;

/// Close `db` and wait for the store behind it to finish closing.
///
/// Dropping the last handle from inside the runtime detaches coven's connection
/// thread: it drains its queue and closes SQLite whenever it reaches the stop,
/// which is not before the test moves on — the `-wal` sidecar is still on disk
/// seconds later. Dropping it off the runtime takes coven's other path, which
/// joins that thread instead, so by the time this returns the store is
/// checkpointed, closed, and safe to copy.
pub(super) fn close_store(db: Database) {
    assert_eq!(
        Arc::strong_count(&db.inner),
        1,
        "closing a store takes the last handle: another clone holds the \
         connection open, and nothing below would wait for it"
    );
    std::thread::spawn(move || drop(db))
        .join()
        .expect("coven's connection thread closes without panicking");
}

/// Copy a closed coven store directory.
///
/// Closed is the precondition, not a preference. coven keeps the store in WAL,
/// so a live one holds committed pages in its `-wal` sidecar — a copy of the
/// `.db` alone is missing them — and the sidecars delete themselves the instant
/// the connection does finally close, which is a file vanishing between this
/// `read_dir` and the copy that follows it.
pub(super) fn copy_store(source: &std::path::Path, destination: &std::path::Path) {
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.ends_with("-wal") && !name.ends_with("-shm"),
            "{name} says this store is still open; close it before copying it"
        );
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_store(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
