//! The previews an import writes with its release: a small copy of each
//! image file, made from the copies its candidate keeps, or read from the
//! folder when none exists.

use std::collections::HashMap;
use std::path::PathBuf;

use bae_pictures::Priority;
use bae_records::NewReleasePreview;

use super::ImportService;
use crate::folder_scanner::{CandidateFile, FileRole};
use crate::{ImportError, ImportPicture};

impl ImportService {
    /// A preview of each of the candidate's image files, in the files' order.
    /// `release_file_ids` names each file's row by the file's path. The
    /// previews are read together, as many at a time as the volume's gate
    /// admits, and the first one that fails ends the rest.
    #[expect(
        clippy::disallowed_methods,
        reason = "the set joins every read and reports the panic of one as an import failure"
    )]
    pub(super) async fn release_previews(
        &self,
        discovered_files: &[CandidateFile],
        release_file_ids: &HashMap<PathBuf, String>,
    ) -> Result<Vec<NewReleasePreview>, ImportError> {
        let mut reads = tokio::task::JoinSet::new();
        let images =
            discovered_files.iter().filter(|entry| matches!(entry.role, FileRole::Artwork));
        for (position, entry) in images.enumerate() {
            let release_file_id = release_file_ids
                .get(&entry.file.version.path)
                .cloned()
                .ok_or_else(|| ImportError::Internal {
                    detail: format!("{} has no release file", entry.file.relative_path),
                })?;
            let picture = ImportPicture::Folder { file: entry.file.version.clone() };
            let pictures = self.pictures.clone();
            reads.spawn(async move {
                let copy =
                    pictures.read_copy(picture, NewReleasePreview::STEP, Priority::Visible).await;
                (position, release_file_id, copy)
            });
        }
        let mut made = Vec::new();
        while let Some(joined) = reads.join_next().await {
            let (position, release_file_id, copy) =
                joined.map_err(|error| ImportError::Internal {
                    detail: format!("reading a release image's preview ended: {error}"),
                })?;
            let copy = copy?;
            made.push((
                position,
                NewReleasePreview {
                    release_file_id,
                    content_type: crate::content_type_of(copy.format),
                    bytes: copy.bytes,
                },
            ));
        }
        made.sort_by_key(|(position, _)| *position);
        Ok(made.into_iter().map(|(_, preview)| preview).collect())
    }
}
