//! What the sync status offers to reads: coven's sync state with what the
//! library holds beside it, and its two parts, why this device's last start
//! failed and where its storage setup stands. A read composes them as
//! fragments, so no other domain reads the sync status's table.

sqlive_compose::fragment!(
    StartFailure,
    r#"
    SELECT failure.category, failure.storage_failure, failure.required_access,
           failure.size_bytes, failure.limit_bytes, failure.detail
    FROM sync_start_failure AS failure
"#
);

// One row always: whether a storage setup runs, which is one in
// `coven_operations` with no outcome yet, beside the newest one that ended
// undone, if any. coven names how an operation ended in `outcome_kind`.
sqlive_compose::fragment!(
    StorageSetup {
        setup_running: bool,
        undone_operation: Option<i64>,
        undone_outcome: Option<coven::StorageSetupOutcome>,
    },
    r#"
    SELECT EXISTS (SELECT 1 FROM coven_operations AS setup
                   WHERE setup.kind = 'StorageSetup' AND setup.outcome IS NULL)
               AS setup_running,
           undone.operation AS undone_operation, undone.outcome AS undone_outcome
    FROM (SELECT 1) AS one
    LEFT JOIN (SELECT operation, outcome
               FROM coven_operations
               WHERE kind = 'StorageSetup' AND outcome_kind IN ('undone', 'rejected')
               ORDER BY operation DESC LIMIT 1) AS undone ON 1
"#
);

// One row: coven's sync state and when sync last finished, what a retry would
// do, the recorded location, the failed start while one stands, and the
// storage setup. `coven_sync_state` is a live query's own: coven runs a query
// that reads it again as a file starts or ends a transfer.
sqlive_compose::fragment!(
    CurrentSyncStatus {
        state: coven::SyncState,
        last_synced: Option<i64>,
        effect: String,
        cooling_until: Option<i64>,
        location: Option<coven::StorageConfig>,
    },
    r#"
    SELECT shown.detail AS state, shown.last_synced, retry.effect, retry.cooling_until,
           status.location, failure.*, setup.*
    FROM coven_sync_state AS shown
    JOIN coven_sync_status AS status
    JOIN coven_retry_effect AS retry
    LEFT JOIN @crate::sync_status::fragments::StartFailure AS failure ON 1
    JOIN @crate::sync_status::fragments::StorageSetup AS setup ON 1
"#
);
