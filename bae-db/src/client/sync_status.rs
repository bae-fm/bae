//! Whether storage is set up on this device, as coven's sync status row
//! (`coven_sync_status`) says: what a release's storage choices and markers
//! are read against, beside the library's own rows.

use super::*;

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
