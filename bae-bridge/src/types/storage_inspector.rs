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

/// How many of a pinning release's files are kept and how many wait.
/// Mirrors `PinFiles`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgePinFiles {
    pub kept: u32,
    pub waiting: u32,
}

/// A release file's queued upload: which file, whether it is being sent,
/// why it waits when it does, and while it is sent, the bytes of its size
/// sent. How fast it goes is the transfer keyed by its `file_id`. Mirrors
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
    /// `files` counts the release's files while its download waits or runs.
    Download {
        operation: BridgeDownloadOp,
        files: Option<BridgePinFiles>,
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
