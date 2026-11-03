//! coven's sync status row (`coven_sync_status`): whether storage is set up
//! on this device, whether the running pass has moved a slot, and how the
//! last pass ended, which the library's sync status and the views read
//! beside their own rows.

use super::*;

/// What coven's sync status row says, as bae reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct CovenSyncRow {
    /// A storage location is recorded and this device holds its
    /// credentials (`storage` is `connected`).
    pub connected: bool,
    /// The running pass has read a slot it hadn't, or claimed one.
    pub moving: bool,
    /// How the last pass ended; `None` until a pass ends on this device,
    /// which counts as synced, since setup, joining and restoring read the
    /// store to its end.
    pub last_pass: Option<coven::LastPass>,
}

/// coven's sync status row, which exists from the store's creation.
pub fn coven_row_on(sql: &SqlReadContext<'_>) -> Result<CovenSyncRow, DbError> {
    Ok(sql.query_row(
        "SELECT storage = 'connected', moving, last_pass FROM coven_sync_status",
        [],
        |row| {
            Ok(CovenSyncRow { connected: row.get(0)?, moving: row.get(1)?, last_pass: row.get(2)? })
        },
    )?)
}

/// Whether storage is set up on this device, as coven's sync status says:
/// what a release's storage choices and markers are read against.
pub fn cloud_home_on(sql: &SqlReadContext<'_>) -> Result<bool, DbError> {
    Ok(sql.query_row("SELECT storage = 'connected' FROM coven_sync_status", [], |row| row.get(0))?)
}

impl Database {
    /// Whether storage is set up on this device now.
    pub async fn cloud_home(&self) -> Result<bool, DbError> {
        self.read(|sql| cloud_home_on(&sql)).await
    }
}
