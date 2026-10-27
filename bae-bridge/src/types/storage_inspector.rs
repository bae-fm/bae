use super::{BridgeDownloadOp, BridgeFile, BridgeOutputOp};

/// What the Storage Manager's inspector shows of the release it is set to.
/// Mirrors `StorageInspector`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageInspector {
    /// The release's entry in each transfer queue that holds one: downloads,
    /// then exports, then uploads.
    pub transfers: Vec<BridgeStorageInspectorTransfer>,
    pub files: Vec<BridgeStorageInspectorFile>,
}

/// One release file in the inspector, beside its upload while one waits.
/// Mirrors `StorageInspectorFile`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageInspectorFile {
    pub file: BridgeFile,
    pub upload: Option<BridgeFileUpload>,
}

/// A release file's queued upload: which file, whether it is being sent,
/// why it waits when it does, and its session's confirmed bytes. How fast it
/// goes is the transfer keyed by its `file_id`. Mirrors
/// `StorageInspectorUpload`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFileUpload {
    pub file_id: String,
    pub state: super::BridgeUploadFileState,
    pub pending: Option<super::BridgePendingWait>,
    pub bar: Option<super::BridgeUploadBar>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeStorageInspectorTransfer {
    Download {
        operation: BridgeDownloadOp,
        paused: bool,
    },
    Output {
        operation: BridgeOutputOp,
        paused: bool,
    },
    /// The release's files waiting in this device's upload queue.
    Upload {
        release_id: String,
        upload: super::BridgeReleaseUpload,
        paused: bool,
    },
}
