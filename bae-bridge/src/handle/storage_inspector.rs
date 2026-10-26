//! The Storage Manager inspector's release as the bridge sends it.

use super::*;

impl crate::types::BridgeStorageInspector {
    pub(super) fn from_core(
        inspector: bae_core::library::storage_inspector::StorageInspector,
    ) -> Self {
        let bae_core::library::storage_inspector::StorageInspector { transfers, files } = inspector;
        Self {
            transfers: transfers
                .into_iter()
                .map(crate::types::BridgeStorageInspectorTransfer::from_core)
                .collect(),
            files: files
                .into_iter()
                .map(crate::types::BridgeStorageInspectorFile::from_core)
                .collect(),
        }
    }
}

impl crate::types::BridgeStorageInspectorTransfer {
    fn from_core(transfer: bae_core::library::storage_inspector::StorageInspectorTransfer) -> Self {
        use bae_core::library::storage_inspector::StorageInspectorTransfer;
        match transfer {
            StorageInspectorTransfer::Download { operation, paused } => Self::Download {
                operation: crate::types::BridgeDownloadOp::from_core(operation),
                paused,
            },
            StorageInspectorTransfer::Output { operation, paused } => Self::Output {
                operation: crate::types::BridgeOutputOp::from_core(operation),
                paused,
            },
            StorageInspectorTransfer::Upload { group, paused } => Self::Upload {
                group: crate::types::BridgeUploadReleaseGroup::from_core(group),
                paused,
            },
        }
    }
}

impl crate::types::BridgeStorageInspectorFile {
    fn from_core(file: bae_core::library::storage_inspector::StorageInspectorFile) -> Self {
        use bae_core::library::storage_inspector::StorageInspectorFile;
        match file {
            StorageInspectorFile::ReleaseFile { file, upload } => Self::ReleaseFile {
                file: BridgeFile::from_core(file),
                operation: upload.map(crate::types::BridgeUploadFileOp::from_core),
            },
            StorageInspectorFile::Upload { upload } => Self::Upload {
                operation: crate::types::BridgeUploadFileOp::from_core(upload),
            },
        }
    }
}
