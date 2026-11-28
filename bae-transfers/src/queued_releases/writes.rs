//! The statements that write the queues' tables, and which of them each queue
//! runs. A statement names its table, so a move that every queue makes is one
//! statement for each queue.

use crate::download_snapshot::DownloadRequest;
use crate::output_snapshot::OutputRequest;
#[cfg(desktop)]
use crate::{output_snapshot::OutputKind, release_queue::ReleaseQueueEntry};
#[cfg(desktop)]
use bae_store::DbError;
use bae_store::WriteStatements;
use coven::CovenWrite;

/// One queue's moves of its entries, each returning how many rows it changed.
/// Only this crate can name this trait, so no other crate runs a queue's
/// statements.
pub trait QueueWrites: Sized {
    /// Queue `entry`, passing over a release already in the queue or one the
    /// library doesn't hold. The statement copies the file count and total
    /// size from the release's storage row, so the entry keeps both after the
    /// library deletes the release.
    #[cfg(desktop)]
    fn insert_on(sql: &CovenWrite<'_>, entry: &ReleaseQueueEntry<Self>) -> Result<usize, DbError>;
    /// Start a queued or active entry, unless the queue is paused.
    fn activate_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize>;
    /// Take out the active entry.
    fn finish_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize>;
    /// Fail a queued or active entry with `error`.
    fn fail_on(sql: &CovenWrite<'_>, release_id: &str, error: &str) -> rusqlite::Result<usize>;
    /// Fail the active entry with `error`.
    fn fail_active_on(
        sql: &CovenWrite<'_>,
        release_id: &str,
        error: &str,
    ) -> rusqlite::Result<usize>;
    /// Queue every failed entry again.
    fn retry_all_on(sql: &CovenWrite<'_>) -> rusqlite::Result<usize>;
    /// Queue a failed entry again.
    fn retry_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize>;
    /// Put the active entry back to queued.
    fn requeue_active_on(sql: &CovenWrite<'_>) -> rusqlite::Result<usize>;
    /// Take out an entry in any phase.
    fn delete_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize>;
}

impl QueueWrites for DownloadRequest {
    #[cfg(desktop)]
    fn insert_on(sql: &CovenWrite<'_>, entry: &ReleaseQueueEntry<Self>) -> Result<usize, DbError> {
        let (kind, target_dir) = match &entry.payload {
            Self::Pin => ("pin", None),
            Self::MoveToThisDevice { target_dir } => {
                ("move_to_this_device", Some(coven::path_bytes(target_dir)))
            }
        };
        Ok(sql.run(&InsertDownloadParams {
            release: &entry.release_id,
            created_at: entry.created_at,
            kind,
            target_dir,
        })?)
    }

    fn activate_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&ActivateDownloadParams { release: release_id })
    }

    fn finish_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&FinishDownloadParams { release: release_id })
    }

    fn fail_on(sql: &CovenWrite<'_>, release_id: &str, error: &str) -> rusqlite::Result<usize> {
        sql.run(&FailDownloadParams { release: release_id, error })
    }

    fn fail_active_on(
        sql: &CovenWrite<'_>,
        release_id: &str,
        error: &str,
    ) -> rusqlite::Result<usize> {
        sql.run(&FailActiveDownloadParams { release: release_id, error })
    }

    fn retry_all_on(sql: &CovenWrite<'_>) -> rusqlite::Result<usize> {
        sql.run(&RetryFailedDownloadsParams {})
    }

    fn retry_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&RetryFailedDownloadParams { release: release_id })
    }

    fn requeue_active_on(sql: &CovenWrite<'_>) -> rusqlite::Result<usize> {
        sql.run(&RequeueActiveDownloadParams {})
    }

    fn delete_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&DeleteDownloadParams { release: release_id })
    }
}

impl QueueWrites for OutputRequest {
    #[cfg(desktop)]
    fn insert_on(sql: &CovenWrite<'_>, entry: &ReleaseQueueEntry<Self>) -> Result<usize, DbError> {
        let OutputRequest { target_dir, kind } = &entry.payload;
        let (kind, preset, filename_pattern, folder_pattern, extras) = match kind {
            OutputKind::Export => ("export", None, None, None, None),
            OutputKind::Save { preset, filename_pattern, folder_pattern, extras } => (
                "save",
                Some(serde_json::to_string(preset).map_err(|error| {
                    DbError::encode(format!("save preset {}", preset.id), error)
                })?),
                Some(serde_json::to_string(filename_pattern).map_err(|error| {
                    DbError::encode(format!("save filename pattern of {}", preset.id), error)
                })?),
                Some(serde_json::to_string(folder_pattern).map_err(|error| {
                    DbError::encode(format!("save folder pattern of {}", preset.id), error)
                })?),
                Some(serde_json::to_string(extras).map_err(|error| {
                    DbError::encode(format!("save extras of {}", preset.id), error)
                })?),
            ),
        };
        Ok(sql.run(&InsertOutputParams {
            release: &entry.release_id,
            created_at: entry.created_at,
            target_dir: coven::path_bytes(target_dir),
            kind,
            preset,
            filename_pattern,
            folder_pattern,
            extras,
        })?)
    }

    fn activate_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&ActivateOutputParams { release: release_id })
    }

    fn finish_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&FinishOutputParams { release: release_id })
    }

    fn fail_on(sql: &CovenWrite<'_>, release_id: &str, error: &str) -> rusqlite::Result<usize> {
        sql.run(&FailOutputParams { release: release_id, error })
    }

    fn fail_active_on(
        sql: &CovenWrite<'_>,
        release_id: &str,
        error: &str,
    ) -> rusqlite::Result<usize> {
        sql.run(&FailActiveOutputParams { release: release_id, error })
    }

    fn retry_all_on(sql: &CovenWrite<'_>) -> rusqlite::Result<usize> {
        sql.run(&RetryFailedOutputsParams {})
    }

    fn retry_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&RetryFailedOutputParams { release: release_id })
    }

    fn requeue_active_on(sql: &CovenWrite<'_>) -> rusqlite::Result<usize> {
        sql.run(&RequeueActiveOutputParams {})
    }

    fn delete_on(sql: &CovenWrite<'_>, release_id: &str) -> rusqlite::Result<usize> {
        sql.run(&DeleteOutputParams { release: release_id })
    }
}

#[cfg(desktop)]
coven::write!(
    InsertDownload(
        release: &'p str,
        created_at: i64,
        kind: &'static str,
        target_dir: Option<Vec<u8>>,
    ),
    r#"
    INSERT INTO download_queue
        (release_id, created_at, kind, target_dir, file_count, total_size, state)
    SELECT :release, :created_at, :kind, :target_dir,
           storage.file_count, storage.total_size, 'queued'
    FROM @bae_records::release_storage::ReleaseStorage AS storage
    WHERE storage.release_id = :release
    ON CONFLICT (release_id) DO NOTHING
"#
);

#[cfg(desktop)]
coven::write!(
    InsertOutput(
        release: &'p str,
        created_at: i64,
        target_dir: Vec<u8>,
        kind: &'static str,
        preset: Option<String>,
        filename_pattern: Option<String>,
        folder_pattern: Option<String>,
        extras: Option<String>,
    ),
    r#"
    INSERT INTO output_queue
        (release_id, created_at, target_dir, kind, preset, filename_pattern, folder_pattern,
         extras, file_count, total_size, state)
    SELECT :release, :created_at, :target_dir, :kind, :preset, :filename_pattern, :folder_pattern,
           :extras, storage.file_count, storage.total_size, 'queued'
    FROM @bae_records::release_storage::ReleaseStorage AS storage
    WHERE storage.release_id = :release
    ON CONFLICT (release_id) DO NOTHING
"#
);

// Queue to pin, in the order of the JSON array `releases`, each release not
// wholly pinned on this device, as its storage row says, turning a failed
// entry into a queued pin in its place.
coven::write!(
    QueuePins(created_at: i64, releases: &'p str),
    r#"
    INSERT INTO download_queue (release_id, created_at, file_count, total_size, state, kind)
    SELECT requested.value, :created_at, storage.file_count, storage.total_size, 'queued', 'pin'
    FROM json_each(:releases) AS requested
    JOIN @bae_records::release_storage::ReleaseStorage AS storage
      ON storage.release_id = requested.value
    WHERE NOT storage.pinned
    ORDER BY requested.key
    ON CONFLICT (release_id) DO UPDATE SET state = 'queued', error = NULL,
        file_count = excluded.file_count, total_size = excluded.total_size,
        kind = 'pin', target_dir = NULL
    WHERE download_queue.state = 'failed'
"#
);

// Queue to move to this device into `target_dir`, in the order of the JSON
// array `releases`, each release the library holds, turning a failed entry
// into this move in its place.
#[cfg(desktop)]
coven::write!(
    QueueMovesToThisDevice(created_at: i64, releases: &'p str, target_dir: Vec<u8>),
    r#"
    INSERT INTO download_queue
        (release_id, created_at, file_count, total_size, state, kind, target_dir)
    SELECT requested.value, :created_at, storage.file_count, storage.total_size,
           'queued', 'move_to_this_device', :target_dir
    FROM json_each(:releases) AS requested
    JOIN @bae_records::release_storage::ReleaseStorage AS storage
      ON storage.release_id = requested.value
    ORDER BY requested.key
    ON CONFLICT (release_id) DO UPDATE SET state = 'queued', error = NULL,
        file_count = excluded.file_count, total_size = excluded.total_size,
        kind = 'move_to_this_device', target_dir = excluded.target_dir
    WHERE download_queue.state = 'failed'
"#
);

coven::write!(
    ActivateDownload(release: &'p str),
    r#"
    UPDATE download_queue SET state = 'active'
    WHERE release_id = :release AND state IN ('queued', 'active')
      AND NOT (SELECT pause.paused FROM release_queues AS pause WHERE pause.queue = 'download')
"#
);

coven::write!(
    ActivateOutput(release: &'p str),
    r#"
    UPDATE output_queue SET state = 'active'
    WHERE release_id = :release AND state IN ('queued', 'active')
      AND NOT (SELECT pause.paused FROM release_queues AS pause WHERE pause.queue = 'output')
"#
);

coven::write!(
    FinishDownload(release: &'p str),
    r#"DELETE FROM download_queue WHERE release_id = :release AND state = 'active'"#
);

coven::write!(
    FinishOutput(release: &'p str),
    r#"DELETE FROM output_queue WHERE release_id = :release AND state = 'active'"#
);

coven::write!(
    FailDownload(release: &'p str, error: &'p str),
    r#"
    UPDATE download_queue SET state = 'failed', error = :error
    WHERE release_id = :release AND state IN ('queued', 'active')
"#
);

coven::write!(
    FailOutput(release: &'p str, error: &'p str),
    r#"
    UPDATE output_queue SET state = 'failed', error = :error
    WHERE release_id = :release AND state IN ('queued', 'active')
"#
);

coven::write!(
    FailActiveDownload(release: &'p str, error: &'p str),
    r#"
    UPDATE download_queue SET state = 'failed', error = :error
    WHERE release_id = :release AND state = 'active'
"#
);

coven::write!(
    FailActiveOutput(release: &'p str, error: &'p str),
    r#"
    UPDATE output_queue SET state = 'failed', error = :error
    WHERE release_id = :release AND state = 'active'
"#
);

coven::write!(
    RetryFailedDownloads,
    r#"UPDATE download_queue SET state = 'queued', error = NULL WHERE state = 'failed'"#
);

coven::write!(
    RetryFailedOutputs,
    r#"UPDATE output_queue SET state = 'queued', error = NULL WHERE state = 'failed'"#
);

coven::write!(
    RetryFailedDownload(release: &'p str),
    r#"
    UPDATE download_queue SET state = 'queued', error = NULL
    WHERE release_id = :release AND state = 'failed'
"#
);

coven::write!(
    RetryFailedOutput(release: &'p str),
    r#"
    UPDATE output_queue SET state = 'queued', error = NULL
    WHERE release_id = :release AND state = 'failed'
"#
);

coven::write!(
    RequeueActiveDownload,
    r#"UPDATE download_queue SET state = 'queued' WHERE state = 'active'"#
);

coven::write!(
    RequeueActiveOutput,
    r#"UPDATE output_queue SET state = 'queued' WHERE state = 'active'"#
);

coven::write!(
    DeleteDownload(release: &'p str),
    r#"DELETE FROM download_queue WHERE release_id = :release"#
);

coven::write!(
    DeleteOutput(release: &'p str),
    r#"DELETE FROM output_queue WHERE release_id = :release"#
);

coven::write!(
    SetQueuePaused(queue: &'static str, paused: bool),
    r#"UPDATE release_queues SET paused = :paused WHERE queue = :queue"#
);

coven::write!(ClearOutputQueue, r#"DELETE FROM output_queue"#);

// Queue the releases of the JSON array `releases` to pin, with no files, the
// first at place `start` and each next one a place later.
#[cfg(any(test, feature = "test-utils"))]
coven::write!(
    QueueDownloadsAtPlaces(start: i64, releases: &'p str),
    r#"
    INSERT INTO download_queue
        (position, release_id, created_at, file_count, total_size, state, kind)
    SELECT :start + requested.key, requested.value, :start + requested.key, 0, 0,
           'queued', 'pin'
    FROM json_each(:releases) AS requested
"#
);
