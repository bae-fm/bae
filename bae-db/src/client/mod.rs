use bae_store::DbError;
use bae_store::Store;
use coven::SqlReadContext;
use rusqlite::{params, OptionalExtension, Row};

mod file_row;
pub mod pending;
pub mod sync_start_failure;
pub use file_row::FileRow;
pub use pending::PendingRow;

/// The library's sync status rows over the store.
#[derive(Clone)]
pub struct Database {
    /// The store every read and write of the sync status runs through.
    store: Store,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    pub fn new(store: Store) -> Self {
        Database { store }
    }
}
