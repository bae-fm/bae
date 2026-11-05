use bae_store::read::*;
use bae_store::DbError;
use bae_store::Store;
use chrono::{DateTime, Utc};
use coven::{CovenWrite, SqlReadContext};
use rusqlite::{params, OptionalExtension, Params, Row};
use std::collections::HashSet;
use std::sync::Arc;

mod admin_operations;
mod collections;
mod file_row;
mod history;
mod membership;
pub mod pending;
mod people;
mod profile_request;
pub mod sync_start_failure;
pub use file_row::FileRow;
pub use pending::PendingRow;

struct DatabaseInner {
    /// The store every read and write of the database runs through.
    store: Store,
    profile_creation: tokio::sync::Mutex<()>,
    history_registration: tokio::sync::Mutex<()>,
}

/// The library's people, membership and sync status over the store.
#[derive(Clone)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    pub fn new(store: Store) -> Self {
        Database {
            inner: Arc::new(DatabaseInner {
                store,
                profile_creation: tokio::sync::Mutex::new(()),
                history_registration: tokio::sync::Mutex::new(()),
            }),
        }
    }
}

fn uuid_column(row: &Row<'_>, index: usize) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(index)?;
    uuid::Uuid::parse_str(&value)
        .map_err(|error| bae_store::read::column_conversion_error(row, index, error))
}
