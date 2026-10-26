use super::{
    BridgeDownloadOp, BridgeFile, BridgeOutputOp, BridgeUploadFileOp, BridgeUploadReleaseGroup,
};

/// What the Storage Manager's inspector shows of the release it is set to.
/// Mirrors `StorageInspector`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageInspector {
    /// The release's entry in each transfer queue that holds one: downloads,
    /// then exports, then uploads.
    pub transfers: Vec<BridgeStorageInspectorTransfer>,
    pub files: Vec<BridgeStorageInspectorFile>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeStorageInspectorFile {
    ReleaseFile {
        file: BridgeFile,
        /// The file's upload while one is queued.
        operation: Option<BridgeUploadFileOp>,
    },
    Upload {
        operation: BridgeUploadFileOp,
    },
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
    Upload {
        group: BridgeUploadReleaseGroup,
        paused: bool,
    },
}
