//! The sync status as one statement: coven's sync state, joined to what the
//! library holds beside it. The statement returns a row for each pending
//! record, or one row when none is pending, and every row repeats the
//! columns that describe the status as a whole.
//!
//! The joined columns are coven's sync state and when sync last finished,
//! what a retry would do, the recorded location, why this
//! device's last start failed, where its storage setup stands, whether a
//! pending record already shows the failure the state names, and how many
//! records are pending.

use super::pending::{pending_columns, pending_joins, PendingColumns};
use super::start_failure::{StartFailureColumns, START_FAILURE_COLUMNS};
use super::storage_setup::{storage_setup_columns, StorageSetupColumns, UNDONE_SETUP_JOIN};
use super::*;

/// The sync state's pass, when the state is named by how the last pass ended:
/// offline, or a failure the pass met. A pending record names every other
/// state, or none does.
const SHOWN_PASS: &str = "CASE sync.state \
     WHEN 'offline' THEN sync.detail->'$.Offline.Pass' \
     WHEN 'offline_with_unsent_changes' THEN sync.detail->'$.OfflineWithUnsentChanges.Pass' \
     WHEN 'signed_out' THEN sync.detail->'$.SignedOut.cause.Pass' \
     WHEN 'no_access' THEN sync.detail->'$.NoAccess.Pass' \
     WHEN 'storage_full' THEN sync.detail->'$.StorageFull.Pass' \
     WHEN 'folder_missing' THEN sync.detail->'$.FolderMissing.Pass' \
     WHEN 'clock_wrong' THEN sync.detail->'$.ClockWrong.Pass' \
     WHEN 'update_needed' THEN sync.detail->'$.UpdateNeeded.Pass' \
     WHEN 'failed' THEN sync.detail->'$.Failed.Pass' \
     END";

/// Whether a pending record shows the failure `state` shows: a removed
/// device or one whose keys another device replaced shows its failure from
/// the record of that reason. Any other failure is the last pass's, which
/// coven records beside it: the connection's condition when storage refused
/// the pass or couldn't be reached, a blocked slot's, and that of the
/// operation whose last attempt failed.
const FAILURE_RECORDED: &str = "CASE \
     WHEN state.state = 'removed' THEN EXISTS ( \
         SELECT 1 FROM coven_pending AS held WHERE held.reason_kind = 'removed') \
     WHEN json_type(state.detail, '$.NeedsReset.reason.MemberKeysReplaced') IS NOT NULL THEN EXISTS ( \
         SELECT 1 FROM coven_pending AS held WHERE held.reason_kind = 'member_keys_replaced') \
     WHEN state.pass IS NULL THEN 0 \
     ELSE EXISTS ( \
         SELECT 1 FROM coven_pending AS held \
         WHERE (held.subject_kind = 'connection' AND held.reason_kind = 'storage' \
                 AND CAST(held.record AS TEXT)->'$.reason.Storage.failure' \
                     = coalesce(state.pass->'$.Offline.failure', \
                         state.pass->'$.Failed.failure.Storage')) \
             OR (held.subject_kind = 'slot' \
                 AND CAST(held.record AS TEXT)->'$.reason' = state.pass->'$.Blocked.reason') \
             OR (held.subject_kind = 'operation' \
                 AND CAST(held.record AS TEXT)->'$.subject.Operation.kind' \
                     = state.pass->'$.Failed.failure.Operation.subject.Operation.kind' \
                 AND CAST(held.record AS TEXT)->'$.reason' \
                     = state.pass->'$.Failed.failure.Operation.reason')) \
     END";

/// The statement, with its columns in the order [`StatusColumns::read`] reads
/// them. `coven_sync_state` is a live query's own: coven runs a query that
/// reads it again as a file starts or ends a transfer.
pub(crate) fn status_query() -> String {
    let start_failure = START_FAILURE_COLUMNS;
    let setup = storage_setup_columns();
    let undone = UNDONE_SETUP_JOIN;
    let pending = pending_columns();
    let joins = pending_joins();
    format!(
        "SELECT shown.detail, shown.last_synced, retry.effect, retry.cooling_until, \
             status.location, {start_failure}, {setup}, \
             shown.failure_recorded, \
             COUNT(file.position) OVER () AS attention_count, \
             {pending} \
         FROM (SELECT state.*, {FAILURE_RECORDED} AS failure_recorded \
             FROM (SELECT sync.*, {SHOWN_PASS} AS pass FROM coven_sync_state AS sync) AS state \
             ) AS shown \
         JOIN coven_sync_status AS status \
         JOIN coven_retry_effect AS retry \
         LEFT JOIN sync_start_failure AS failure \
         {undone} \
         LEFT JOIN coven_pending AS file \
         {joins} \
         ORDER BY file.position"
    )
}

/// The columns that describe the status as a whole, as one row reads them.
struct StatusColumns {
    state: coven::SyncState,
    last_synced: Option<i64>,
    effect: String,
    cooling_until: Option<i64>,
    location: Option<coven::StorageConfig>,
    start_failure: StartFailureColumns,
    storage_setup: StorageSetupColumns,
    failure_recorded: bool,
    attention_count: u32,
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
            failure_recorded: row.get("failure_recorded")?,
            attention_count: bae_store::read::count_column(row, "attention_count")?,
        })
    }
}

/// One row of the statement: the columns of the whole status and those of a
/// pending record.
type StatusRow = (StatusColumns, PendingColumns);

fn read_row(row: &Row<'_>) -> rusqlite::Result<StatusRow> {
    Ok((StatusColumns::read(row)?, PendingColumns::read(row, 16)?))
}

/// The sync status as one read of the library sees it.
pub fn sync_status_on(sql: &SqlReadContext<'_>) -> Result<SyncStatusSnapshot, DbError> {
    snapshot_of(sql.query(&status_query(), [], read_row)?)
}

/// The sync status the statement's rows give.
fn snapshot_of(rows: Vec<StatusRow>) -> Result<SyncStatusSnapshot, DbError> {
    let mut status = None;
    let mut pending = Vec::new();
    for (columns, record) in rows {
        if let Some(row) = record.row()? {
            pending.push(PendingItem::listed(row));
        }
        status.get_or_insert(columns);
    }
    let StatusColumns {
        state,
        last_synced,
        effect,
        cooling_until,
        location,
        start_failure,
        storage_setup,
        failure_recorded,
        attention_count,
    } = status.ok_or_else(|| DbError::unreadable("coven_sync_state", "no row"))?;
    Ok(SyncStatusSnapshot {
        coven: coven::SyncStatus {
            state,
            last_synced: last_synced.map(coven::stored_time).transpose()?,
        },
        pending,
        error: start_failure.error()?,
        retry_effect: coven::RetryEffect::from_columns(&effect, cooling_until)?,
        storage_setup: storage_setup.state()?,
        location,
        failure_recorded,
        attention_count,
    })
}

#[cfg(test)]
#[path = "status_query_tests.rs"]
mod tests;
