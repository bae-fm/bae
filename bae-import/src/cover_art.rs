//! The cover a candidate is committed with, and the gallery its picker offers.
//! What catalogs serve — the covers, their image sets, the cache that reads
//! them — is [`bae_catalogs::remote_image`].

use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::{RemoteCover, RemoteImageSet};

/// The persisted owner whose external identities supply a cover gallery.
pub enum CoverTarget {
    Release(String),
    Candidate(crate::CandidateKey),
}

/// Whether an owner names an external release, and the artwork it offers.
/// An empty linked gallery is not an unidentified release.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteCoverGallery {
    Unlinked,
    Linked(Vec<RemoteCover>),
}

/// Where a cover's bytes are read from — a remote address, a file the folder
/// holds, the candidate's stored file-tag snapshot, or the library. Every
/// source names the identity its bytes have, so a reader can cache what it
/// drew under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverImageSource {
    /// The cover of the library release imported from the folder's files.
    Library {
        image: bae_db::ImageRef,
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
    Embedded {
        source: crate::folder_scanner::FileVersion,
        data: Vec<u8>,
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
pub struct CoverChoice {
    pub selection: CoverSelection,
    pub image: CoverImageSource,
}

impl CoverChoice {
    /// A catalog's image, with every copy the catalog serves of it.
    pub fn remote(image: RemoteImageSet, source: Catalog) -> Self {
        Self {
            selection: CoverSelection::Remote(image.clone(), source),
            image: CoverImageSource::Remote { image },
        }
    }

    /// One of the folder's own images, named by its relative path and drawn
    /// from where it sits on disk.
    pub fn local(file_id: String, file: crate::folder_scanner::FileVersion) -> Self {
        Self { selection: CoverSelection::Local(file_id), image: CoverImageSource::Local { file } }
    }

    /// Artwork stored in the candidate's file-tag snapshot. The source file
    /// identifies the selection; the snapshot owns the exact bytes rendered
    /// by both the pane and the sidebar, read from `source`.
    pub fn embedded(
        source_file_id: String,
        source: crate::folder_scanner::FileVersion,
        data: Vec<u8>,
    ) -> Self {
        Self {
            selection: CoverSelection::Embedded(source_file_id),
            image: CoverImageSource::Embedded { source, data },
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

#[cfg(test)]
#[path = "cover_art_tests.rs"]
mod tests;
