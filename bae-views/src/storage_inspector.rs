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
}

/// A release's files as the inspector shows them, in the order of
/// `RELEASE_FILES_IN_ORDER`, each with where this device's pin of it stands
/// while the download queue pins the release (kept, downloading, or waiting)
/// and its upload while it waits in this device's queue, from coven's `coven_pins`,
/// `coven_downloading` and `coven_uploads`, each joined by the coven file the
/// row references, the text of the error behind the upload's record when
/// coven keeps one (`coven_pending`), and the path of the original this
/// device holds of it. A pin's download lists its file in
/// `coven_downloading` before `coven_pins` holds a row for it.
pub(crate) const INSPECTOR_FILES: &str = concat!(
    "SELECT ",
    bae_records::release_file_columns!(),
    ", CASE WHEN queued.kind = 'pin' AND queued.state <> 'failed' THEN \
            CASE WHEN pin.file IS NOT NULL THEN 'kept' \
                 WHEN downloading.file IS NOT NULL THEN 'downloading' \
                 ELSE 'waiting' END END AS pin_state, \
        upload.file AS upload_file, upload.sending AS upload_sending, \
        upload.pending AS upload_pending, upload_record.cause AS upload_cause, \
        original.path AS original_path \
     FROM release_files AS f \
     LEFT JOIN download_queue AS queued ON queued.release_id = f.release_id \
     LEFT JOIN coven_pins AS pin ON pin.file = f.file \
     LEFT JOIN coven_downloading AS downloading ON downloading.file = f.file \
     LEFT JOIN coven_uploads AS upload ON upload.file = f.file \
     LEFT JOIN coven_pending AS upload_record \
        ON upload_record.file = f.file AND upload_record.record = upload.pending \
     LEFT JOIN coven_user_files AS original ON original.file = f.file \
     WHERE f.release_id = ?1 \
     ORDER BY f.original_filename COLLATE natural_order, f.id"
);

/// How many of the release's files this device keeps, is downloading and has
/// yet to download, and whether the download queue holds the release to pin it
/// without having failed, which is when those counts show.
const PIN_COUNTS: &str = "SELECT \
        EXISTS (SELECT 1 FROM download_queue AS queued \
                WHERE queued.release_id = ?1 AND queued.kind = 'pin' \
                  AND queued.state <> 'failed') AS pinning, \
        COUNT(*) FILTER (WHERE pin.file IS NULL AND downloading.file IS NOT NULL), \
        COUNT(*) FILTER (WHERE pin.file IS NULL AND downloading.file IS NULL), \
        COUNT(*) FILTER (WHERE pin.file IS NOT NULL) \
     FROM release_files AS f \
     LEFT JOIN coven_pins AS pin ON pin.file = f.file \
     LEFT JOIN coven_downloading AS downloading ON downloading.file = f.file \
     WHERE f.release_id = ?1";

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

/// The records of moves whose upload found an original missing or changed,
/// with the error behind each when coven keeps one: what an upload whose own
/// record says nothing waits for. coven keeps them on the move, naming the
/// original by its path.
const MOVE_RECORDS: &str = "SELECT record, cause FROM coven_pending WHERE file IS NULL";

fn inspector_files_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbStorageInspectorFile>, DbError> {
    let moves: Vec<coven::PendingRecord> = sql
        .query(MOVE_RECORDS, [], |row| {
            let mut record = row.get::<_, coven::PendingRecord>(0)?;
            record.cause = row.get(1)?;
            Ok(record)
        })?
        .into_iter()
        .filter(|record| matches!(record.subject, coven::PendingSubject::Move { .. }))
        .collect();
    // The record of the move whose upload found the original whose path's
    // bytes are `path`.
    let move_record = |path: &[u8]| {
        moves
            .iter()
            .find(|record| {
                matches!(&record.reason, coven::PendingReason::Original { path: original, .. }
                    if coven::path_bytes(original) == path)
            })
            .cloned()
    };
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
                pending: match row.get::<_, Option<coven::PendingRecord>>("upload_pending")? {
                    Some(mut record) => {
                        record.cause = row.get("upload_cause")?;
                        Some(record)
                    }
                    None => row
                        .get::<_, Option<Vec<u8>>>("original_path")?
                        .and_then(|path| move_record(&path)),
                },
            }),
            None => None,
        };
        Ok(DbStorageInspectorFile { file, pin, upload })
    })?)
}
