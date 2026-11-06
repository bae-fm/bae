//! bae's sync status over the store: the `Database` and the row that keeps
//! why a start failed, and the sync status as coven and this device's rows
//! give it. This crate's part of the schema is `001_initial.sql`.

pub mod client;
mod pending_item;
pub use pending_item::{PendingItem, PendingSubjectName, PendingWait};

/// bae-db's part of the schema: `001_initial.sql`, which holds the sync
/// status's table.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] =
    &[bae_store::SchemaPart::new(&[include_str!("../migrations/001_initial.sql")])];

mod sync_status;
pub use sync_status::{
    pending_action, sync_status_rows_on, PendingAction, SyncIndicator, SyncRetryOutcome,
    SyncStatusRows, SyncStatusSnapshot,
};
#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;
pub use client::Database;
pub use client::FileRow;
pub use client::PendingRow;
