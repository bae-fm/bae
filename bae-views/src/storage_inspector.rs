//! The Storage Manager inspector's release: its files, each beside its
//! download or upload while one waits, and the release's entry in each
//! transfer queue.

use super::*;
use crate::album_detail::{FileDetail, ReleaseUpload};
use bae_records::records::release_with_files::ReleaseFileByIdParams;
use bae_store::at_most_one;
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
                ("core.queue.queued_to_pin", counts.waiting, TransferStage::Waiting),
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

/// Where a file of a release stands in the release's download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownloadPhase {
    Waiting,
    Downloading,
    Done,
}

impl rusqlite::types::FromSql for DownloadPhase {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        match value.as_str()? {
            "waiting" => Ok(Self::Waiting),
            "downloading" => Ok(Self::Downloading),
            "done" => Ok(Self::Done),
            other => Err(rusqlite::types::FromSqlError::Other(
                format!("{other:?} is no download phase").into(),
            )),
        }
    }
}

// Each file of a release with a queued or active download: the entry's kind,
// and where the file stands. A pin checks coven's pinned and downloading files.
// A move checks the copies coven has placed and the one it is downloading into
// the chosen folder.
coven::fragment!(
    ReleaseFileDownload { download_kind: DownloadKind, download_state: DownloadPhase },
    r#"
    SELECT file.file_id AS download_file_id, file.release_id AS download_release_id,
           queued.kind AS download_kind,
           CASE queued.kind
               WHEN 'pin' THEN CASE
                   WHEN pin.file IS NOT NULL THEN 'done'
                   WHEN downloading.file IS NOT NULL THEN 'downloading'
                   ELSE 'waiting' END
               WHEN 'move_to_this_device' THEN CASE
                   WHEN queued.state = 'queued' THEN 'waiting'
                   WHEN placed.file IS NOT NULL THEN 'done'
                   WHEN copying.file IS NOT NULL THEN 'downloading'
                   ELSE 'waiting' END
           END AS download_state
    FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
    JOIN @bae_transfers::download_entry::QueuedDownloadEntry AS queued
      ON queued.release_id = file.release_id AND queued.state <> 'failed'
    LEFT JOIN coven_pins AS pin ON queued.kind = 'pin' AND pin.file = file.stored_file
    LEFT JOIN coven_downloading AS downloading
           ON queued.kind = 'pin' AND downloading.file = file.stored_file
    LEFT JOIN coven_device_only_placed AS placed
           ON queued.kind = 'move_to_this_device' AND placed.file = file.stored_file
    LEFT JOIN coven_device_only_downloading AS copying
           ON queued.kind = 'move_to_this_device' AND copying.file = file.stored_file
"#
);

// Each file waiting in this device's upload queue: whether this process is
// sending it now, and the record of what it waits for.
coven::fragment!(
    QueuedUpload {
        uploading_file: coven::FileId,
        sending: bool,
        pending: Option<coven::PendingRecord>,
    },
    r#"
    SELECT upload.file AS uploading_file, upload.sending, upload.pending
    FROM coven_uploads AS upload
"#
);

// Each record of what coven's work waits for, with the text of the error behind
// it when coven keeps it.
coven::fragment!(
    KeptPending {
        record: coven::PendingRecord,
        cause: Option<String>,
    },
    r#"
    SELECT kept.position, kept.file, kept.record, kept.cause
    FROM coven_pending AS kept
"#
);

// The release `release` as the inspector's transfers show it: its album's
// title, its storage row (its files' count and size, its upload and its failed
// move), its entries in the download and export queues, and, while the download
// queue pins it, how many of its files are downloading, waiting and kept. No
// row for a release the library does not hold.
coven::read!(
    writer InspectedRelease(release: String) {
        struct PinCounts { downloading: u32, waiting: u32, kept: u32 }
        pins: json<Option<PinCounts>>,
    },
    r#"
    SELECT held.*, album.*, storage.*, download.*, output.*,
           (SELECT json_object(
                       'downloading',
                       COUNT(*) FILTER (WHERE pinning.download_state = 'downloading'),
                       'waiting',
                       COUNT(*) FILTER (WHERE pinning.download_state = 'waiting'),
                       'kept', COUNT(*) FILTER (WHERE pinning.download_state = 'done'))
            FROM @crate::storage_inspector::ReleaseFileDownload AS pinning
            WHERE pinning.download_release_id = :release AND pinning.download_kind = 'pin'
            HAVING COUNT(*) > 0) AS pins
    FROM @bae_records::records::fragments::ReleaseAlbum AS held
    JOIN @bae_records::records::fragments::AlbumTitle AS album
      ON album.album_id = held.album_id
    JOIN @bae_records::release_storage::ReleaseStorage AS storage
      ON storage.release_id = :release
    LEFT JOIN @bae_transfers::queued_releases::rows::DownloadQueueRow AS download
           ON download.release_id = :release
    LEFT JOIN @bae_transfers::queued_releases::rows::OutputQueueRow AS output
           ON output.release_id = :release
    WHERE held.release_id = :release
"#
);

// The files of the release `release` in natural filename order, each with its
// download while the release has one, its queued upload while it waits in the
// queue and the record that upload names. Apart from its upload, the file
// carries the record of the move that found its original missing or changed,
// matched by its path.
coven::read!(
    writer InspectedFiles(release: String),
    r#"
    SELECT file.*, download.*, upload.*, upload_record.*, move_record.*
    FROM @bae_records::records::file_fragments::ReleaseFileRecord AS file
    LEFT JOIN @crate::storage_inspector::ReleaseFileDownload AS download
           ON download.download_release_id = :release AND download.download_file_id = file.id
    LEFT JOIN @crate::storage_inspector::QueuedUpload AS upload
           ON upload.uploading_file = file.file
    LEFT JOIN @crate::storage_inspector::KeptPending AS upload_record
           ON upload_record.file = file.file AND upload_record.record = upload.pending
    LEFT JOIN coven_user_files AS original ON original.file = file.file
    LEFT JOIN @crate::storage_inspector::KeptPending AS move_record
           ON move_record.position = (SELECT MIN(failed.position) FROM coven_pending AS failed
                                      WHERE failed.subject_kind = 'move'
                                        AND failed.reason_kind = 'original'
                                        AND failed.original_path = original.path)
    WHERE file.release_id = :release
    ORDER BY file.original_filename COLLATE natural_order, file.id
"#
);

/// What [`InspectedRelease`] and [`InspectedFiles`] read of the release
/// `release_id`, in one snapshot.
struct InspectorRows {
    release_id: String,
    release: Vec<InspectedReleaseRow>,
    files: Vec<InspectedFilesRow>,
}

/// The reads of the release `release_id` as the inspector shows it.
fn storage_inspector_read(
    release_id: &str,
) -> coven::Both<InspectedReleaseParams, InspectedFilesParams> {
    let release = release_id.to_string();
    coven::Both(
        InspectedReleaseParams { release: release.clone() },
        InspectedFilesParams { release },
    )
}

impl InspectorRows {
    /// The release as the inspector's rows hold it; `None` when the library
    /// holds no such release.
    fn process(self) -> Result<Option<crate::DbStorageInspector>, DbError> {
        let Some(row) = at_most_one::<InspectedReleaseParams, _>(self.release)? else {
            return Ok(None);
        };
        let file_count = row.storage.file_count();
        let release = crate::DbInspectorRelease {
            title: row.album.clone().into_title(),
            file_count: u64::try_from(file_count)
                .map_err(|_error| DbError::unreadable("a release's file count", file_count))?,
            total_size: row.storage.total_size()?,
        };
        let failed_move = row.storage.failed_move().map(|(file_name, cause)| crate::DbFailedMove {
            file_name: file_name.to_string(),
            cause: cause.map(str::to_string),
        });
        Ok(Some(crate::DbStorageInspector {
            release_id: self.release_id,
            release,
            files: self.files.into_iter().map(inspector_file).collect::<Result<_, _>>()?,
            pins: row.pins.map(|pins| crate::DbPinCounts {
                downloading: pins.downloading,
                waiting: pins.waiting,
                kept: pins.kept,
            }),
            upload: row.storage.upload()?,
            failed_move,
            entries: ReleaseQueueEntries::of_rows(row.download, row.output, &row.held, &row.album)?,
        }))
    }
}

/// One file as the inspector shows it, from its row.
fn inspector_file(row: InspectedFilesRow) -> Result<DbStorageInspectorFile, DbError> {
    let file = row.file.into_file()?;
    let download = row.download.map(|download| {
        let kind = download.download_kind;
        match download.download_state {
            DownloadPhase::Waiting => FileDownload::Waiting { kind },
            DownloadPhase::Downloading => FileDownload::Downloading { kind, file: file.file },
            DownloadPhase::Done => FileDownload::Done,
        }
    });
    let cause = row.upload_record.and_then(|kept| kept.cause);
    let upload = row.upload.map(|upload| DbFileUpload {
        file: upload.uploading_file,
        sending: upload.sending,
        pending: upload.pending.map(|record| with_cause(record, cause)),
    });
    let move_record = row.move_record.map(|kept| with_cause(kept.record, kept.cause));
    Ok(DbStorageInspectorFile { file, download, upload, move_record })
}

/// `record` with the text of the error behind it, `cause`.
fn with_cause(mut record: coven::PendingRecord, cause: Option<String>) -> coven::PendingRecord {
    record.cause = cause;
    record
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
            .subscribe_reconfigurable(initial, |release_id| {
                Ok(coven::Maybe(release_id.as_deref().map(storage_inspector_read)))
            })
            .process(|release_id, rows| {
                let Some((release_id, (release, files))) = bae_store::requested(release_id, rows)
                else {
                    return Ok(None);
                };
                let rows = InspectorRows { release_id: release_id.clone(), release, files };
                Ok(rows.process()?.map(StorageInspector::read))
            })
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
            .subscribe_reconfigurable(initial, |file_id| {
                Ok(coven::Maybe(file_id.clone().map(|file| ReleaseFileByIdParams { file })))
            })
            .process(|_, rows| {
                let Some(row) =
                    rows.map(at_most_one::<ReleaseFileByIdParams, _>).transpose()?.flatten()
                else {
                    return Ok(None);
                };
                Ok(Some(FileDetail::from_raw(row.record.into_file()?)))
            })
    }
}
