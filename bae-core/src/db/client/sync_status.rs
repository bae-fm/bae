//! coven's sync status (`coven_sync_status`): whether storage is set up and
//! sync runs, which the library's sync status and the views read beside
//! their own rows.

use super::*;

/// coven's sync status, the one row of `coven_sync_status`, which exists
/// from the store's creation.
pub(crate) fn coven_status_on(sql: &SqlReadContext<'_>) -> Result<coven::SyncStatus, DbError> {
    Ok(sql.query_row("SELECT status FROM coven_sync_status", [], |row| row.get(0))?)
}

/// Whether storage is set up on this device, as coven's sync status says:
/// what a release's storage choices and markers are read against.
pub(crate) fn cloud_home_on(sql: &SqlReadContext<'_>) -> Result<bool, DbError> {
    Ok(!matches!(coven_status_on(sql)?, coven::SyncStatus::Disconnected))
}

impl Database {
    /// coven's own sync status now.
    pub(crate) async fn coven_sync_status(&self) -> Result<coven::SyncStatus, DbError> {
        self.read(|sql| coven_status_on(&sql)).await
    }
}
