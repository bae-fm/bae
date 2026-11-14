//! What each Activity transfer line's bar counts from the library's tables:
//! each release of the line's batch with its size and how much of it the
//! tables say is done. How far the files and releases on the move have got
//! is not here: it changes with every byte, and travels with the transfers.

use crate::sync_activity::DownloadLine;
use bae_records::records::queued_uploads::{sending_files_on, RELEASE_BYTES_STORED};
use bae_store::read::{count_column, optional_count_column};
use bae_store::DbError;
use std::collections::BTreeMap;

/// Each transfer line's batch, release by release, as one read finds it.
/// Bytes moving change none of it, so they rerun nothing; a batch growing,
/// a release's files stored or pinned, and a file starting or ending its
/// upload do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityBytes {
    pub uploads: Option<BatchReleases<UploadRelease>>,
    pub pins: Option<BatchReleases<DownloadRelease>>,
    pub moving_to_this_device: Option<BatchReleases<DownloadRelease>>,
}

/// The releases one line's batch holds, while it holds any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchReleases<Release> {
    /// Which of its line's batches this is, each one past the one before:
    /// a reader tells by it that the batch it held ended and another began.
    pub number: u64,
    /// Each release by id; `None` for one the library no longer holds.
    pub releases: BTreeMap<String, Option<Release>>,
}

/// A release of the Uploading line's batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadRelease {
    /// Its whole size.
    pub bytes_total: u64,
    /// Of its size, the bytes of its files stored: those not queued to
    /// upload.
    pub bytes_stored: u64,
    /// Each of its files this process is sending now, with the file's size.
    pub sending: BTreeMap<coven::FileId, u64>,
}

/// A release of the Pinning or Moving to this device line's batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadRelease {
    /// Its whole size.
    pub bytes_total: u64,
    /// No entry of the line's kind for it is in the download queue, and it
    /// is pinned whole, or for a move-to-this-device no longer stored in the cloud.
    pub done: bool,
}

/// Every transfer line's batch, release by release.
pub fn activity_bytes_on(sql: &coven::SqlReadContext<'_>) -> Result<ActivityBytes, DbError> {
    Ok(ActivityBytes {
        uploads: upload_bytes_on(sql)?,
        pins: download_bytes_on(sql, &DownloadLine::PINS)?,
        moving_to_this_device: download_bytes_on(sql, &DownloadLine::MOVING_TO_THIS_DEVICE)?,
    })
}

/// The Uploading line's batch, each release with its stored bytes and its
/// files being sent. A cover's or a picture's file belongs to no release of
/// the batch.
fn upload_bytes_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<Option<BatchReleases<UploadRelease>>, DbError> {
    let mut releases: BTreeMap<String, Option<UploadRelease>> = sql
        .query(
            &format!(
                "SELECT batch.release_id AS release_id, storage.total_size AS bytes_total, \
                     {RELEASE_BYTES_STORED} AS bytes_stored \
                 FROM upload_batch AS batch \
                 LEFT JOIN release_storage AS storage ON storage.release_id = batch.release_id"
            ),
            [],
            |row| {
                let release = match optional_count_column(row, "bytes_total")? {
                    Some(bytes_total) => Some(UploadRelease {
                        bytes_total,
                        bytes_stored: count_column(row, "bytes_stored")?,
                        sending: BTreeMap::new(),
                    }),
                    None => None,
                };
                Ok((row.get("release_id")?, release))
            },
        )?
        .into_iter()
        .collect();
    if releases.is_empty() {
        return Ok(None);
    }
    // A cover's or a picture's file has no release. A release's file being
    // sent is queued, so its release is in the batch and still held.
    for file in sending_files_on(sql)? {
        if let Some(release_id) = &file.release_id {
            let release = releases
                .get_mut(release_id)
                .and_then(Option::as_mut)
                .expect("a release with a file queued is in the upload batch");
            let size = file.file_size.expect("a release file being sent has its size read with it");
            release.sending.insert(file.file, size);
        }
    }
    let number = sql.query_row("SELECT number FROM upload_batch_number", [], |row| {
        count_column(row, "number")
    })?;
    Ok(Some(BatchReleases { number, releases }))
}

/// `line`'s batch, each release with whether it is done.
fn download_bytes_on(
    sql: &coven::SqlReadContext<'_>,
    line: &DownloadLine,
) -> Result<Option<BatchReleases<DownloadRelease>>, DbError> {
    let releases: BTreeMap<String, Option<DownloadRelease>> = sql
        .query(
            &format!(
                "SELECT batch.release_id AS release_id, storage.total_size AS bytes_total, \
                     ({}) AS done \
                 FROM download_batch AS batch \
                 LEFT JOIN release_storage AS storage ON storage.release_id = batch.release_id \
                 WHERE batch.kind = ?1",
                line.done()
            ),
            [line.kind],
            |row| {
                let release = match optional_count_column(row, "bytes_total")? {
                    Some(bytes_total) => {
                        Some(DownloadRelease { bytes_total, done: row.get("done")? })
                    }
                    None => None,
                };
                Ok((row.get("release_id")?, release))
            },
        )?
        .into_iter()
        .collect();
    if releases.is_empty() {
        return Ok(None);
    }
    let number = sql.query_row(
        "SELECT number FROM download_batch_number WHERE kind = ?1",
        [line.kind],
        |row| count_column(row, "number"),
    )?;
    Ok(Some(BatchReleases { number, releases }))
}

#[cfg(test)]
#[path = "activity_bytes_tests.rs"]
mod tests;
