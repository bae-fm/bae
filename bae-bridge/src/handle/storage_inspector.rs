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
            StorageInspectorTransfer::Download { operation } => Self::Download {
                operation: crate::types::BridgeDownloadOp::from_core(operation),
            },
            StorageInspectorTransfer::Output { operation } => Self::Output {
                operation: crate::types::BridgeOutputOp::from_core(operation),
            },
            StorageInspectorTransfer::Upload { release_id, upload } => Self::Upload {
                release_id,
                upload: crate::types::BridgeReleaseUpload::from_core(upload),
            },
        }
    }
}

impl crate::types::BridgeStorageInspectorFile {
    fn from_core(row: bae_core::library::storage_inspector::StorageInspectorFile) -> Self {
        let bae_core::library::storage_inspector::StorageInspectorFile { file, upload } = row;
        Self {
            file: BridgeFile::from_core(file),
            upload: upload.map(|upload| crate::types::BridgeFileUpload {
                file_id: upload.file.to_string(),
                state: match upload.phase {
                    coven::UploadPhase::Waiting => crate::types::BridgeUploadFileState::Waiting,
                    coven::UploadPhase::Uploading => crate::types::BridgeUploadFileState::Uploading,
                },
                pending: upload
                    .pending
                    .map(crate::types::BridgePendingWait::from_core),
                bar: upload.bar.map(crate::types::BridgeUploadBar::from_core),
            }),
        }
    }
}
