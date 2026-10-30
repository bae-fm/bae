//! The Storage Manager inspector's release: its files, each beside its
//! upload while one waits, and the release's entry in each transfer queue.

use super::release_queue::ReleaseQueueState;
use super::{DownloadRow, OutputPhaseRow, PendingWait, ReleaseQueueEntries, UploadState};
use crate::album_detail::{FileDetail, FilePin, ReleaseUpload};

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
/// error behind that while this process holds it. How far it has got is its
/// file's transfer, keyed by `file`.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspectorUpload {
    pub file: coven::FileId,
    pub phase: UploadState,
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
        summary: Option<Vec<crate::library::TransferCount>>,
    },
    /// Coven downloading the release's files a move to the cloud asked this
    /// device to keep here, outside the download queue: `summary` counts the
    /// files downloading and kept, each left out at zero.
    PinFetch {
        summary: Vec<crate::library::TransferCount>,
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
    /// The release `release_id` from one read of it, `rows`, each upload's
    /// record with the error `causes` holds for it.
    pub(crate) fn read(
        release_id: &str,
        rows: crate::db::DbStorageInspector,
        causes: &coven::PendingCauses,
    ) -> Self {
        Self::of(
            release_id,
            rows.files,
            &rows.entries,
            rows.upload.map(ReleaseUpload::from_raw),
            causes,
        )
    }

    /// The release `release_id`: its `files` in their order, each upload's
    /// record with the error `causes` holds for it; its entry in each queue
    /// (`entries`, and its files waiting to upload, `upload`); while its
    /// download waits or runs, each file's pin and the download's count of
    /// them; and otherwise, while coven downloads a file it keeps here, each
    /// file's pin and coven's fetch counting them.
    pub(crate) fn of(
        release_id: &str,
        files: Vec<crate::db::DbStorageInspectorFile>,
        entries: &ReleaseQueueEntries,
        upload: Option<ReleaseUpload>,
        causes: &coven::PendingCauses,
    ) -> Self {
        let pinning = if entries
            .download
            .as_ref()
            .is_some_and(|row| !matches!(row.op.state, ReleaseQueueState::Failed { .. }))
        {
            Some(Pinning::Download)
        } else if files
            .iter()
            .any(|file| file.pin.is_some_and(|pin| pin != FilePin::Kept))
        {
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
                        UploadState::Uploading
                    } else {
                        UploadState::Waiting
                    },
                    pending: upload.pending.map(|mut record| {
                        record.cause = causes.cause(&record);
                        PendingWait::of(&record, Some(release_id))
                    }),
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
            u32::try_from(
                files
                    .iter()
                    .filter(|file| file.pin.as_ref().is_some_and(stands))
                    .count(),
            )
            .expect("a release's file count")
        };
        let pins = pinning.map(|pinning| {
            use crate::library::TransferStage;
            (
                pinning,
                crate::library::release_queue::transfer_summary([
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
        Self {
            transfers: storage_inspector_transfers(release_id, entries, upload, pins),
            files,
        }
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
    pins: Option<(Pinning, Vec<crate::library::TransferCount>)>,
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
        items.push(StorageInspectorTransfer::Output {
            operation: operation.clone(),
        });
    }
    if let Some(upload) = upload {
        items.push(StorageInspectorTransfer::Upload {
            release_id: release_id.to_string(),
            upload,
        });
    }
    items
}

#[cfg(test)]
#[path = "storage_inspector_tests.rs"]
mod tests;
