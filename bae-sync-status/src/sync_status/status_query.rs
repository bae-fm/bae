//! The sync status as one statement: coven's sync state, joined to what the
//! library holds beside it, in one row.

use super::fragments::{CurrentSyncStatus, StartFailure};
use super::*;

coven::read!(
    writer LibrarySyncStatus,
    r#"
    SELECT current.*
    FROM @crate::sync_status::fragments::CurrentSyncStatus AS current
"#
);

/// The sync status as one read of the library sees it.
pub fn sync_status_on(sql: &ReadContext<'_>) -> Result<SyncStatusSnapshot, DbError> {
    bae_store::statement_row(sql, &LibrarySyncStatusParams {})?.current.into_snapshot()
}

/// Follow the sync status: one live query that joins coven's sync state to
/// the library's rows.
pub fn subscribe_sync_status(
    store: &bae_store::Store,
) -> coven::LiveQuery<SyncStatusSnapshot, DbError> {
    store.subscribe_row(LibrarySyncStatusParams {}).process(|row| row.current.into_snapshot())
}

impl CurrentSyncStatus {
    /// The sync status the fragment's row gives.
    pub fn into_snapshot(self) -> Result<SyncStatusSnapshot, DbError> {
        let Self { state, last_synced, effect, cooling_until, location, failure, setup } = self;
        Ok(SyncStatusSnapshot {
            coven: coven::SyncStatus {
                state,
                last_synced: last_synced.map(coven::stored_time).transpose()?,
            },
            error: failure.map(StartFailure::into_error).transpose()?,
            retry_effect: coven::RetryEffect::from_columns(&effect, cooling_until)?,
            storage_setup: setup.state()?,
            location,
        })
    }
}

#[cfg(test)]
#[path = "status_query_tests.rs"]
mod tests;
