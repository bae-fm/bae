//! The Storage Manager inspector's release: its files, each beside its
//! download or upload while one waits, and the release's entry in each
//! transfer queue.

use super::*;
use crate::album_detail::{FileDetail, ReleaseUpload};
pub use bae_sync_status::FileProblem;
use bae_sync_status::PendingWait;
use bae_transfers::{DownloadKind, DownloadRow, OutputPhaseRow, ReleaseQueueEntries};

/// What the inspector shows of one release, read in core for the release id
/// it is set to.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspector {
    /// The release's entry in each transfer queue that holds one: downloads,
    /// then exports, then uploads.
    pub transfers: Vec<StorageInspectorTransfer>,
    pub files: Vec<StorageInspectorFile>,
}

/// One row of the inspector's file list: a release file and its upload,
/// if one is queued; where the file's download stands while the queue pins
/// or moves the release; and what is wrong with its original, when a move found
/// it missing or changed.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspectorFile {
    pub file: FileDetail,
    pub upload: Option<StorageInspectorUpload>,
    pub download: Option<FileDownload>,
    pub problem: Option<FileProblem>,
}

/// A file of a release the download queue pins or moves to this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDownload {
    Waiting {
        kind: DownloadKind,
    },
    /// How far the download has come is the transfer keyed by this file.
    Downloading {
        kind: DownloadKind,
        file: coven::FileId,
    },
    /// The pin holds the file whole or the move has placed its copy.
    Done,
}

/// A release file's queued upload as the inspector shows it: which file,
/// whether it is being sent now, and why it waits when it does, with the
/// error behind that when coven keeps it. How far it has got is its
/// file's transfer, keyed by `file`.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspectorUpload {
    pub file: coven::FileId,
    pub sending: bool,
    pub pending: Option<PendingWait>,
}

/// The release's entry in one transfer queue. Pausing a whole queue is the
/// Storage Manager's footer's, not the release's.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageInspectorTransfer {
    /// `summary` counts the release's files downloading, waiting and kept,
    /// each left out at zero, while its pin waits or runs.
    Download {
        operation: DownloadRow,
        summary: Option<Vec<bae_transfers::TransferCount>>,
    },
    Output {
        operation: OutputPhaseRow,
    },
    /// The release's files waiting in this device's upload queue, with the
    /// release's album title and its files' count and size, which name the
    /// upload while it waits for a missing original.
    Upload {
        release_id: String,
        release: crate::DbInspectorRelease,
        upload: ReleaseUpload,
    },
    /// The release's move to the cloud, failed because the original of
    /// `file_name` changed, with the release's album title and its files'
    /// count and size and the text of the error behind the failure when coven
    /// kept it. coven keeps the move failed until the person cancels it.
    MoveFailed {
        release_id: String,
        release: crate::DbInspectorRelease,
        file_name: String,
        cause: Option<String>,
    },
}

impl StorageInspector {
    /// The release `rows` are of, as the inspector shows it: its files in
    /// their order, each upload's record with the error behind it; its entry
    /// in each queue and its files waiting to upload; and, while its download
    /// pins it, the download's count of its files by where their pins stand.
    pub fn read(rows: crate::DbStorageInspector) -> Self {
        let crate::DbStorageInspector {
            release_id,
            release,
            files,
            pins,
            upload,
            failed_move,
            entries,
        } = rows;
        let files: Vec<StorageInspectorFile> = files
            .into_iter()
            .map(|file| StorageInspectorFile {
                upload: file.upload.map(|upload| StorageInspectorUpload {
                    file: upload.file,
                    sending: upload.sending,
                    pending: upload.pending.map(|record| PendingWait::of(&record)),
                }),
                download: file.download,
                problem: file
                    .move_record
                    .as_ref()
                    .and_then(|record| FileProblem::of_reason(&record.reason)),
                file: FileDetail::from_raw(file.file),
            })
            .collect();
        let pins = pins.map(|counts| {
            use bae_transfers::TransferStage;
            bae_transfers::transfer_summary([
                ("core.queue.pinning", counts.downloading, TransferStage::Moving),
                ("core.queue.waiting", counts.waiting, TransferStage::Waiting),
                ("core.queue.pinned", counts.kept, TransferStage::Done),
            ])
        });
        let upload = upload.map(ReleaseUpload::from_raw);
        Self {
            transfers: storage_inspector_transfers(
                &release_id,
                release,
                &entries,
                upload,
                failed_move,
                pins,
            ),
            files,
        }
    }
}

/// The release's entry in each queue, downloads first, then exports, then
/// its files waiting to upload (`upload`), named by `release`, then its
/// failed move (the file whose original changed, with the text of the error
/// behind it); `pins` summarizes its files for the download that pins them.
fn storage_inspector_transfers(
    release_id: &str,
    release: crate::DbInspectorRelease,
    entries: &ReleaseQueueEntries,
    upload: Option<ReleaseUpload>,
    failed_move: Option<crate::DbFailedMove>,
    pins: Option<Vec<bae_transfers::TransferCount>>,
) -> Vec<StorageInspectorTransfer> {
    let mut items = Vec::new();
    if let Some(operation) = &entries.download {
        items.push(StorageInspectorTransfer::Download {
            operation: operation.clone(),
            summary: pins,
        });
    }
    if let Some(operation) = &entries.output {
        items.push(StorageInspectorTransfer::Output { operation: operation.clone() });
    }
    if let Some(upload) = upload {
        items.push(StorageInspectorTransfer::Upload {
            release_id: release_id.to_string(),
            release: release.clone(),
            upload,
        });
    }
    if let Some(crate::DbFailedMove { file_name, cause }) = failed_move {
        items.push(StorageInspectorTransfer::MoveFailed {
            release_id: release_id.to_string(),
            release,
            file_name,
            cause,
        });
    }
    items
}

#[cfg(test)]
#[path = "storage_inspector_tests.rs"]
mod tests;

/// The release `release_id` as the inspector shows it, `None` when the
/// library holds no such release: its files with their downloads and uploads,
/// what of it waits to upload, and its download and export entries.
fn storage_inspector_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbStorageInspector>, DbError> {
    let Some(release) =
        sql.query_row(INSPECTOR_RELEASE, params![release_id], inspector_release).optional()?
    else {
        return Ok(None);
    };
    Ok(Some(DbStorageInspector {
        release_id: release_id.to_string(),
        release,
        files: inspector_files_on(sql, release_id)?,
        pins: pin_counts_on(sql, release_id)?,
        upload: bae_records::records::queued_uploads::release_upload_on(sql, release_id)?,
        failed_move: sql
            .query_row(FAILED_MOVE, params![release_id], |row| {
                Ok(crate::DbFailedMove { file_name: row.get(0)?, cause: row.get(1)? })
            })
            .optional()?,
        entries: bae_transfers::tables::release_queue_entries_on(sql, release_id)?,
    }))
}

/// The release `?1`'s album title and its files' count and total size, as
/// `release_storage` keeps them; no row when the library holds no such
/// release.
const INSPECTOR_RELEASE: &str = "SELECT a.title AS title, storage.file_count AS file_count, \
         storage.total_size AS total_size \
     FROM releases AS r \
     JOIN albums AS a ON a.id = r.album_id \
     JOIN release_storage AS storage ON storage.release_id = r.id \
     WHERE r.id = ?1";

/// The release `?1`'s failed move as `release_storage` keeps it: the name of
/// the file whose original changed and the text of the error behind the
/// failure; no row while no move failed.
const FAILED_MOVE: &str = "SELECT failed_move_file, failed_move_cause FROM release_storage \
     WHERE release_id = ?1 AND failed_move_file IS NOT NULL";

/// One row of [`INSPECTOR_RELEASE`].
fn inspector_release(row: &Row<'_>) -> rusqlite::Result<crate::DbInspectorRelease> {
    Ok(crate::DbInspectorRelease {
        title: row.get("title")?,
        file_count: count_column(row, "file_count")?,
        total_size: count_column(row, "total_size")?,
    })
}

impl LibraryViews {
    /// Follow the release `initial` names, or none, as the Storage Manager's
    /// inspector shows it, each value read in one go and none while no such
    /// release exists. Another release is a
    /// new request on the same query.
    pub fn subscribe_storage_inspector(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<StorageInspector>, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |release_id, sql| {
                let Some(release_id) = release_id else {
                    return Ok(None);
                };
                storage_inspector_on(&sql, release_id)
            })
            .process(|rows| Ok(rows.map(StorageInspector::read)))
    }

    /// Follow the release file `initial` names, or none, as the inspector's
    /// file details show it; the query returns none while no such file
    /// exists. Another file is a
    /// new request on the same query.
    pub fn subscribe_storage_file(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<FileDetail>, bae_store::DbError>
    {
        self.store
            .subscribe_reconfigurable(initial, |file_id, sql| {
                let Some(file_id) = file_id else {
                    return Ok(None);
                };
                Ok(sql.query_row(STORAGE_FILE, params![file_id], row_to_file).optional()?)
            })
            .process(|row| Ok(row.map(FileDetail::from_raw)))
    }
}

/// One release file, by its id.
const STORAGE_FILE: &str = concat!(
    "SELECT ",
    bae_records::release_file_columns!(),
    " FROM release_files AS f WHERE f.id = ?1"
);

/// Each file's phase in the release's queued or active download. A pin
/// checks coven's pinned and downloading files. A move checks the copies
/// coven has placed and the one it is downloading into the chosen folder.
macro_rules! file_downloads {
    () => {
        "WITH file_downloads AS (SELECT f.id AS file_row, queued.kind AS download_kind, \
            CASE queued.kind \
                WHEN 'pin' THEN CASE \
                    WHEN pin.file IS NOT NULL THEN 'done' \
                    WHEN downloading.file IS NOT NULL THEN 'downloading' \
                    ELSE 'waiting' END \
                WHEN 'move_to_this_device' THEN CASE \
                    WHEN queued.state = 'queued' THEN 'waiting' \
                    WHEN placed.file IS NOT NULL THEN 'done' \
                    WHEN copying.file IS NOT NULL THEN 'downloading' \
                    ELSE 'waiting' END \
            END AS download_state \
         FROM release_files AS f \
         LEFT JOIN download_queue AS queued \
            ON queued.release_id = f.release_id AND queued.state <> 'failed' \
         LEFT JOIN coven_pins AS pin ON queued.kind = 'pin' AND pin.file = f.file \
         LEFT JOIN coven_downloading AS downloading \
            ON queued.kind = 'pin' AND downloading.file = f.file \
         LEFT JOIN coven_device_only_placed AS placed \
            ON queued.kind = 'move_to_this_device' AND placed.file = f.file \
         LEFT JOIN coven_device_only_downloading AS copying \
            ON queued.kind = 'move_to_this_device' AND copying.file = f.file \
         WHERE f.release_id = ?1) "
    };
}

/// A release's files in natural filename order, with each file's download,
/// its queued upload and the error behind its upload record when coven
/// keeps one. Apart from its upload, the file carries the record of the move
/// that found its original missing or changed, matched by its path, with
/// the error behind it.
pub(crate) const INSPECTOR_FILES: &str = concat!(
    file_downloads!(),
    "SELECT ",
    bae_records::release_file_columns!(),
    ", file_downloads.download_kind, file_downloads.download_state, \
        upload.file AS upload_file, upload.sending AS upload_sending, \
        upload.pending AS upload_pending, upload_record.cause AS upload_cause, \
        move_record.record AS move_pending, move_record.cause AS move_cause \
     FROM release_files AS f \
     JOIN file_downloads ON file_downloads.file_row = f.id \
     LEFT JOIN coven_uploads AS upload ON upload.file = f.file \
     LEFT JOIN coven_pending AS upload_record \
        ON upload_record.file = f.file AND upload_record.record = upload.pending \
     LEFT JOIN coven_user_files AS original ON original.file = f.file \
     LEFT JOIN coven_pending AS move_record \
        ON move_record.position = (SELECT MIN(m.position) FROM coven_pending AS m \
                                   WHERE m.subject_kind = 'move' AND m.reason_kind = 'original' \
                                     AND m.original_path = original.path) \
     WHERE f.release_id = ?1 \
     ORDER BY f.original_filename COLLATE natural_order, f.id"
);

/// How many of the release's files this device keeps, is downloading and has
/// yet to download, from each file's download (`file_downloads!`), and whether the
/// download queue pins the release, which is when those counts show.
const PIN_COUNTS: &str = concat!(
    file_downloads!(),
    "SELECT COUNT(*) > 0 AS pinning, \
        COUNT(*) FILTER (WHERE download_state = 'downloading'), \
        COUNT(*) FILTER (WHERE download_state = 'waiting'), \
        COUNT(*) FILTER (WHERE download_state = 'done') \
     FROM file_downloads WHERE download_kind = 'pin'"
);

fn pin_counts_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<crate::DbPinCounts>, DbError> {
    Ok(sql.query_row(PIN_COUNTS, params![release_id], |row| {
        Ok(row.get::<_, bool>(0)?.then_some(crate::DbPinCounts {
            downloading: row.get(1)?,
            waiting: row.get(2)?,
            kept: row.get(3)?,
        }))
    })?)
}

fn inspector_files_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbStorageInspectorFile>, DbError> {
    Ok(sql.query(INSPECTOR_FILES, params![release_id], |row| {
        let file = row_to_file(row)?;
        let download = match row.get::<_, Option<String>>("download_state")?.as_deref() {
            Some("done") => Some(FileDownload::Done),
            Some("downloading") => {
                Some(FileDownload::Downloading { kind: row.get("download_kind")?, file: file.file })
            }
            Some("waiting") => Some(FileDownload::Waiting { kind: row.get("download_kind")? }),
            Some(state) => {
                return Err(bae_store::read::column_conversion_error(row, "download_state", state))
            }
            None => None,
        };
        let upload = match row.get::<_, Option<coven::FileId>>("upload_file")? {
            Some(file) => Some(DbFileUpload {
                file,
                sending: row.get("upload_sending")?,
                pending: {
                    let mut record =
                        row.get::<_, Option<coven::PendingRecord>>("upload_pending")?;
                    if let Some(record) = &mut record {
                        record.cause = row.get("upload_cause")?;
                    }
                    record
                },
            }),
            None => None,
        };
        let mut move_record = row.get::<_, Option<coven::PendingRecord>>("move_pending")?;
        if let Some(record) = &mut move_record {
            record.cause = row.get("move_cause")?;
        }
        Ok(DbStorageInspectorFile { file, download, upload, move_record })
    })?)
}
