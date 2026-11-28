//! What this device's upload queue holds of the library's rows, read from
//! coven's `coven_uploads` beside them.

use super::*;
use crate::UploadProgress;

// How many of the release `:release`'s files wait in this device's upload
// queue.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer ReleaseQueuedFileCount(release: String) { queued: i64 },
    r#"
    SELECT COUNT(*) AS queued FROM release_files AS f
    JOIN coven_uploads AS upload ON upload.file = f.file
    WHERE f.release_id = :release
"#
);

// Whether any of the release `:release`'s files waits in this device's upload
// queue.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer ReleaseHasQueuedFile(release: String) { queued: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM release_files AS f
                   JOIN coven_uploads AS upload ON upload.file = f.file
                   WHERE f.release_id = :release) AS queued
"#
);

// Whether the release file `:file` waits in this device's upload queue.
#[cfg(any(test, feature = "test-utils"))]
coven::read!(
    writer ReleaseFileQueued(file: String) { queued: bool },
    r#"
    SELECT EXISTS (SELECT 1 FROM release_files AS f
                   JOIN coven_uploads AS upload ON upload.file = f.file
                   WHERE f.id = :file) AS queued
"#
);

// Each release with files this process is sending now, and how many. One row
// per upload in flight is read, each release file found by its file.
coven::fragment!(
    ReleaseSending { files: i64 },
    r#"
    SELECT f.release_id, COUNT(*) AS files
    FROM coven_sending AS sending
    JOIN release_files AS f ON f.file = sending.file
    GROUP BY f.release_id
"#
);

// What holds the upload queue back, as one row: `upload_wait` names the wait
// ([`UploadQueueHeld::wait`] gives what it names) and `upload_wait_at` is the earliest
// time coven gives a file's wait to end. Each file's wait is coven's, as
// `coven_upload_phases` gives it, read once. The first that holds names the
// wait: `paused` while the person paused uploads; `sync_stopped` while sync
// isn't running; nothing while a file is being sent; `offline` while coven
// says the device has no usable network; `metered` while a file waits for an
// unmetered network; `retrying` while a file waits for its retry or a
// provider's cooldown to end. NULL when nothing holds the queue.
coven::fragment!(
    UploadQueueHeld { upload_wait: Option<String>, upload_wait_at: Option<i64> },
    r#"
    SELECT CASE
               WHEN pause.paused THEN 'paused'
               WHEN phases.not_syncing THEN 'sync_stopped'
               WHEN phases.sending THEN NULL
               WHEN (SELECT usable FROM coven_network) IS 0 THEN 'offline'
               WHEN phases.metered THEN 'metered'
               WHEN phases.wait_at IS NOT NULL THEN 'retrying'
           END AS upload_wait, phases.wait_at AS upload_wait_at
    FROM coven_upload_pause AS pause,
         (SELECT MAX(wait = 'not_syncing') AS not_syncing,
                 MAX(phase = 'uploading') AS sending,
                 MAX(wait = 'metered') AS metered,
                 MIN(at) AS wait_at
          FROM coven_upload_phases) AS phases
"#
);

impl UploadQueueHeld {
    /// What holds the upload queue back; `None` when nothing does.
    pub fn wait(&self) -> Result<Option<UploadQueueWait>, DbError> {
        self.upload_wait
            .as_deref()
            .map(|name| {
                named_wait(name, self.upload_wait_at)
                    .ok_or_else(|| DbError::unreadable("the upload queue's wait", name))
            })
            .transpose()
    }
}

/// The wait [`UploadQueueHeld`] names `name`, whose retry is due at `at_ms`;
/// `None` when `name` names no wait, or a retry has no time.
fn named_wait(name: &str, at_ms: Option<i64>) -> Option<UploadQueueWait> {
    Some(match name {
        "paused" => UploadQueueWait::Paused { by: PausedBy::Person },
        "sync_stopped" => UploadQueueWait::Paused { by: PausedBy::SyncStopped },
        "offline" => UploadQueueWait::Offline,
        "metered" => UploadQueueWait::Metered,
        "retrying" => UploadQueueWait::Retrying { at_ms: at_ms? },
        _ => return None,
    })
}

/// The upload progress of a release with queued files, `waiting` of which
/// only wait and `sending` of which are being sent. A count below zero, or
/// past a `u32`, is refused.
pub fn upload_progress(waiting: i64, sending: i64) -> Result<UploadProgress, DbError> {
    let count = |value: i64| {
        u32::try_from(value)
            .map_err(|_error| DbError::unreadable("a release's upload count", value))
    };
    Ok(UploadProgress { queued: count(waiting)?, uploading: count(sending)? })
}

impl crate::release_storage::ReleaseStorage {
    /// What of the release waits in this device's upload queue; `None` when
    /// none of its files waits. A count below zero, or past a `u32`, is
    /// refused.
    pub fn upload(&self) -> Result<Option<DbReleaseUpload>, DbError> {
        if self.queued_count == 0 {
            return Ok(None);
        }
        let count = |value: i64| {
            u32::try_from(value)
                .map_err(|_error| DbError::unreadable("a release's upload count", value))
        };
        let (waiting, sending, stored) =
            (count(self.waiting_files)?, count(self.sending_files)?, count(self.stored_files)?);
        let missing = if sending > 0 { None } else { self.missing_original_file.clone() };
        Ok(Some(DbReleaseUpload { waiting, sending, stored, missing }))
    }
}

// What this process is sending now, as one row: each file in flight, a
// cover's or a picture's among them, with the release and size of a release's
// file; each release with files in flight, found by its files from them, with
// the bytes of its files stored of its whole size; and whether the person
// paused uploads. One row per upload in flight is read.
coven::read!(
    writer SendingUploads {
        files: json<Vec<crate::models::DbSendingFile>>,
        releases: json<Vec<crate::models::DbSendingRelease>>,
        paused: bool
    },
    r#"
    SELECT (SELECT json_group_array(json_object(
                       'file', sending.file, 'release_id', f.release_id,
                       'file_size', CASE WHEN f.id IS NOT NULL THEN stored.size END))
            FROM coven_sending AS sending
            LEFT JOIN release_files AS f ON f.file = sending.file
            LEFT JOIN coven_files AS stored ON stored.id = sending.file) AS files,
           (SELECT json_group_array(json_object(
                       'release_id', storage.release_id,
                       'bytes_stored', storage.bytes_stored,
                       'bytes_total', storage.total_size))
            FROM @crate::release_storage::ReleaseStorage AS storage
            WHERE storage.release_id IN (
                SELECT f.release_id
                FROM coven_sending AS sending
                JOIN release_files AS f ON f.file = sending.file)) AS releases,
           pause.paused AS paused
    FROM coven_upload_pause AS pause
"#
);

/// What this process is sending now, as [`SendingUploads`] reads it.
fn sending_uploads_of(row: SendingUploadsRow) -> Result<DbSendingUploads, DbError> {
    Ok(DbSendingUploads { files: row.files, releases: row.releases, paused: row.paused })
}

impl Records {
    /// How many of the release's files still wait to upload from this device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        let statement = ReleaseQueuedFileCountParams { release: release_id.to_string() };
        Ok(self.store.read_row(statement).await?.queued)
    }

    /// Whether any of the release's files still waits to upload from this
    /// device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        let statement = ReleaseHasQueuedFileParams { release: release_id.to_string() };
        Ok(self.store.read_row(statement).await?.queued)
    }

    /// [`Self::has_pending_uploads_for_release`], live: it reruns as the
    /// release's files enter and leave the upload queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_has_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<bool, DbError> {
        let statement = ReleaseHasQueuedFileParams { release: release_id.to_string() };
        self.store.subscribe_row(statement).process(|row| Ok(row.queued))
    }

    /// The files this process is sending now and the releases they belong
    /// to with the bytes of each stored, live: it reruns as files start and
    /// stop being sent or the person pauses uploads. How far each file has
    /// got is coven's upload watch.
    pub fn subscribe_sending_uploads(&self) -> coven::LiveQuery<DbSendingUploads, DbError> {
        self.store.subscribe_row(SendingUploadsParams {}).process(sending_uploads_of)
    }

    /// Whether a release file's coven file still waits to upload from this
    /// device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn has_pending_cloud_upload(&self, file_id: &str) -> Result<bool, DbError> {
        let statement = ReleaseFileQueuedParams { file: file_id.to_string() };
        Ok(self.store.read_row(statement).await?.queued)
    }
}

#[cfg(test)]
#[path = "queued_uploads_tests.rs"]
mod tests;
