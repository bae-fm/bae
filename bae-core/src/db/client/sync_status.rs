//! The sync status a screen shows, read in one go: coven's status
//! (`coven_sync_status`), what a retry would do (`coven_retry_effect`), its
//! pending records (`coven_pending`), and why this device's last attempt to
//! start sync failed (`sync_start_failure`).

use super::pending::pending_rows_on;
use super::sync_start_failure::start_failure_on;
use super::*;
use crate::library::SyncStatusSnapshot;

/// The sync status as one read of its tables sees it.
fn sync_status_on(sql: SqlReadContext<'_>) -> coven::CovenResult<SyncStatusSnapshot> {
    let status = coven_status_on(&sql)?;
    let pending = pending_rows_on(&sql)?;
    let start_failure = start_failure_on(&sql)?;
    let retry_effect = coven::RetryEffect::read(sql)?;
    Ok(SyncStatusSnapshot::read(
        status,
        retry_effect,
        pending,
        start_failure,
    ))
}

/// coven's sync status, the one row of `coven_sync_status`, which exists
/// from the store's creation.
fn coven_status_on(sql: &SqlReadContext<'_>) -> Result<coven::SyncStatus, DbError> {
    Ok(sql.query_row("SELECT status FROM coven_sync_status", [], |row| row.get(0))?)
}

/// Whether storage is set up on this device, as coven's sync status says:
/// what a release's storage choices and markers are read against.
pub(super) fn cloud_home_on(sql: &SqlReadContext<'_>) -> Result<bool, DbError> {
    Ok(!matches!(
        coven_status_on(sql)?,
        coven::SyncStatus::Disconnected
    ))
}

impl Database {
    /// Follow the sync status, each value read from its tables in one go.
    pub(crate) fn subscribe_sync_status(&self) -> coven::LiveQuery<SyncStatusSnapshot> {
        self.inner.handle.subscribe(sync_status_on)
    }

    /// The sync status now, as [`Self::subscribe_sync_status`] reads it.
    pub(crate) async fn sync_status(&self) -> Result<SyncStatusSnapshot, DbError> {
        Ok(self.inner.handle.read(sync_status_on).await?)
    }

    /// coven's own sync status now.
    pub(crate) async fn coven_sync_status(&self) -> Result<coven::SyncStatus, DbError> {
        self.read(|sql| coven_status_on(&sql)).await
    }
}
