//! The cover an import stores: the image its candidate's draft holds and
//! the copies bae makes of it.

use crate::cover_art::PreparedCover;
use crate::file_tag_snapshot::FileTagSnapshot;
use crate::folder_scanner::CandidateFile;
use crate::ImportError;
use bae_catalogs::remote_image::RemoteImage;
use bae_records::{CoverCopy, NewCover, NewCoverPicture};
use bae_util::content_type::ContentType;

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

    /// The cover as the import writes it: the picture it is made from and its
    /// browse and large copies. A folder's image is the release's own file
    /// `release_file_ids` names, and its copies come from the copies its
    /// candidate keeps, read from the folder only when none exists. A
    /// catalog's image and embedded art become the cover's original, which
    /// the import keeps in the cover's own namespace beside the copies.
    pub(super) async fn made(
        self,
        pictures: &crate::pictures::ImportPictures,
        discovered_files: &[CandidateFile],
        release_file_ids: &std::collections::HashMap<std::path::PathBuf, String>,
        release_id: &str,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Result<NewCover, ImportError> {
        let (picture, browse, large) = match self {
            Self::Local(path) => {
                let file = folder_cover(discovered_files, &path)?;
                let file_id =
                    release_file_ids.get(&file.version.path).cloned().ok_or_else(|| {
                        ImportError::Internal {
                            detail: format!("the selected cover {path} has no release file"),
                        }
                    })?;
                let picture = crate::ImportPicture::Folder { file: file.version.clone() };
                let browse = pictures
                    .read(
                        picture.clone(),
                        Some(CoverCopy::Browse.edge()),
                        bae_pictures::Priority::Visible,
                    )
                    .await?;
                let large = pictures
                    .read(picture, Some(CoverCopy::Large.edge()), bae_pictures::Priority::Visible)
                    .await?;
                (NewCoverPicture::ReleaseFile { file_id }, browse, large)
            }
            Self::Remote(bytes) | Self::Embedded(bytes) => {
                // Names the format it refuses before libvips fails on it.
                bae_util::stored_image::inspect_image(&bytes)
                    .map_err(|detail| ImportError::CoverArt { detail })?;
                let (browse, large) = copies_of(&bytes, diagnostics).await?;
                (NewCoverPicture::Original { bytes }, browse, large)
            }
        };
        let content_type = copy_content_type(&browse)?;
        if copy_content_type(&large)? != content_type {
            return Err(ImportError::Internal {
                detail: "a cover's browse and large copies are in different formats".to_string(),
            });
        }
        Ok(NewCover { release_id: release_id.to_string(), picture, browse, large, content_type })
    }
}

/// The browse and large copies of the picture `bytes` hold, made on a
/// blocking thread: libvips decodes and shrinks them, CPU work.
async fn copies_of(
    bytes: &[u8],
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<(Vec<u8>, Vec<u8>), ImportError> {
    let bytes = bytes.to_vec();
    diagnostics
        .blocking("make the import's cover copies", move || {
            Ok::<_, bae_pictures::PicturesError>((
                bae_pictures::make_copy_of_bytes(bytes.clone(), bae_pictures::PictureStep::S768)?
                    .bytes,
                bae_pictures::make_copy_of_bytes(bytes, bae_pictures::PictureStep::S1536)?.bytes,
            ))
        })
        .await?
        .map_err(|error| ImportError::CoverArt { detail: error.to_string() })
}

/// The content type of a copy of a picture: PNG for a picture with
/// transparency, else JPEG.
pub(super) fn copy_content_type(bytes: &[u8]) -> Result<ContentType, ImportError> {
    let header = &bytes[..bytes.len().min(ContentType::IMAGE_HEADER_LEN)];
    [ContentType::Jpeg, ContentType::Png]
        .into_iter()
        .find(|content_type| content_type.opens_with(header) == Some(true))
        .ok_or_else(|| ImportError::Internal {
            detail: "a picture copy is neither a JPEG nor a PNG".to_string(),
        })
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

/// The selected cover's file among the folder's discovered files.
///
/// The path is the candidate's stored selection, so a folder that no longer
/// holds that image is a candidate whose selection no longer describes it,
/// and that is stated rather than replaced by some other image. Only the
/// artwork the folder offers as a cover can be it: a damaged image of the
/// same name is no cover.
fn folder_cover<'a>(
    discovered_files: &'a [CandidateFile],
    selected_cover_path: &str,
) -> Result<&'a crate::folder_scanner::ScannedFile, ImportError> {
    discovered_files
        .iter()
        .find(|entry| {
            entry.file.relative_path == selected_cover_path && entry.cover_choice().is_some()
        })
        .map(|entry| &entry.file)
        .ok_or_else(|| ImportError::LocalCover {
            detail: format!(
                "Selected cover {selected_cover_path} not found among discovered images"
            ),
        })
}

#[cfg(test)]
#[path = "cover_image_tests.rs"]
mod tests;
