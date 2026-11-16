//! The Storage Manager inspector's release: its files, each beside its
//! upload while one waits, and the release's entry in each transfer queue.

use super::*;
use crate::album_detail::{FileDetail, FilePin, ReleaseUpload};
use bae_sync_status::PendingWait;
use bae_transfers::{DownloadRow, OutputPhaseRow, ReleaseQueueEntries};

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
/// if one is queued; and while the download queue pins the release, where the
/// file's pin stands.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspectorFile {
    pub file: FileDetail,
    pub upload: Option<StorageInspectorUpload>,
    pub pin: Option<FilePin>,
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
    /// The release's files waiting in this device's upload queue.
    Upload {
        release_id: String,
        upload: ReleaseUpload,
    },
}

impl StorageInspector {
    /// The release `rows` are of, as the inspector shows it: its files in
    /// their order, each upload's record with the error behind it; its entry
    /// in each queue and its files waiting to upload; and, while its download
    /// pins it, the download's count of its files by where their pins stand.
    pub fn read(rows: crate::DbStorageInspector) -> Self {
        let crate::DbStorageInspector { release_id, files, pins, upload, entries } = rows;
        let files: Vec<StorageInspectorFile> = files
            .into_iter()
            .map(|file| StorageInspectorFile {
                upload: file.upload.map(|upload| StorageInspectorUpload {
                    file: upload.file,
                    sending: upload.sending,
                    pending: upload
                        .pending
                        .map(|record| PendingWait::of(&record, Some(&release_id))),
                }),
                pin: file.pin,
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
        Self { transfers: storage_inspector_transfers(&release_id, &entries, upload, pins), files }
    }
}

/// The release's entry in each queue, downloads first, then exports, then
/// its files waiting to upload (`upload`); `pins` summarizes its files for
/// the download that pins them.
fn storage_inspector_transfers(
    release_id: &str,
    entries: &ReleaseQueueEntries,
    upload: Option<ReleaseUpload>,
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
        items.push(StorageInspectorTransfer::Upload { release_id: release_id.to_string(), upload });
    }
    items
}

#[cfg(test)]
#[path = "storage_inspector_tests.rs"]
mod tests;

/// The release `release_id` as the inspector shows it, `None` when the
/// library holds no such release: its files with their pins and uploads,
/// what of it waits to upload, and its download and export entries.
fn storage_inspector_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbStorageInspector>, DbError> {
    if find_release_by_id_on(sql, release_id)?.is_none() {
        return Ok(None);
    }
    Ok(Some(DbStorageInspector {
        release_id: release_id.to_string(),
        files: inspector_files_on(sql, release_id)?,
        pins: pin_counts_on(sql, release_id)?,
        upload: bae_records::records::queued_uploads::release_upload_on(sql, release_id)?,
        entries: bae_transfers::tables::release_queue_entries_on(sql, release_id)?,
    }))
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
    /// file details show it; none while no such file exists. Another file is a
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

/// Where this device's pin of each file of the release `?1` stands while the
/// download queue pins the release without having failed (kept, downloading,
/// or waiting), NULL while it doesn't, as the CTE `pins` with each file row's
/// id. A pin's download lists its file in `coven_downloading` before
/// `coven_pins` holds a row for it. The CTE is a macro so that the files
/// statement and the counts statement use the same SQL for each file's pin.
macro_rules! file_pins {
    () => {
        "WITH pins AS (SELECT f.id AS file_row, \
            CASE WHEN queued.kind = 'pin' AND queued.state <> 'failed' THEN \
                CASE WHEN pin.file IS NOT NULL THEN 'kept' \
                     WHEN downloading.file IS NOT NULL THEN 'downloading' \
                     ELSE 'waiting' END END AS pin_state \
         FROM release_files AS f \
         LEFT JOIN download_queue AS queued ON queued.release_id = f.release_id \
         LEFT JOIN coven_pins AS pin ON pin.file = f.file \
         LEFT JOIN coven_downloading AS downloading ON downloading.file = f.file \
         WHERE f.release_id = ?1) "
    };
}

/// A release's files as the inspector shows them, in the order of
/// `RELEASE_FILES_IN_ORDER`, each with where this device's pin of it stands
/// while the download queue pins the release (kept, downloading, or waiting)
/// and its upload while it waits in this device's queue, from coven's `coven_pins`,
/// `coven_downloading` and `coven_uploads`, each joined by the coven file the
/// row references, the text of the error behind the upload's record when
/// coven keeps one (`coven_pending`); a file with no upload record of its own
/// shows the record of the move whose upload found its original, named by its
/// path, missing or changed. Each file's pin is the one `file_pins!` states.
pub(crate) const INSPECTOR_FILES: &str = concat!(
    file_pins!(),
    "SELECT ",
    bae_records::release_file_columns!(),
    ", pins.pin_state AS pin_state, \
        upload.file AS upload_file, upload.sending AS upload_sending, \
        COALESCE(upload.pending, move_record.record) AS upload_pending, \
        CASE WHEN upload.pending IS NOT NULL THEN upload_record.cause \
             ELSE move_record.cause END AS upload_cause \
     FROM release_files AS f \
     JOIN pins ON pins.file_row = f.id \
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
/// yet to download, from each file's pin (`file_pins!`), and whether the
/// download queue pins the release, which is when those counts show.
const PIN_COUNTS: &str = concat!(
    file_pins!(),
    "SELECT COALESCE(MAX(pin_state IS NOT NULL), 0) AS pinning, \
        COUNT(*) FILTER (WHERE pin_state = 'downloading'), \
        COUNT(*) FILTER (WHERE pin_state = 'waiting'), \
        COUNT(*) FILTER (WHERE pin_state = 'kept') \
     FROM pins"
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
        let pin = match row.get::<_, Option<String>>("pin_state")?.as_deref() {
            Some("kept") => Some(crate::album_detail::FilePin::Kept),
            Some("downloading") => {
                Some(crate::album_detail::FilePin::Downloading { file: file.file })
            }
            Some(_) => Some(crate::album_detail::FilePin::Waiting),
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
        Ok(DbStorageInspectorFile { file, pin, upload })
    })?)
}
