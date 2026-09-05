//! The release's files with their current upload attached by blob identity.

use std::collections::HashMap;

use super::outbox_snapshot::UploadBlobKey;

/// Preserve the original file and transfer records across the projection.
/// The bridge instantiates these with its wire records, without duplicating
/// the joining logic or round-tripping their display fields through core.
#[derive(Debug, PartialEq)]
pub enum StorageInspectorFile<File, Upload> {
    ReleaseFile { file: File, upload: Option<Upload> },
    Upload { upload: Upload },
}

/// Each transfer queue has its own pause state and one operation per release.
pub enum StorageInspectorTransfer<Download, Output, Upload> {
    Download { operation: Download, paused: bool },
    Output { operation: Output, paused: bool },
    Upload { group: Upload, paused: bool },
}

fn selected_release<T>(release_id: &str, records: Vec<(String, T)>) -> Option<T> {
    let mut selected = records.into_iter().filter(|(id, _)| id == release_id);
    let result = selected.next().map(|(_, record)| record);
    assert!(
        selected.next().is_none(),
        "duplicate release in transfer queue"
    );
    result
}

pub fn storage_inspector_transfers<Download, Output, Upload>(
    release_id: &str,
    downloads: (Vec<(String, Download)>, bool),
    outputs: (Vec<(String, Output)>, bool),
    uploads: (Vec<(String, Upload)>, bool),
) -> Vec<StorageInspectorTransfer<Download, Output, Upload>> {
    let mut items = Vec::new();
    if let Some(operation) = selected_release(release_id, downloads.0) {
        items.push(StorageInspectorTransfer::Download {
            operation,
            paused: downloads.1,
        });
    }
    if let Some(operation) = selected_release(release_id, outputs.0) {
        items.push(StorageInspectorTransfer::Output {
            operation,
            paused: outputs.1,
        });
    }
    if let Some(group) = selected_release(release_id, uploads.0) {
        items.push(StorageInspectorTransfer::Upload {
            group,
            paused: uploads.1,
        });
    }
    items
}

pub fn storage_inspector_release_files<File, Upload>(
    release_id: &str,
    files: Vec<(String, File)>,
    upload_groups: Vec<(String, Vec<(String, Upload)>)>,
) -> Vec<StorageInspectorFile<File, Upload>> {
    let uploads = selected_release(release_id, upload_groups).unwrap_or_default();
    storage_inspector_files(files, uploads)
}

/// Release files retain their order and identity when transfers start or end.
/// Uploads outside the imported file set (including generated artwork and
/// objects being removed) remain visible after the release files.
fn storage_inspector_files<File, Upload>(
    files: Vec<(String, File)>,
    uploads: Vec<(String, Upload)>,
) -> Vec<StorageInspectorFile<File, Upload>> {
    let mut upload_order = Vec::with_capacity(uploads.len());
    let mut by_id = HashMap::with_capacity(uploads.len());
    for (id, upload) in uploads {
        upload_order.push(id.clone());
        assert!(
            by_id.insert(id, upload).is_none(),
            "duplicate upload identity"
        );
    }
    let mut rows = Vec::with_capacity(files.len());
    for (id, file) in files {
        // release_files is write-once and its blob id is its primary key.
        let upload_id = UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, id).stable_id();
        rows.push(StorageInspectorFile::ReleaseFile {
            file,
            upload: by_id.remove(&upload_id),
        });
    }
    for id in upload_order {
        if let Some(upload) = by_id.remove(&id) {
            rows.push(StorageInspectorFile::Upload { upload });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_by_namespaced_identity_and_preserves_untransferred_files() {
        let rows = storage_inspector_files(
            vec![
                ("audio".into(), "track.flac"),
                ("notes".into(), "notes.txt"),
            ],
            vec![
                ("covers:audio".into(), "cover"),
                ("release_files:audio".into(), "progress"),
            ],
        );
        assert_eq!(
            rows,
            vec![
                StorageInspectorFile::ReleaseFile {
                    file: "track.flac",
                    upload: Some("progress")
                },
                StorageInspectorFile::ReleaseFile {
                    file: "notes.txt",
                    upload: None
                },
                StorageInspectorFile::Upload { upload: "cover" },
            ]
        );
    }

    #[test]
    fn finishing_uploads_keeps_the_contents_in_order() {
        let rows = storage_inspector_files::<_, ()>(
            vec![("b".into(), "b.flac"), ("a".into(), "a.flac")],
            vec![],
        );
        assert_eq!(
            rows,
            vec![
                StorageInspectorFile::ReleaseFile {
                    file: "b.flac",
                    upload: None
                },
                StorageInspectorFile::ReleaseFile {
                    file: "a.flac",
                    upload: None
                },
            ]
        );
    }

    #[test]
    fn uploads_survive_an_empty_file_set_in_queue_order() {
        let rows = storage_inspector_files::<(), _>(
            vec![],
            vec![
                ("covers:c".into(), "cover"),
                ("release_files:a".into(), "unwinding"),
            ],
        );
        assert_eq!(
            rows,
            vec![
                StorageInspectorFile::Upload { upload: "cover" },
                StorageInspectorFile::Upload {
                    upload: "unwinding"
                },
            ]
        );
    }
}
