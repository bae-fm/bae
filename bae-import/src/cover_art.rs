//! The cover a candidate is committed with, and the rules that pick it from
//! the folder's images and its files' embedded artwork. What catalogs serve
//! — the covers, their image sets, the cache that reads them — is
//! [`bae_catalogs::remote_image`].

use crate::folder_scanner::ScannedFile;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::RemoteImageSet;
use bae_util::content_type_hint::ContentTypeHint;

/// Where a cover's bytes are read from — a remote address, a file the folder
/// holds, the candidate's stored file-tag snapshot, or the library. Every
/// source names the identity its bytes have, so a reader can cache what it
/// drew under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverImageSource {
    /// The cover of the library release imported from the folder's files.
    Library {
        image: bae_records::ImageRef,
    },
    Remote {
        image: RemoteImageSet,
    },
    /// An image file of the folder, at the version the scan read.
    Local {
        file: crate::folder_scanner::FileVersion,
    },
    /// Artwork embedded in an audio file's tags, as the file-tag snapshot
    /// stored it, with the version of that file the snapshot read it from.
    /// The bytes are not here: a list row names the artwork, and the reader
    /// that draws it fetches them from the candidate's snapshot by
    /// `candidate` and `source_file_id`.
    Embedded {
        source: crate::folder_scanner::FileVersion,
        candidate: crate::CandidateKey,
        source_file_id: String,
    },
}

/// User's cover art selection for an import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoverSelection {
    /// A catalog's image, and the catalog for attribution. The import
    /// commits the original; the downscaled copies are what smaller slots
    /// showing the selection draw.
    Remote(bae_catalogs::remote_image::RemoteImageSet, Catalog),
    /// Local file in the album folder (relative path from album root)
    Local(String),
    /// Artwork embedded in one audio file, identified by that file's relative
    /// path in the file-tag snapshot.
    Embedded(String),
}

/// Why a cover was refused: it names something the candidate's scan did not
/// store. A cover is one of the folder's images or the artwork its tags hold,
/// as the scan read them, and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoverRefusal {
    #[error("the folder holds no file {file_id}")]
    NotInFolder { file_id: String },
    #[error("{file_id} is not one of the folder's images")]
    NotAnImage { file_id: String },
    #[error("the tags read from {source_file_id} hold no artwork")]
    NoTagArtwork { source_file_id: String },
}

/// The cover a candidate's draft holds: a [`CoverSelection`] with whatever
/// the import needs to store it without asking anyone again. A catalog's
/// image carries the bytes fetched for it, so no draft holds a remote cover
/// the import would have to fetch; an image the folder holds needs nothing
/// beyond its name.
#[derive(Clone, Debug, PartialEq)]
pub enum PreparedCover {
    /// A catalog's image, the catalog for attribution, and the original's
    /// bytes as that catalog served them.
    Remote {
        image: RemoteImageSet,
        source: Catalog,
        fetched: bae_catalogs::remote_image::RemoteImage,
    },
    /// Local file in the album folder (relative path from album root)
    Local(String),
    /// Artwork embedded in one audio file, identified by that file's relative
    /// path in the file-tag snapshot.
    Embedded(String),
}

impl PreparedCover {
    /// What the cover names, without the bytes it carries.
    pub fn selection(&self) -> CoverSelection {
        match self {
            Self::Remote { image, source, .. } => CoverSelection::Remote(image.clone(), *source),
            Self::Local(file_id) => CoverSelection::Local(file_id.clone()),
            Self::Embedded(source_file_id) => CoverSelection::Embedded(source_file_id.clone()),
        }
    }
}

/// The cover a candidate will be committed with, and where to draw it from.
///
/// The selection is the candidate's stored one — what the scan read off the
/// folder, what its identification fetched, or what the person chose. A
/// candidate with none stored commits with no cover, so there is no such
/// thing here as a cover that only a reader knows about.
///
/// One image, not a preview and a thumbnail: a remote image carries its
/// catalog's downscaled copies, and the size a slot draws at picks among them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverChoice {
    /// A catalog's image, with every copy the catalog serves of it.
    Remote { image: RemoteImageSet, source: Catalog },
    /// One of the folder's own images, named by its relative path and drawn
    /// from where it sits on disk.
    Local { file_id: String, file: crate::folder_scanner::FileVersion },
    /// Artwork stored in the candidate's file-tag snapshot, named by the file
    /// it came from and drawn from the bytes the snapshot holds, which both
    /// the pane and the list render. The choice names the artwork without
    /// holding its bytes.
    Embedded {
        candidate: crate::CandidateKey,
        source_file_id: String,
        source: crate::folder_scanner::FileVersion,
    },
}

impl CoverChoice {
    /// What the choice names, without the image it draws.
    pub fn selection(&self) -> CoverSelection {
        match self {
            Self::Remote { image, source } => CoverSelection::Remote(image.clone(), *source),
            Self::Local { file_id, .. } => CoverSelection::Local(file_id.clone()),
            Self::Embedded { source_file_id, .. } => {
                CoverSelection::Embedded(source_file_id.clone())
            }
        }
    }

    /// Where the choice's bytes are drawn from.
    pub fn into_image(self) -> CoverImageSource {
        match self {
            Self::Remote { image, .. } => CoverImageSource::Remote { image },
            Self::Local { file, .. } => CoverImageSource::Local { file },
            Self::Embedded { candidate, source_file_id, source } => {
                CoverImageSource::Embedded { source, candidate, source_file_id }
            }
        }
    }
}

/// The cover a draft read from its files' tags is committed with: the
/// snapshot's embedded artwork, when its bytes are an image the import can
/// store. Folder artwork remains the candidate's source-neutral fallback.
pub(crate) fn embedded_cover_selection(
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
) -> Option<PreparedCover> {
    let cover = snapshot.embedded_cover.as_ref()?;
    cover
        .content_type
        .is_supported_cover()
        .then(|| PreparedCover::Embedded(cover.source_relative_path.clone()))
}

/// The cover a folder holds for itself, first of:
///
/// 1. a folder image named as the front cover (`cover`, `folder`, `front`;
///    see [`conventional_artwork_name`]) — the image the folder's owner put
///    there to be the cover;
/// 2. the artwork its own audio embeds — `embedded`, the selection the
///    file-tag snapshot names;
/// 3. any other folder image, by [`default_local_cover_file`]'s order.
///
/// `None` when the folder has none of them.
///
/// One rule, run wherever a candidate's cover comes from its folder: the
/// scan that creates its state stores it, and a candidate holding no draft
/// row, whose blank cover no row stores, reads it from its scanned files.
pub(crate) fn folder_cover<'a>(
    embedded: Option<PreparedCover>,
    artwork: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<PreparedCover> {
    let artwork = artwork.into_iter().collect::<Vec<_>>();
    front_cover_file(artwork.iter().copied())
        .map(local_selection)
        .or(embedded)
        .or_else(|| default_local_cover_file(artwork).map(local_selection))
}

/// The cover applying a folder's file tags selects: the tags' embedded
/// artwork, unless the folder holds an image named as the front cover, which
/// [`folder_cover`] ranks ahead of it. `None` when the tags embed nothing, so
/// applying them leaves the candidate's cover as it is.
pub(crate) fn file_tags_cover<'a>(
    embedded: Option<PreparedCover>,
    artwork: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<PreparedCover> {
    embedded.map(|embedded| front_cover_file(artwork).map(local_selection).unwrap_or(embedded))
}

/// The folder image named as the front cover, by [`artwork_order`] among
/// several.
fn front_cover_file<'a>(
    files: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<&'a ScannedFile> {
    files
        .into_iter()
        .filter(|file| {
            ContentTypeHint::path_is_supported_cover(&file.version.path)
                && conventional_artwork_name(file).is_some()
        })
        .min_by(|left, right| artwork_order(left, right))
}

fn local_selection(file: &ScannedFile) -> PreparedCover {
    PreparedCover::Local(file.relative_path.clone())
}

/// Select the folder image used when no source supplies artwork. Every caller
/// shares this ordering; only the selected file's later use differs.
pub(crate) fn default_local_cover_file<'a>(
    files: impl IntoIterator<Item = &'a ScannedFile>,
) -> Option<&'a ScannedFile> {
    files
        .into_iter()
        .filter(|file| ContentTypeHint::path_is_supported_cover(&file.version.path))
        .min_by(|left, right| artwork_order(left, right))
}

fn artwork_order(left: &ScannedFile, right: &ScannedFile) -> std::cmp::Ordering {
    let left_name = left.relative_path.to_lowercase();
    let right_name = right.relative_path.to_lowercase();
    match (conventional_artwork_name(left), conventional_artwork_name(right)) {
        (Some(left_artwork), Some(right_artwork)) => left_artwork
            .cmp(&right_artwork)
            .then_with(|| left.version.size.cmp(&right.version.size))
            .then_with(|| left.relative_path.cmp(&right.relative_path)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left
            .version
            .size
            .cmp(&right.version.size)
            .then_with(|| left_name.cmp(&right_name))
            .then_with(|| left.relative_path.cmp(&right.relative_path)),
    }
}

fn conventional_artwork_name(file: &ScannedFile) -> Option<String> {
    let stem = std::path::Path::new(&file.relative_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| {
            stem.trim_start_matches(|character: char| {
                character.is_ascii_digit() || !character.is_alphanumeric()
            })
            .to_lowercase()
        })?;
    let mut words =
        stem.split(|character: char| !character.is_alphanumeric()).filter(|word| !word.is_empty());
    let first = words.next()?;
    let names_front_artwork = ["cover", "folder", "front"].contains(&first);
    let names_non_front_artwork = words
        .any(|word| ["back", "rear", "disc", "cd", "booklet", "inlay", "inside"].contains(&word));
    (names_front_artwork && !names_non_front_artwork).then_some(stem)
}

#[cfg(test)]
#[path = "cover_art_tests.rs"]
mod tests;
