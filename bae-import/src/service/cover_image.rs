//! The cover an import stores: the image its candidate's draft holds, read
//! as its source holds it and resized to the JPEG bae stores.

use crate::cover_art::PreparedCover;
use crate::file_tag_snapshot::FileTagSnapshot;
use crate::folder_scanner::CandidateFile;
use crate::ImportError;
use bae_catalogs::remote_image::RemoteImage;

/// Where the bytes of an import's cover come from. Nothing is chosen at
/// import: this is the candidate's stored selection, the image a person
/// chose, the one its identification fetched, or the folder's own cover the
/// scan stored for it.
pub(super) enum ImportCover {
    /// A catalog's image, its exact bytes as the draft's cover holds them.
    /// The import never fetches it.
    Remote(Vec<u8>),
    /// An image file in the folder, by its path under the candidate's folder,
    /// read when the release is written.
    Local(String),
    /// The artwork embedded in the candidate's audio, as the file-tag
    /// snapshot that seeded the pane captured it, so the import never opens
    /// the tags a second time.
    Embedded(Vec<u8>),
}

impl ImportCover {
    /// The cover the draft holds, from the remote bytes it carries or the
    /// file-tag snapshot the import was queued with. A stored embedded
    /// cover names the source audio exactly; no other snapshot image stands
    /// in for it.
    pub(super) fn selected(
        cover: Option<PreparedCover>,
        snapshot: Option<&FileTagSnapshot>,
        candidate_key: &crate::CandidateKey,
    ) -> Result<Option<Self>, ImportError> {
        let internal = |detail: String| ImportError::Internal { detail };
        match cover {
            Some(PreparedCover::Remote { fetched, .. }) => {
                Ok(Some(Self::Remote(downloaded_cover(fetched)?)))
            }
            Some(PreparedCover::Local(path)) => Ok(Some(Self::Local(path))),
            Some(PreparedCover::Embedded(source_file_id)) => {
                let Some(snapshot) = snapshot else {
                    return Err(internal(format!(
                        "{candidate_key}'s embedded cover has no prepared tag snapshot"
                    )));
                };
                match &snapshot.embedded_cover {
                    Some(cover) if cover.source_relative_path == source_file_id => {
                        Ok(Some(Self::Embedded(cover.data.clone())))
                    }
                    Some(cover) => Err(internal(format!(
                        "Selected embedded cover {source_file_id} does not match snapshot source {}",
                        cover.source_relative_path
                    ))),
                    None => Err(internal(format!(
                        "Selected embedded cover {source_file_id} is absent from the file-tag snapshot"
                    ))),
                }
            }
            None => Ok(None),
        }
    }

    /// The cover as bae stores it: its source's bytes resized to a ≤600px
    /// JPEG, one funnel for all three sources, and the row built from that
    /// output, so its hash, size and content type describe the stored file
    /// rather than its source.
    pub(super) async fn stored(
        self,
        discovered_files: &[CandidateFile],
        release_id: &str,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Result<(bae_db::DbLibraryImage, Vec<u8>), ImportError> {
        let source = match self {
            Self::Remote(bytes) | Self::Embedded(bytes) => bytes,
            Self::Local(path) => folder_cover(discovered_files, &path, diagnostics).await?,
        };
        // Decoding and re-encoding the image is CPU work.
        let bytes = bae_util::task_failure::blocking("resize the import's cover", move || {
            bae_util::stored_image::stored_jpeg(&source)
        })
        .await
        .map_err(|failed| diagnostics.bug(failed))?
        .map_err(|detail| ImportError::CoverArt { detail })?;
        Ok((bae_db::DbLibraryImage::cover(release_id), bytes))
    }
}

/// One downloaded cover's bytes. Its content type is checked and dropped: the
/// resize re-encodes the bytes, so it does not describe what is stored.
fn downloaded_cover(image: RemoteImage) -> Result<Vec<u8>, ImportError> {
    let RemoteImage { bytes, content_type } = image;
    if matches!(content_type, bae_util::content_type::ContentType::OctetStream) {
        return Err(ImportError::CoverArt {
            detail: "Cover bytes aren't a recognized image format (PNG/JPEG/GIF/WebP/BMP)"
                .to_string(),
        });
    }
    Ok(bytes)
}

/// Read the selected cover file's bytes as the folder holds them. Nothing is
/// written here, and no row is built: the caller resizes them and records the
/// result, so nothing describing these bytes can be mistaken for a
/// description of the stored file.
///
/// The path is the candidate's stored selection, so a folder that no longer
/// holds that image is a candidate whose selection no longer describes it,
/// and that is stated rather than replaced by some other image. Only the
/// artwork the folder offers as a cover can be it: a damaged image of the
/// same name is no cover.
///
/// The read runs on a blocking thread, so a cover on a slow volume does not
/// stall the import worker's runtime.
async fn folder_cover(
    discovered_files: &[CandidateFile],
    selected_cover_path: &str,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Vec<u8>, ImportError> {
    let cover_file = discovered_files
        .iter()
        .find(|entry| {
            entry.file.relative_path == selected_cover_path && entry.cover_choice().is_some()
        })
        .map(|entry| &entry.file)
        .ok_or_else(|| ImportError::LocalCover {
            detail: format!(
                "Selected cover {selected_cover_path} not found among discovered images"
            ),
        })?;

    let path = cover_file.version.path.clone();
    let bytes =
        bae_util::task_failure::blocking("read the import's cover", move || std::fs::read(&path))
            .await
            .map_err(|failed| diagnostics.bug(failed))?
            .map_err(|e| ImportError::LocalCover {
                detail: format!(
                    "Failed to read cover art {}: {e}",
                    cover_file.version.path.display()
                ),
            })?;

    Ok(bytes)
}

#[cfg(test)]
#[path = "cover_image_tests.rs"]
mod tests;
