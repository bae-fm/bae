//! The sync status as one statement: coven's sync state, joined to what the
//! library holds beside it, in one row.
//!
//! The joined columns are coven's sync state and when sync last finished,
//! what a retry would do, the recorded location, why this device's last
//! start failed, where its storage setup stands, and the name of the file
//! the failure's pending record is about.

use super::cause_file::{cause_file_column, CAUSE_FILE_JOINS};
use super::start_failure::{StartFailureColumns, START_FAILURE_COLUMNS};
use super::storage_setup::{storage_setup_columns, StorageSetupColumns, UNDONE_SETUP_JOIN};
use super::*;

/// The subject of the pending record the state names as its cause, when the
/// state is a failure a pending record names.
const CAUSE_SUBJECT: &str =
    "CASE sync.state WHEN 'failed' THEN sync.detail->'$.Failed.Pending.record.subject' END";

/// The statement, with its columns in the order [`StatusColumns::read`] reads
/// them. `coven_sync_state` is a live query's own: coven runs a query that
/// reads it again as a file starts or ends a transfer.
pub(crate) fn status_query() -> String {
    let start_failure = START_FAILURE_COLUMNS;
    let setup = storage_setup_columns();
    let undone = UNDONE_SETUP_JOIN;
    let cause_file = cause_file_column();
    format!(
        "SELECT shown.detail, shown.last_synced, retry.effect, retry.cooling_until, \
             status.location, {start_failure}, {setup}, \
             {cause_file} AS cause_file \
         FROM (SELECT sync.*, {CAUSE_SUBJECT} AS cause_subject \
             FROM coven_sync_state AS sync) AS shown \
         JOIN coven_sync_status AS status \
         JOIN coven_retry_effect AS retry \
         LEFT JOIN sync_start_failure AS failure \
         {undone} \
         {CAUSE_FILE_JOINS}"
    )
}

/// The columns of the statement's one row.
struct StatusColumns {
    state: coven::SyncState,
    last_synced: Option<i64>,
    effect: String,
    cooling_until: Option<i64>,
    location: Option<coven::StorageConfig>,
    start_failure: StartFailureColumns,
    storage_setup: StorageSetupColumns,
    cause_file: Option<String>,
}

impl StatusColumns {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            state: row.get(0)?,
            last_synced: row.get(1)?,
            effect: row.get(2)?,
            cooling_until: row.get(3)?,
            location: row.get(4)?,
            start_failure: StartFailureColumns::read(row, 5)?,
            storage_setup: StorageSetupColumns::read(row, 11)?,
            cause_file: row.get("cause_file")?,
        })
    }
}

/// The sync status as one read of the library sees it.
pub fn sync_status_on(sql: &SqlReadContext<'_>) -> Result<SyncStatusSnapshot, DbError> {
    snapshot_of(sql.query_row(&status_query(), [], StatusColumns::read)?)
}

/// The sync status the statement's row gives.
fn snapshot_of(columns: StatusColumns) -> Result<SyncStatusSnapshot, DbError> {
    let StatusColumns {
        state,
        last_synced,
        effect,
        cooling_until,
        location,
        start_failure,
        storage_setup,
        cause_file,
    } = columns;
    Ok(SyncStatusSnapshot {
        coven: coven::SyncStatus {
            state,
            last_synced: last_synced.map(coven::stored_time).transpose()?,
        },
        error: start_failure.error()?,
        retry_effect: coven::RetryEffect::from_columns(&effect, cooling_until)?,
        storage_setup: storage_setup.state()?,
        location,
        cause_file,
    })
}

#[cfg(test)]
#[path = "status_query_tests.rs"]
mod tests;
