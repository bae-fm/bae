//! The sync status as one statement: coven's sync state, joined to what the
//! library holds beside it. The statement returns a row for each pending
//! record, or one row when none is pending, and every row repeats the
//! columns that describe the status as a whole.
//!
//! The joined columns are coven's sync state and when sync last finished,
//! what a retry would do, the recorded location, why this
//! device's last start failed, where its storage setup stands, how many
//! records are pending, and the name of the file the failure's pending
//! record is about.

use super::pending::{cause_file_column, pending_columns, pending_joins, PendingColumns};
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
    let pending = pending_columns();
    let joins = pending_joins();
    let cause_file = cause_file_column();
    format!(
        "SELECT shown.detail, shown.last_synced, retry.effect, retry.cooling_until, \
             status.location, {start_failure}, {setup}, \
             COUNT(file.position) OVER () AS attention_count, \
             {cause_file} AS cause_file, \
             {pending} \
         FROM (SELECT sync.*, {CAUSE_SUBJECT} AS cause_subject \
             FROM coven_sync_state AS sync) AS shown \
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
    attention_count: u32,
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
            attention_count: bae_store::read::count_column(row, "attention_count")?,
            cause_file: row.get("cause_file")?,
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
        attention_count,
        cause_file,
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
        attention_count,
        cause_file,
    })
}

#[cfg(test)]
#[path = "status_query_tests.rs"]
mod tests;
