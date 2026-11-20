//! The sync status as the status line and the activity list show it: coven's
//! sync state joined, in one statement, to the file its failure is about,
//! what a retry would do and why this device's last start failed, behind the
//! `SyncStatus` owner.

mod pending_item;
pub use pending_item::{FileProblem, PendingItem, PendingSubjectName, PendingWait};
mod sync_status;
pub use sync_status::{
    copy_behind, sync_status_on, CopyResetNotice, SetupDismissError, StartFailureRecordError,
    StorageSetupState, SyncRetryOutcome, SyncStatus, SyncStatusSnapshot,
};
mod sync_shown;
pub use sync_shown::{SyncCauseLine, SyncFix, SyncShown, SyncStatusWord, SyncTone};

#[cfg(any(test, feature = "test-utils"))]
pub mod test_fixtures;

/// The parts of the schema the sync status holds.
pub const SCHEMA_PARTS: &[bae_store::SchemaPart] = &[sync_status::SCHEMA];
