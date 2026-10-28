use super::{BridgeDownloadOp, BridgeFile, BridgeOutputOp};

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
/// and while the release is pinning, where its pin stands. Mirrors
/// `StorageInspectorFile`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageInspectorFile {
    pub file: BridgeFile,
    pub upload: Option<BridgeFileUpload>,
    pub pin: Option<BridgeFilePin>,
}

/// Whether a pinning release's file is kept whole on this device yet.
/// Mirrors `FilePin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFilePin {
    Kept,
    Waiting,
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
    /// Coven fetching the copies the release's pins wait for, outside the
    /// download queue: `summary` counts the pinned files waiting and kept,
    /// each left out at zero.
    PinFetch {
        summary: Vec<super::BridgeTransferCount>,
    },
    Output {
        operation: BridgeOutputOp,
    },
    /// The release's files waiting in this device's upload queue.
    Upload {
        release_id: String,
        upload: super::BridgeReleaseUpload,
    },
}
