//! What the transfer queues and the storage transitions say of one release,
//! as its detail shows it beside the library's rows.

use crate::queued_releases::tables::download_phase_on;
use crate::release_storage_action::{transfer_action_column, transfer_action_sql};
use crate::DownloadPhase;
use bae_db::DbError;
use bae_types::release_storage::ReleaseStorageAction;
use coven::SqlReadContext;

/// One release's entry in this device's download queue and the storage
/// transition it is in.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseTransferState {
    /// Its entry in the download queue; `None` when it has none.
    pub download: Option<DownloadPhase>,
    /// The storage transition this process runs for it; `None` when it runs
    /// none.
    pub action: Option<ReleaseStorageAction>,
}

/// What the queues and the transitions say of `release_id` now.
pub fn release_transfer_state_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<ReleaseTransferState, DbError> {
    let action = sql.query_row(
        &format!("SELECT {} AS transfer_action", transfer_action_sql("?1")),
        [release_id],
        |row| transfer_action_column(row, "transfer_action"),
    )?;
    Ok(ReleaseTransferState { download: download_phase_on(sql, release_id)?, action })
}
