//! The Storage Manager inspector's release: its files, each beside its
//! upload while one waits, and the release's entry in each transfer queue.

use super::release_queue::ReleaseQueueState;
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
/// if one is queued: coven queues at most one pending attachment per row;
/// and while the release is pinning, where the file's pin stands.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageInspectorFile {
    pub file: FileDetail,
    pub upload: Option<StorageInspectorUpload>,
    pub pin: Option<FilePin>,
}

/// Where one file of a release pinning stands: kept whole on this device,
/// or not yet. Which of the files not kept coven is downloading now, and how
/// far each has got, coven doesn't say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePin {
    Kept,
    Waiting,
}

/// How many of a release pinning's files are kept and how many wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinFiles {
    pub kept: u32,
    pub waiting: u32,
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
    /// `files` counts the release's files while its download waits or runs.
    Download {
        operation: DownloadRow,
        files: Option<PinFiles>,
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
    /// The release `release_id`: its `files` in their order, each upload's
    /// record with the error `causes` holds for it; its entry in each queue
    /// (`entries`, and its files waiting to upload, `upload`); and while its
    /// download waits or runs, each file's pin and the download's count of
    /// them.
    pub(crate) fn of(
        release_id: &str,
        files: Vec<FileDetail>,
        entries: &ReleaseQueueEntries,
        upload: Option<ReleaseUpload>,
        causes: &coven::PendingCauses,
    ) -> Self {
        let pinning = entries
            .download
            .as_ref()
            .is_some_and(|row| !matches!(row.op.state, ReleaseQueueState::Failed { .. }));
        let files: Vec<StorageInspectorFile> = files
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
                pin: pinning.then_some(if file.kept {
                    FilePin::Kept
                } else {
                    FilePin::Waiting
                }),
                file,
            })
            .collect();
        let count = |pin: FilePin| {
            u32::try_from(files.iter().filter(|file| file.pin == Some(pin)).count())
                .expect("a release's file count")
        };
        let pins = pinning.then(|| PinFiles {
            kept: count(FilePin::Kept),
            waiting: count(FilePin::Waiting),
        });
        Self {
            transfers: storage_inspector_transfers(release_id, entries, upload, pins),
            files,
        }
    }
}

/// The release's entry in each queue, downloads first, with `pins` counting
/// its files while the download waits or runs, then exports, then its files
/// waiting to upload (`upload`).
fn storage_inspector_transfers(
    release_id: &str,
    entries: &ReleaseQueueEntries,
    upload: Option<ReleaseUpload>,
    pins: Option<PinFiles>,
) -> Vec<StorageInspectorTransfer> {
    let mut items = Vec::new();
    if let Some(operation) = &entries.download {
        items.push(StorageInspectorTransfer::Download {
            operation: operation.clone(),
            files: pins,
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
