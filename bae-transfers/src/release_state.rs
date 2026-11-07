//! What the transfer queues and the storage transitions say of one release,
//! as its detail shows it beside the library's rows.

use crate::download_entry::{download_entry_column, download_entry_sql};
use crate::release_storage_action::release_transfer_action_on;
use crate::DownloadEntry;
use bae_store::DbError;
use bae_types::release_storage::ReleaseStorageAction;
use coven::SqlReadContext;

/// One release's entry in this device's download queue and the storage
/// transition it is in.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseTransferState {
    /// Its entry in the download queue, and what holds it back; `None` when
    /// it has none.
    pub download: Option<DownloadEntry>,
    /// The storage transition this process runs for it; `None` when it runs
    /// none.
    pub action: Option<ReleaseStorageAction>,
}

/// What the queues and the transitions say of `release_id` now.
pub fn release_transfer_state_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<ReleaseTransferState, DbError> {
    Ok(ReleaseTransferState {
        download: sql.query_row(
            &format!("SELECT {}", download_entry_sql("?1")),
            [release_id],
            download_entry_column,
        )?,
        action: release_transfer_action_on(sql, release_id)?,
    })
}
