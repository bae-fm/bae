use super::{BridgeDownloadKind, BridgeDownloadOp, BridgeFile, BridgeOutputOp};

/// What the Storage Manager's inspector shows of the release it is set to.
/// Mirrors `StorageInspector`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageInspector {
    /// The release's entry in each transfer queue that holds one: downloads,
    /// then exports, then uploads; while coven fetches the copies its pins
    /// wait for, that fetch after its download.
    pub transfers: Vec<BridgeStorageInspectorTransfer>,
    pub files: Vec<BridgeStorageInspectorFile>,
}

/// One release file in the inspector, beside its upload while one waits,
/// where its download stands while the release pins or moves to this device,
/// and what a move found wrong with its original. Mirrors `StorageInspectorFile`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageInspectorFile {
    pub file: BridgeFile,
    pub upload: Option<BridgeFileUpload>,
    pub download: Option<BridgeFileDownload>,
    pub problem: Option<BridgeFileProblem>,
}

/// What a move found wrong with a file's original. Mirrors `FileProblem`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileProblem {
    /// The original is gone, so the move's upload waits for it.
    OriginalMissing,
    /// The original changed since import, so the move failed.
    OriginalChanged,
}

/// Where a release's file stands while the release pins or moves to this
/// device. Mirrors `FileDownload`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileDownload {
    Waiting {
        kind: BridgeDownloadKind,
    },
    /// How far the download has come is the transfer keyed by `file_id`.
    Downloading {
        kind: BridgeDownloadKind,
        file_id: String,
    },
    Done,
}

/// A release file's queued upload: which file, whether it is being sent, and
/// why it waits when it does. How far it has got is the upload file transfer
/// keyed by its `file_id`. Mirrors `StorageInspectorUpload`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFileUpload {
    pub file_id: String,
    pub state: super::BridgeUploadFileState,
    pub pending: Option<super::BridgePendingWait>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeStorageInspectorTransfer {
    /// `summary` counts the release's files waiting and kept, each left out
    /// at zero, while its download waits or runs.
    Download {
        operation: BridgeDownloadOp,
        summary: Option<Vec<super::BridgeTransferCount>>,
    },
    Output {
        operation: BridgeOutputOp,
    },
    /// The release's files waiting in this device's upload queue, with its
    /// album's title and its files' count and total size, which name the
    /// upload while it waits for a missing original.
    Upload {
        release_id: String,
        title: String,
        file_count: u64,
        total_size: u64,
        upload: super::BridgeReleaseUpload,
    },
    /// The release's move to the cloud, which failed because the original of
    /// the file named `changed` changed since import, with the album's title
    /// and its files' count and total size, and the text of the error behind
    /// the failure when coven kept it.
    MoveFailed {
        release_id: String,
        title: String,
        file_count: u64,
        total_size: u64,
        changed: String,
        detail: Option<String>,
    },
}
