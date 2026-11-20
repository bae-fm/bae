//! The Storage Manager inspector's release as the bridge sends it.

use super::*;

impl crate::types::BridgeStorageInspector {
    pub(crate) fn from_core(inspector: bae_views::storage_inspector::StorageInspector) -> Self {
        let bae_views::storage_inspector::StorageInspector { transfers, files } = inspector;
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
    fn from_core(transfer: bae_views::storage_inspector::StorageInspectorTransfer) -> Self {
        use bae_views::storage_inspector::StorageInspectorTransfer;
        match transfer {
            StorageInspectorTransfer::Download { operation, summary } => Self::Download {
                operation: crate::types::BridgeDownloadOp::from_core(operation),
                summary: summary.map(|summary| {
                    summary.into_iter().map(crate::types::BridgeTransferCount::from_core).collect()
                }),
            },
            StorageInspectorTransfer::Output { operation } => {
                Self::Output { operation: crate::types::BridgeOutputOp::from_core(operation) }
            }
            StorageInspectorTransfer::Upload { release_id, release, upload } => Self::Upload {
                release_id,
                title: release.title,
                file_count: release.file_count,
                total_size: release.total_size,
                upload: crate::types::BridgeReleaseUpload::from_core(upload),
            },
            StorageInspectorTransfer::MoveFailed { release_id, release, file_name, cause } => {
                Self::MoveFailed {
                    release_id,
                    title: release.title,
                    file_count: release.file_count,
                    total_size: release.total_size,
                    changed: file_name,
                    detail: cause,
                }
            }
        }
    }
}

impl crate::types::BridgeStorageInspectorFile {
    fn from_core(row: bae_views::storage_inspector::StorageInspectorFile) -> Self {
        use bae_views::storage_inspector::FileProblem;
        let bae_views::storage_inspector::StorageInspectorFile { file, upload, pin, problem } = row;
        Self {
            file: BridgeFile::from_core(file),
            upload: upload.map(|upload| crate::types::BridgeFileUpload {
                file_id: upload.file.to_string(),
                state: if upload.sending {
                    crate::types::BridgeUploadFileState::Uploading
                } else {
                    crate::types::BridgeUploadFileState::Waiting
                },
                pending: upload.pending.map(crate::types::BridgePendingWait::from_core),
            }),
            pin: pin.map(|pin| match pin {
                bae_views::album_detail::FilePin::Kept => crate::types::BridgeFilePin::Kept,
                bae_views::album_detail::FilePin::Downloading { file } => {
                    crate::types::BridgeFilePin::Downloading { file_id: file.to_string() }
                }
                bae_views::album_detail::FilePin::Waiting => crate::types::BridgeFilePin::Waiting,
            }),
            problem: problem.map(|problem| match problem {
                FileProblem::OriginalMissing => crate::types::BridgeFileProblem::OriginalMissing,
                FileProblem::OriginalChanged => crate::types::BridgeFileProblem::OriginalChanged,
            }),
        }
    }
}
