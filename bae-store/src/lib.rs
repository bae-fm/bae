//! bae's library store, with no table of its own: the `Store` over coven's
//! handle and the entry points every read, write and live query runs
//! through, how a generated read or write runs, the coven calls bae makes on a library, the schema's shape (each
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
mod file_budget;
pub use file_budget::lease_one;
mod file_stream;
pub use file_stream::{BlockingFileReader, FileStream};
pub mod file_version;
pub use file_version::file_version_sql;
pub mod list_windows;
pub use list_windows::{ListPage, ListWindow, SortDirection, WindowRead};
pub mod live_query;
pub mod live_results;
pub mod path_column;
pub mod query;
pub mod read;
mod read_statement;
pub use read_statement::{
    statement_optional, statement_row, statement_rows, RowsOfBoth, WriterRead,
};
pub mod schema;
pub use schema::{
    configure_store, Schema, SchemaCollation, SchemaFunction, SchemaPart, SqlArguments,
};
include!(concat!(env!("OUT_DIR"), "/schema.rs"));
mod store;
pub use store::{unsent_to_storage, utc_now, StorageSetupSteps, Store};
mod store_read;
pub mod upload_flags;
pub use store_read::StoreRead;
mod write_statement;
pub use write_statement::WriteStatements;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_devices;
#[cfg(any(test, feature = "test-utils"))]
mod test_sources;
#[cfg(any(test, feature = "test-utils"))]
pub use test_sources::TestSources;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
