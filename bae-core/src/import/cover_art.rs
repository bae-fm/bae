//! The cover a candidate is committed with, and the gallery its picker offers.
//! What catalogs serve — the covers, their image sets, the cache that reads
//! them — is [`bae_catalogs::remote_image`].

use bae_catalogs::catalog::Catalog;
use bae_catalogs::remote_image::{RemoteCover, RemoteImageSet};

/// The persisted owner whose external identities supply a cover gallery.
pub enum CoverTarget {
    Release(String),
    Candidate(crate::import::CandidateKey),
}

/// Whether an owner names an external release, and the artwork it offers.
/// An empty linked gallery is not an unidentified release.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteCoverGallery {
    Unlinked,
    Linked(Vec<RemoteCover>),
}

/// Where a cover's bytes are read from — a remote address, a file the folder
/// holds, or the candidate's stored file-tag snapshot. Every source names the
/// identity its bytes have, so a reader can cache what it drew under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverImageSource {
    Remote {
        image: RemoteImageSet,
    },
    /// An image file of the folder, at the version the scan read.
    Local {
        file: crate::import::folder_scanner::FileVersion,
    },
    /// Artwork embedded in an audio file's tags, as the file-tag snapshot
    /// stored it, with the version of that file the snapshot read it from.
    Embedded {
        source: crate::import::folder_scanner::FileVersion,
        data: Vec<u8>,
    },
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
    pub selection: crate::import::CoverSelection,
    pub image: CoverImageSource,
}

impl CoverChoice {
    /// A catalog's image, with every copy the catalog serves of it.
    pub fn remote(image: RemoteImageSet, source: Catalog) -> Self {
        Self {
            selection: crate::import::CoverSelection::Remote(image.clone(), source),
            image: CoverImageSource::Remote { image },
        }
    }

    /// One of the folder's own images, named by its relative path and drawn
    /// from where it sits on disk.
    pub fn local(file_id: String, file: crate::import::folder_scanner::FileVersion) -> Self {
        Self {
            selection: crate::import::CoverSelection::Local(file_id),
            image: CoverImageSource::Local { file },
        }
    }

    /// Artwork stored in the candidate's file-tag snapshot. The source file
    /// identifies the selection; the snapshot owns the exact bytes rendered
    /// by both the pane and the sidebar, read from `source`.
    pub fn embedded(
        source_file_id: String,
        source: crate::import::folder_scanner::FileVersion,
        data: Vec<u8>,
    ) -> Self {
        Self {
            selection: crate::import::CoverSelection::Embedded(source_file_id),
            image: CoverImageSource::Embedded { source, data },
        }
    }
}
