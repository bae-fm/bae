//! bae's library store, with no table of its own: the `Store` over coven's
//! handle and the entry points every read, write and live query runs
//! through, the coven calls bae makes on a library, the schema's shape (each
//! crate's parts of it, run as one migration) and the SQL functions and
//! collation the parts declare, list windows, the cancellable live queries a
//! surface pulls values from, and the names a person enters.

mod builder;
pub use builder::store_builder;
mod device_state;
pub use device_state::FormerDeviceState;
mod entered_name;
pub use entered_name::{BlankName, EnteredName};
mod error;
pub use error::{DbError, WriteError};
mod file_stream;
pub use file_stream::FileStream;
mod file_version;
pub use file_version::file_version_sql;
pub mod list_windows;
pub use list_windows::{
    LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows, ListWindow, SortDirection,
    WindowRead,
};
pub mod live_query;
pub mod path_column;
pub mod query;
pub mod read;
pub mod schema;
pub use schema::{
    configure_store, Schema, SchemaCollation, SchemaFunction, SchemaPart, SqlArguments, FOLD_CASE,
    NAME_KEY, NAME_LIST_KEY, NATURAL_ORDER, SEARCH_PATTERN,
};
mod store;
pub use store::{syncs_on, utc_now, StorageSetupSteps, Store, UNMETERED_ONLY, UPLOADS_PAUSED};
mod store_read;
pub use store_read::StoreRead;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_devices;
#[cfg(any(test, feature = "test-utils"))]
mod test_sources;
#[cfg(any(test, feature = "test-utils"))]
pub use test_sources::TestSources;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;
mod with_sync_state;
pub use with_sync_state::WithSyncState;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
