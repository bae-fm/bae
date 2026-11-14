//! Where a cover's image comes from and what saving it selects.

use super::*;

/// Where a cover's bytes are read from. Every source carries the identity its
/// bytes have, so the app caches what it drew under it without reading the
/// file to learn whether it changed.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverImageSource {
    /// The cover of the library release imported from the folder's files.
    Library {
        image: BridgeImageRef,
    },
    /// A slot of a library release's image strip: its saved cover or one of
    /// its image files, read by `fetch_release_image`.
    ReleaseImage {
        release_id: String,
        source: BridgeGallerySource,
    },
    Remote {
        image: BridgeRemoteImageSet,
    },
    /// An image file of the folder, at the version the scan read.
    Local {
        file: BridgeFileVersion,
    },
    /// Tag artwork at its source file's version; `read_embedded_cover` has its bytes.
    Embedded {
        source: BridgeFileVersion,
        candidate_key: Vec<u8>,
        source_file_id: String,
    },
}

/// One version of a file on disk: its absolute path, and the size and
/// modification time (nanoseconds since the Unix epoch) that tell the bytes
/// there from whatever is written at that path later. Mirror of bae-library's
/// `FileVersion`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFileVersion {
    /// Its path as coven's bytes (`coven::path_bytes`), which the app opens
    /// the file from.
    pub path: Vec<u8>,
    pub size: u64,
    pub modified_at_ns: i64,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverSelection {
    ReleaseImage { file_id: String },
    RemoteCover { selection: BridgeRemoteCoverSelection },
    EmbeddedCover { source_file_id: String },
}

/// A cover and the one image every slot showing it draws; the slot's size
/// picks among a remote image's copies.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverChoice {
    pub selection: BridgeCoverSelection,
    pub image: BridgeCoverImageSource,
}
