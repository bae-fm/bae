//! The Storage Manager inspector's release: its files, each beside its
//! upload while one waits, and the release's entry in each transfer queue.

use super::*;
use crate::album_detail::{FileDetail, FilePin, ReleaseUpload};
use bae_db::PendingWait;
use bae_transfers::ReleaseQueueState;
use bae_transfers::{DownloadRow, OutputPhaseRow, ReleaseQueueEntries};
use coven::UploadPhase;

/// What the inspector shows of one release, read in core for the release id
/// it is set to.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspector {
    /// The release's entry in each transfer queue that holds one: downloads,
    /// then exports, then uploads; while coven downloads the files this
    /// device keeps here, that fetch after its download.
    pub transfers: Vec<StorageInspectorTransfer>,
    pub files: Vec<StorageInspectorFile>,
}

/// One row of the inspector's file list: a release file and its upload,
/// if one is queued; and while the release is pinning, where the file's pin
/// stands: every file while the download queue pins the release, and each
/// file coven keeps here while coven downloads them.
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
    pub phase: UploadPhase,
    pub pending: Option<PendingWait>,
}

/// The release's entry in one transfer queue. Pausing a whole queue is the
/// Storage Manager's footer's, not the release's.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageInspectorTransfer {
    /// `summary` counts the release's files downloading, waiting and kept,
    /// each left out at zero, while its download waits or runs.
    Download {
        operation: DownloadRow,
        summary: Option<Vec<bae_transfers::TransferCount>>,
    },
    /// Coven downloading the release's files a move to the cloud asked this
    /// device to keep here, outside the download queue: `summary` counts the
    /// files downloading and kept, each left out at zero.
    PinFetch {
        summary: Vec<bae_transfers::TransferCount>,
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
    /// The release `release_id` from one read of it, `rows`.
    pub fn read(release_id: &str, rows: crate::DbStorageInspector) -> Self {
        Self::of(release_id, rows.files, &rows.entries, rows.upload.map(ReleaseUpload::from_raw))
    }

    /// The release `release_id`: its `files` in their order, each upload's
    /// record with the error behind it; its entry in each queue
    /// (`entries`, and its files waiting to upload, `upload`); while its
    /// download waits or runs, each file's pin and the download's count of
    /// them; and otherwise, while coven downloads a file it keeps here, each
    /// file's pin and coven's fetch counting them.
    pub(crate) fn of(
        release_id: &str,
        files: Vec<crate::DbStorageInspectorFile>,
        entries: &ReleaseQueueEntries,
        upload: Option<ReleaseUpload>,
    ) -> Self {
        let pinning = if entries
            .download
            .as_ref()
            .is_some_and(|row| !matches!(row.op.state, ReleaseQueueState::Failed { .. }))
        {
            Some(Pinning::Download)
        } else if files.iter().any(|file| file.pin.is_some_and(|pin| pin != FilePin::Kept)) {
            Some(Pinning::Fetch)
        } else {
            None
        };
        let files: Vec<StorageInspectorFile> = files
            .into_iter()
            .map(|file| StorageInspectorFile {
                upload: file.upload.map(|upload| StorageInspectorUpload {
                    file: upload.file,
                    phase: if upload.sending {
                        UploadPhase::Uploading
                    } else {
                        UploadPhase::Waiting
                    },
                    pending: upload
                        .pending
                        .map(|record| PendingWait::of(&record, Some(release_id))),
                }),
                pin: match pinning {
                    Some(Pinning::Download) => Some(file.pin.unwrap_or(FilePin::Waiting)),
                    Some(Pinning::Fetch) => file.pin,
                    None => None,
                },
                file: FileDetail::from_raw(file.file),
            })
            .collect();
        let count = |stands: fn(&FilePin) -> bool| {
            u32::try_from(files.iter().filter(|file| file.pin.as_ref().is_some_and(stands)).count())
                .expect("a release's file count")
        };
        let pins = pinning.map(|pinning| {
            use bae_transfers::TransferStage;
            (
                pinning,
                bae_transfers::transfer_summary([
                    (
                        "core.queue.pinning",
                        count(|pin| matches!(pin, FilePin::Downloading { .. })),
                        TransferStage::Moving,
                    ),
                    (
                        "core.queue.waiting",
                        count(|pin| matches!(pin, FilePin::Waiting)),
                        TransferStage::Waiting,
                    ),
                    (
                        "core.queue.pinned",
                        count(|pin| matches!(pin, FilePin::Kept)),
                        TransferStage::Done,
                    ),
                ]),
            )
        });
        Self { transfers: storage_inspector_transfers(release_id, entries, upload, pins), files }
    }
}

/// What brings a pinning release's files to this device: its entry in the
/// download queue while it waits or runs, or else coven downloading the
/// files it keeps here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pinning {
    Download,
    Fetch,
}

/// The release's entry in each queue, downloads first, then coven's fetch of
/// the files it keeps here, then exports, then its files waiting to
/// upload (`upload`); `pins` summarizes its files for whichever pins them.
fn storage_inspector_transfers(
    release_id: &str,
    entries: &ReleaseQueueEntries,
    upload: Option<ReleaseUpload>,
    pins: Option<(Pinning, Vec<bae_transfers::TransferCount>)>,
) -> Vec<StorageInspectorTransfer> {
    let mut items = Vec::new();
    let (download_summary, fetch_summary) = match pins {
        Some((Pinning::Download, summary)) => (Some(summary), None),
        Some((Pinning::Fetch, summary)) => (None, Some(summary)),
        None => (None, None),
    };
    if let Some(operation) = &entries.download {
        items.push(StorageInspectorTransfer::Download {
            operation: operation.clone(),
            summary: download_summary,
        });
    }
    if let Some(summary) = fetch_summary {
        items.push(StorageInspectorTransfer::PinFetch { summary });
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
        files: inspector_files_on(sql, release_id)?,
        upload: bae_db::client::queued_uploads::release_upload_on(sql, release_id)?,
        entries: bae_transfers::tables::release_queue_entries_on(sql, release_id)?,
    }))
}

impl LibraryViews {
    /// Follow the release `initial` names, or none, as the Storage Manager's
    /// inspector shows it, each value read in one go. Another release is a
    /// new request on the same query.
    pub fn subscribe_storage_inspector(
        &self,
        initial: Option<String>,
    ) -> coven::ReconfigurableLiveQuery<Option<String>, Option<DbStorageInspector>> {
        self.store
            .subscribe_reconfigurable(initial, |release_id, sql| {
                let Some(release_id) = release_id else {
                    return Ok(None);
                };
                storage_inspector_on(&sql, release_id).map_err(CovenError::from)
            })
            .process(Ok)
    }
}

/// A release's files as the inspector shows them, in the order of
/// `RELEASE_FILES_IN_ORDER`, each with where this device's pin of it stands
/// and its upload while it waits in this device's queue, from coven's `coven_pins`,
/// `coven_downloading` and `coven_uploads`, each joined by the coven file the
/// row references, the text of the error behind the upload's record when
/// coven keeps one (`coven_pending`), and the path of the original this
/// device holds of it. A pin's download lists its file in
/// `coven_downloading` before `coven_pins` holds a row for it.
pub(crate) const INSPECTOR_FILES: &str = concat!(
    "SELECT ",
    bae_db::release_file_columns!(),
    ", pin.file IS NOT NULL AS pin_kept, \
        downloading.file AS downloading_file, \
        upload.file AS upload_file, upload.sending AS upload_sending, \
        upload.pending AS upload_pending, upload_record.cause AS upload_cause, \
        original.path AS original_path \
     FROM release_files AS f \
     LEFT JOIN coven_pins AS pin ON pin.file = f.file \
     LEFT JOIN coven_downloading AS downloading ON downloading.file = f.file \
     LEFT JOIN coven_uploads AS upload ON upload.file = f.file \
     LEFT JOIN coven_pending AS upload_record \
        ON upload_record.file = f.file AND upload_record.record = upload.pending \
     LEFT JOIN coven_user_files AS original ON original.file = f.file \
     WHERE f.release_id = ?1 \
     ORDER BY f.original_filename COLLATE natural_order, f.id"
);

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
        let pin = match (
            row.get::<_, bool>("pin_kept")?,
            row.get::<_, Option<coven::FileId>>("downloading_file")?,
        ) {
            (true, _) => Some(crate::album_detail::FilePin::Kept),
            (false, Some(file)) => Some(crate::album_detail::FilePin::Downloading { file }),
            (false, None) => None,
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
        Ok(DbStorageInspectorFile { file: row_to_file(row)?, pin, upload })
    })?)
}
