//! The Storage Manager inspector's release: its files, each beside its
//! upload while one waits, and the release's entry in each transfer queue.

use super::{
    DownloadRow, OutputPhaseRow, PendingWait, ReleaseQueueEntries, UploadBar, UploadState,
};
use crate::album_detail::{FileDetail, ReleaseUpload};

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
/// if one is queued: coven queues at most one pending attachment per row.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspectorFile {
    pub file: FileDetail,
    pub upload: Option<StorageInspectorUpload>,
}

/// A release file's queued upload as the inspector shows it: which file,
/// whether it is being sent now, why it waits when it does, with the error
/// behind that while this process holds it, and while it is sent, the bytes
/// of its size sent. How fast it goes is its transfer.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspectorUpload {
    pub file: coven::FileId,
    pub phase: UploadState,
    pub pending: Option<PendingWait>,
    pub bar: Option<UploadBar>,
}

/// The release's entry in one transfer queue. Pausing a whole queue is the
/// Storage Manager's footer's, not the release's.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageInspectorTransfer {
    Download {
        operation: DownloadRow,
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
    /// The release's `files` in their order, each upload's record with the
    /// error `causes` holds for it.
    pub(crate) fn of(
        files: Vec<FileDetail>,
        transfers: Vec<StorageInspectorTransfer>,
        causes: &coven::PendingCauses,
    ) -> Self {
        let files = files
            .into_iter()
            .map(|file| StorageInspectorFile {
                upload: file.upload.clone().map(|upload| StorageInspectorUpload {
                    file: upload.file,
                    phase: upload.phase,
                    pending: upload.pending.map(|mut record| {
                        record.cause = causes.cause(&record);
                        PendingWait::of(&record)
                    }),
                    bar: upload.bar,
                }),
                file,
            })
            .collect();
        Self { transfers, files }
    }
}

/// The release's entry in each queue, downloads first, then exports, then
/// its files waiting to upload (`upload`).
pub(crate) fn storage_inspector_transfers(
    release_id: &str,
    entries: &ReleaseQueueEntries,
    upload: Option<ReleaseUpload>,
) -> Vec<StorageInspectorTransfer> {
    let mut items = Vec::new();
    if let Some(operation) = &entries.download {
        items.push(StorageInspectorTransfer::Download {
            operation: operation.clone(),
        });
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
