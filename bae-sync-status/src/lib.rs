//! The sync status as the status line and the activity list show it: coven's
//! sync state beside the library's read of its pending records and the action
//! each offers, what a retry would do and why this device's last start
//! failed, behind the `SyncStatus` owner.

mod pending_item;
pub use pending_item::{PendingItem, PendingSubjectName, PendingWait};
mod sync_status;
pub use sync_status::{
    pending_action, sync_status_rows_on, FileRow, PendingAction, PendingRow, SetupDismissError,
    StartFailureRecordError, StorageSetupState, SyncIndicator, SyncRetryOutcome, SyncStatus,
    SyncStatusRows, SyncStatusSnapshot,
};

#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;

/// The parts of the schema the sync status holds.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] = &[sync_status::SCHEMA];
