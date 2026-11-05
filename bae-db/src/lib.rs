//! bae's people, membership and sync status over the store: the `Database`
//! and the row writers that keep them, the people, collections and history a
//! library holds, and the sync status as coven and this device's rows give
//! it. This crate's part of the schema is `001_initial.sql`.

pub mod admin_change;
pub mod client;
pub mod collections;
pub mod history;
pub mod membership;
mod pending_item;
pub mod people;
pub use pending_item::{PendingItem, PendingSubjectName, PendingWait};

/// bae-db's part of the schema: `001_initial.sql`, which holds every table
/// no other crate's part creates.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] =
    &[bae_store::SchemaPart::new(&[include_str!("../migrations/001_initial.sql")])
        .synced(synced::synced_tables, synced::namespaces)];

mod sync_status;
pub mod synced;
pub use sync_status::{
    pending_action, sync_status_rows_on, PendingAction, SyncIndicator, SyncRetryOutcome,
    SyncStatusRows, SyncStatusSnapshot,
};
#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;
pub use client::Database;
pub use client::FileRow;
pub use client::PendingRow;
