//! The release's files with their queued uploads joined by release-file row key.

/// Preserve the original file and transfer records across the projection.
/// The bridge instantiates these with its wire records, without duplicating
/// the joining logic or round-tripping their display fields through core.
#[derive(Debug, PartialEq)]
pub enum StorageInspectorFile<File, Upload> {
    ReleaseFile { file: File, uploads: Vec<Upload> },
    Upload { upload: Upload },
}

/// Each transfer queue has its own pause state and one operation per release.
pub enum StorageInspectorTransfer<Download, Output, Upload> {
    Download { operation: Download, paused: bool },
    Output { operation: Output, paused: bool },
    Upload { group: Upload, paused: bool },
}

/// Queued uploads and their release-file row keys; artwork has no release-file key.
pub type InspectorUploads<Upload> = Vec<(Option<coven::RowKey>, Upload)>;

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
    upload_groups: Vec<(String, InspectorUploads<Upload>)>,
) -> Vec<StorageInspectorFile<File, Upload>> {
    let uploads = selected_release(release_id, upload_groups).unwrap_or_default();
    storage_inspector_files(files, uploads)
}

/// Release files retain their order and identity when transfers start or end.
/// Uploads outside the imported file set (including generated artwork and
/// uploads whose rows were deleted) remain visible after the release files.
fn storage_inspector_files<File, Upload>(
    files: Vec<(String, File)>,
    mut uploads: InspectorUploads<Upload>,
) -> Vec<StorageInspectorFile<File, Upload>> {
    let mut rows = Vec::with_capacity(files.len());
    for (id, file) in files {
        let row = coven::RowKey::from(id.as_str());
        // Replacing an attachment may leave several committed uploads for one
        // row. Keep every operation, in queue order, beside that row.
        let (matching, remaining): (Vec<_>, Vec<_>) = uploads
            .into_iter()
            .partition(|(key, _)| key.as_ref() == Some(&row));
        uploads = remaining;
        rows.push(StorageInspectorFile::ReleaseFile {
            file,
            uploads: matching.into_iter().map(|(_, upload)| upload).collect(),
        });
    }
    for (_, upload) in uploads {
        rows.push(StorageInspectorFile::Upload { upload });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_release_rows_and_keeps_images_separate() {
        let rows = storage_inspector_files(
            vec![
                ("audio".into(), "track.flac"),
                ("notes".into(), "notes.txt"),
            ],
            vec![(None, "cover"), (Some("audio".into()), "progress")],
        );
        assert_eq!(
            rows,
            vec![
                StorageInspectorFile::ReleaseFile {
                    file: "track.flac",
                    uploads: vec!["progress"]
                },
                StorageInspectorFile::ReleaseFile {
                    file: "notes.txt",
                    uploads: vec![]
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
                    uploads: vec![]
                },
                StorageInspectorFile::ReleaseFile {
                    file: "a.flac",
                    uploads: vec![]
                },
            ]
        );
    }

    #[test]
    fn uploads_survive_an_empty_file_set_in_queue_order() {
        let rows = storage_inspector_files::<(), _>(
            vec![],
            vec![(None, "cover"), (Some("a".into()), "queued")],
        );
        assert_eq!(
            rows,
            vec![
                StorageInspectorFile::Upload { upload: "cover" },
                StorageInspectorFile::Upload { upload: "queued" },
            ]
        );
    }
}
