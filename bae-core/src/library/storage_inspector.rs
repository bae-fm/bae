//! The Storage Manager inspector's release: its files, each beside the upload
//! coven holds for it, and the release's entry in each transfer queue.

use super::{DownloadRow, DownloadSnapshot, OutputPhaseRow, OutputPhaseSnapshot};
use super::{UploadFileOp, UploadReleaseGroup};
use crate::album_detail::FileDetail;

/// What the inspector shows of one release, read in core for the release id
/// it is set to.
#[derive(Debug, Clone)]
pub struct StorageInspector {
    /// The release's entry in each transfer queue that holds one: downloads,
    /// then exports, then uploads.
    pub transfers: Vec<StorageInspectorTransfer>,
    pub files: Vec<StorageInspectorFile>,
}

/// One row of the inspector's file list.
#[derive(Debug, Clone)]
pub enum StorageInspectorFile {
    /// A release file and its upload, if one is queued: coven queues at most
    /// one pending attachment per row.
    ReleaseFile {
        file: FileDetail,
        upload: Option<UploadFileOp>,
    },
    /// An upload of the release that names none of its file rows, such as
    /// its cover.
    Upload { upload: UploadFileOp },
}

/// The release's entry in one transfer queue, beside whether that queue is
/// paused: each queue has its own pause state.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageInspectorTransfer {
    Download {
        operation: DownloadRow,
        paused: bool,
    },
    Output {
        operation: OutputPhaseRow,
        paused: bool,
    },
    Upload {
        group: UploadReleaseGroup,
        paused: bool,
    },
}

impl StorageInspector {
    /// The release's `files` in their order, each beside its queued upload,
    /// then the uploads that name none of them, read from `transfers`.
    pub(crate) fn of(files: Vec<FileDetail>, transfers: Vec<StorageInspectorTransfer>) -> Self {
        let uploads = transfers
            .iter()
            .find_map(|transfer| match transfer {
                StorageInspectorTransfer::Upload { group, .. } => Some(
                    group
                        .files
                        .iter()
                        .map(|upload| (upload.release_file_key().cloned(), upload.clone()))
                        .collect(),
                ),
                StorageInspectorTransfer::Download { .. }
                | StorageInspectorTransfer::Output { .. } => None,
            })
            .unwrap_or_default();
        let files = join_files(
            files
                .into_iter()
                .map(|file| (file.id.clone(), file))
                .collect(),
            uploads,
        )
        .into_iter()
        .map(|row| match row {
            Joined::ReleaseFile { file, upload } => {
                StorageInspectorFile::ReleaseFile { file, upload }
            }
            Joined::Upload { upload } => StorageInspectorFile::Upload { upload },
        })
        .collect();
        Self { transfers, files }
    }
}

/// The release's entry in each queue, downloads first, then exports, then
/// its upload group (`upload`, from a queue paused when `uploads_paused`).
pub(crate) fn storage_inspector_transfers(
    release_id: &str,
    downloads: &DownloadSnapshot,
    outputs: &OutputPhaseSnapshot,
    upload: Option<UploadReleaseGroup>,
    uploads_paused: bool,
) -> Vec<StorageInspectorTransfer> {
    let mut items = Vec::new();
    if let Some(operation) = selected_release(release_id, &downloads.rows, |row| &row.op.release_id)
    {
        items.push(StorageInspectorTransfer::Download {
            operation: operation.clone(),
            paused: downloads.paused,
        });
    }
    if let Some(operation) = selected_release(release_id, &outputs.rows, |row| &row.op.release_id) {
        items.push(StorageInspectorTransfer::Output {
            operation: operation.clone(),
            paused: outputs.paused,
        });
    }
    if let Some(group) = upload {
        assert_eq!(
            group.release_id, release_id,
            "the shown upload group is the selected release's"
        );
        items.push(StorageInspectorTransfer::Upload {
            group,
            paused: uploads_paused,
        });
    }
    items
}

/// The release's upload group among `groups`, which hold one per release.
pub(crate) fn selected_upload_group(
    release_id: &str,
    groups: &[UploadReleaseGroup],
) -> Option<UploadReleaseGroup> {
    selected_release(release_id, groups, |group| &group.release_id).cloned()
}

fn selected_release<'a, T>(
    release_id: &str,
    records: &'a [T],
    id: impl Fn(&T) -> &String,
) -> Option<&'a T> {
    let mut selected = records.iter().filter(|record| id(record) == release_id);
    let result = selected.next();
    assert!(
        selected.next().is_none(),
        "duplicate release in transfer queue"
    );
    result
}

/// A release file joined with its upload, or an upload that joins none.
#[derive(Debug, PartialEq)]
enum Joined<File, Upload> {
    ReleaseFile { file: File, upload: Option<Upload> },
    Upload { upload: Upload },
}

/// Release files retain their order and identity when transfers start or end.
/// Uploads matching none of the release's file rows remain visible after the
/// release files.
fn join_files<File, Upload>(
    files: Vec<(String, File)>,
    mut uploads: Vec<(Option<coven::RowKey>, Upload)>,
) -> Vec<Joined<File, Upload>> {
    let mut rows = Vec::with_capacity(files.len());
    for (id, file) in files {
        let row = coven::RowKey::from(id.as_str());
        let upload = uploads
            .iter()
            .position(|(key, _)| key.as_ref() == Some(&row))
            .map(|index| uploads.remove(index).1);
        rows.push(Joined::ReleaseFile { file, upload });
    }
    for (_, upload) in uploads {
        rows.push(Joined::Upload { upload });
    }
    rows
}

#[cfg(test)]
#[path = "storage_inspector_tests.rs"]
mod tests;
