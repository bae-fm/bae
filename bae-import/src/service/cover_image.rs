//! The cover an import stores: the image its candidate's draft holds and
//! the copies bae makes of it.

use crate::cover_art::PreparedCover;
use crate::file_tag_snapshot::FileTagSnapshot;
use crate::folder_scanner::CandidateFile;
use crate::make_cover_copies;
use crate::ImportError;
use bae_catalogs::remote_image::RemoteImage;
use bae_records::{CoverCopy, NewCover, NewCoverPicture};

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
    /// The artwork embedded in one of the candidate's audio files, by the
    /// file's path under the candidate's folder, with the art's bytes as the
    /// file-tag snapshot that seeded the pane captured them, so the import
    /// never opens the tags a second time.
    Embedded { source_relative_path: String, art: Vec<u8> },
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
                        Ok(Some(Self::Embedded {
                            source_relative_path: source_file_id,
                            art: cover.data.clone(),
                        }))
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
    /// candidate keeps, read from the folder only when none exists. Embedded
    /// art names the release's audio file that holds it, and its copies come
    /// from the art the snapshot captured; where that audio is one of
    /// `left_out_audio`, which the release has no file of, the art becomes the
    /// cover's original. A catalog's image becomes the
    /// cover's original, which the import keeps in the cover's own namespace
    /// beside the copies.
    pub(super) async fn made(
        self,
        pictures: &crate::pictures::ImportPictures,
        discovered_files: &[CandidateFile],
        left_out_audio: &[String],
        release_file_ids: &std::collections::HashMap<std::path::PathBuf, String>,
        release_id: &str,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Result<NewCover, ImportError> {
        let (picture, copies) = match self {
            Self::Local(path) => {
                let file = folder_cover(discovered_files, &path)?;
                let file_id = release_file_id(release_file_ids, file, &path)?;
                let picture = crate::ImportPicture::Folder { file: file.version.clone() };
                let turn = bae_pictures::AskedTurn::unchanging(bae_pictures::PictureTurn::Visible);
                let browse = pictures
                    .read_copy(picture.clone(), CoverCopy::Browse.step(), turn.clone())
                    .await?;
                let large = pictures.read_copy(picture, CoverCopy::Large.step(), turn).await?;
                (NewCoverPicture::ReleaseFile { file_id }, crate::cover_copies_of(browse, large)?)
            }
            Self::Embedded { source_relative_path, art }
                if left_out_audio.contains(&source_relative_path) =>
            {
                let copies = copies_of_bytes(art.clone(), diagnostics).await?;
                (NewCoverPicture::Original { bytes: art }, copies)
            }
            Self::Embedded { source_relative_path, art } => {
                let file = art_source(discovered_files, &source_relative_path)?;
                let file_id = release_file_id(release_file_ids, file, &source_relative_path)?;
                (NewCoverPicture::Embedded { file_id }, copies_of_bytes(art, diagnostics).await?)
            }
            Self::Remote(bytes) => {
                let copies = copies_of_bytes(bytes.clone(), diagnostics).await?;
                (NewCoverPicture::Original { bytes }, copies)
            }
        };
        Ok(NewCover { release_id: release_id.to_string(), picture, copies })
    }
}

/// The cover's copies made from a picture held in memory, refused with the
/// format it names before libvips fails on it.
async fn copies_of_bytes(
    bytes: Vec<u8>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<bae_records::CoverCopies, ImportError> {
    bae_util::stored_image::inspect_image(&bytes)
        .map_err(|detail| ImportError::CoverArt { detail })?;
    make_cover_copies(bytes, diagnostics).await
}

/// The id the release gives `file`, the cover's picture at `path` under the
/// candidate's folder.
fn release_file_id(
    release_file_ids: &std::collections::HashMap<std::path::PathBuf, String>,
    file: &crate::folder_scanner::ScannedFile,
    path: &str,
) -> Result<String, ImportError> {
    release_file_ids.get(&file.version.path).cloned().ok_or_else(|| ImportError::Internal {
        detail: format!("the selected cover {path} has no release file"),
    })
}

/// The audio file at `source_relative_path` among the folder's discovered
/// files, whose tags hold the selected cover's art. The path is the
/// candidate's stored selection, which the file-tag snapshot matched, so a
/// folder that no longer holds that file is stated.
fn art_source<'a>(
    discovered_files: &'a [CandidateFile],
    source_relative_path: &str,
) -> Result<&'a crate::folder_scanner::ScannedFile, ImportError> {
    discovered_files
        .iter()
        .find(|entry| entry.file.relative_path == source_relative_path)
        .map(|entry| &entry.file)
        .ok_or_else(|| ImportError::Internal {
            detail: format!(
                "the audio file {source_relative_path} whose art is the cover is not among the \
                 discovered files"
            ),
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
