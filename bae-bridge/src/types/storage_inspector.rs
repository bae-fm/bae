use super::{
    BridgeDownloadOp, BridgeDownloadSnapshot, BridgeFile, BridgeOutboxPauseState,
    BridgeOutboxSnapshot, BridgeOutputOp, BridgeOutputSnapshot, BridgeUploadFileOp,
    BridgeUploadReleaseGroup,
};

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeStorageInspectorFile {
    ReleaseFile {
        file: BridgeFile,
        upload: Option<BridgeUploadFileOp>,
    },
    Upload {
        upload: BridgeUploadFileOp,
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

#[uniffi::export]
pub fn bridge_storage_inspector_transfers(
    release_id: String,
    downloads: BridgeDownloadSnapshot,
    outputs: BridgeOutputSnapshot,
    outbox: BridgeOutboxSnapshot,
) -> Vec<BridgeStorageInspectorTransfer> {
    use bae_core::library::storage_inspector::{
        storage_inspector_transfers, StorageInspectorTransfer,
    };
    storage_inspector_transfers(
        &release_id,
        (
            downloads
                .downloads
                .into_iter()
                .map(|op| (op.release_id.clone(), op))
                .collect(),
            downloads.paused,
        ),
        (
            outputs
                .outputs
                .into_iter()
                .map(|op| (op.release_id.clone(), op))
                .collect(),
            outputs.paused,
        ),
        (
            outbox
                .upload_groups
                .into_iter()
                .map(|group| (group.release_id.clone(), group))
                .collect(),
            outbox.pause_state == BridgeOutboxPauseState::Paused,
        ),
    )
    .into_iter()
    .map(|item| match item {
        StorageInspectorTransfer::Download { operation, paused } => {
            BridgeStorageInspectorTransfer::Download { operation, paused }
        }
        StorageInspectorTransfer::Output { operation, paused } => {
            BridgeStorageInspectorTransfer::Output { operation, paused }
        }
        StorageInspectorTransfer::Upload { group, paused } => {
            BridgeStorageInspectorTransfer::Upload { group, paused }
        }
    })
    .collect()
}

#[uniffi::export]
pub fn bridge_storage_inspector_files(
    release_id: String,
    files: Vec<BridgeFile>,
    outbox: BridgeOutboxSnapshot,
) -> Vec<BridgeStorageInspectorFile> {
    use bae_core::library::storage_inspector::{
        storage_inspector_release_files, StorageInspectorFile,
    };
    storage_inspector_release_files(
        &release_id,
        files
            .into_iter()
            .map(|file| (file.id.clone(), file))
            .collect(),
        outbox
            .upload_groups
            .into_iter()
            .map(|group| {
                (
                    group.release_id,
                    group
                        .files
                        .into_iter()
                        .map(|upload| (upload.file_id.clone(), upload))
                        .collect(),
                )
            })
            .collect(),
    )
    .into_iter()
    .map(|row| match row {
        StorageInspectorFile::ReleaseFile { file, upload } => {
            BridgeStorageInspectorFile::ReleaseFile { file, upload }
        }
        StorageInspectorFile::Upload { upload } => BridgeStorageInspectorFile::Upload { upload },
    })
    .collect()
}
