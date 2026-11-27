//! What this device's upload queue holds of the library's rows, read from
//! coven's `coven_uploads` beside them.

use super::*;
use crate::UploadProgress;
use bae_store::statement_row;

/// How many of the release `?1`'s files wait in this device's upload queue.
#[cfg(any(test, feature = "test-utils"))]
pub(super) const RELEASE_QUEUED_FILE_COUNT: &str = "SELECT COUNT(*) FROM release_files AS f \
     JOIN coven_uploads AS upload ON upload.file = f.file \
     WHERE f.release_id = ?1";

/// Whether any of the release `?1`'s files waits in this device's upload
/// queue.
#[cfg(any(test, feature = "test-utils"))]
pub(super) const RELEASE_HAS_QUEUED_FILE: &str = "SELECT EXISTS (SELECT 1 FROM release_files AS f \
     JOIN coven_uploads AS upload ON upload.file = f.file \
     WHERE f.release_id = ?1)";

/// Whether the release file `?1` waits in this device's upload queue.
#[cfg(any(test, feature = "test-utils"))]
pub(super) const RELEASE_FILE_QUEUED: &str = "SELECT EXISTS (SELECT 1 FROM release_files AS f \
     JOIN coven_uploads AS upload ON upload.file = f.file WHERE f.id = ?1)";

// Each release with files this process is sending now, and how many. One row
// per upload in flight is read, each release file found by its file.
sqlive_compose::fragment!(
    ReleaseSending { files: i64 },
    r#"
    SELECT f.release_id, COUNT(*) AS files
    FROM coven_sending AS sending
    JOIN release_files AS f ON f.file = sending.file
    GROUP BY f.release_id
"#
);

/// The columns of a release's storage row `storage`
/// ([`ReleaseStorageRows`]) that [`release_upload_in_row`] maps: how many of
/// the release's files wait, are being sent and are stored, and the name of
/// the release file whose missing original a move of the release waits for.
pub const RELEASE_UPLOAD_COLUMNS: &str = "storage.queued_count, storage.waiting_files, \
     storage.sending_files, storage.stored_files, storage.missing_original_file";

// What holds the upload queue back, as one row: `upload_wait` names the wait
// ([`UploadQueueHeld::wait`] gives what it names) and `upload_wait_at` is the earliest
// time coven gives a file's wait to end. Each file's wait is coven's, as
// `coven_upload_phases` gives it, read once. The first that holds names the
// wait: `paused` while the person paused uploads; `sync_stopped` while sync
// isn't running; nothing while a file is being sent; `offline` while coven
// says the device has no usable network; `metered` while a file waits for an
// unmetered network; `retrying` while a file waits for its retry or a
// provider's cooldown to end. NULL when nothing holds the queue.
sqlive_compose::fragment!(
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

/// How many of a release's files wait, are being sent and are stored, as
/// [`RELEASE_UPLOAD_COLUMNS`] read them; `None` when none of its files
/// is queued. A count below zero, or past a `u32`, is a column-conversion
/// error.
fn upload_counts_in_row(row: &Row<'_>) -> rusqlite::Result<Option<(u32, u32, u32)>> {
    let queued: i64 = row.get("queued_count")?;
    if queued == 0 {
        return Ok(None);
    }
    Ok(Some((
        bae_store::read::count_column(row, "waiting_files")?,
        bae_store::read::count_column(row, "sending_files")?,
        bae_store::read::count_column(row, "stored_files")?,
    )))
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

/// The release's upload as [`RELEASE_UPLOAD_COLUMNS`] read it; `None` when
/// none of its files waits.
pub fn release_upload_in_row(row: &Row<'_>) -> rusqlite::Result<Option<DbReleaseUpload>> {
    let Some((waiting, sending, stored)) = upload_counts_in_row(row)? else {
        return Ok(None);
    };
    let missing = if sending > 0 { None } else { row.get("missing_original_file")? };
    Ok(Some(DbReleaseUpload { waiting, sending, stored, missing }))
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
sqlive_compose::read!(
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
fn sending_uploads_on(sql: &impl bae_store::WriterRead) -> Result<DbSendingUploads, DbError> {
    let row = statement_row(sql, &SendingUploadsParams {})?;
    Ok(DbSendingUploads { files: row.files, releases: row.releases, paused: row.paused })
}

impl Records {
    /// How many of the release's files still wait to upload from this device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(RELEASE_QUEUED_FILE_COUNT, [release_id], |row| row.get(0))
                    .map_err(DbError::from)
            })
            .await
    }

    /// Whether any of the release's files still waits to upload from this
    /// device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(RELEASE_HAS_QUEUED_FILE, [release_id], |row| row.get(0))
                    .map_err(DbError::from)
            })
            .await
    }

    /// [`Self::has_pending_uploads_for_release`], live: it reruns as the
    /// release's files enter and leave the upload queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_has_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<bool, DbError> {
        let release_id = release_id.to_string();
        self.store.subscribe(move |sql| {
            Ok(sql.query_row(RELEASE_HAS_QUEUED_FILE, [&release_id], |row| row.get(0))?)
        })
    }

    /// The files this process is sending now and the releases they belong
    /// to with the bytes of each stored, live: it reruns as files start and
    /// stop being sent or the person pauses uploads. How far each file has
    /// got is coven's upload watch.
    pub fn subscribe_sending_uploads(&self) -> coven::LiveQuery<DbSendingUploads, DbError> {
        self.store.subscribe(|sql| sending_uploads_on(&sql))
    }

    /// Whether a release file's coven file still waits to upload from this
    /// device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn has_pending_cloud_upload(&self, file_id: &str) -> Result<bool, DbError> {
        let file_id = file_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(RELEASE_FILE_QUEUED, [file_id], |row| row.get(0))
                    .map_err(DbError::from)
            })
            .await
    }
}

#[cfg(test)]
#[path = "queued_uploads_tests.rs"]
mod tests;
