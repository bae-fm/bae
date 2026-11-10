//! What kind of file a release file is, and where it sits in the release's
//! folder, as a file's details show them.

use bae_records::{DocumentKind, ReleaseFileRole};
use bae_util::content_type::ContentType;

/// What kind of file a release file is: the role, and for a document the
/// kind, its import stored, with the format its content type names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    /// Audio, with its format's name ("FLAC").
    Audio {
        format: String,
    },
    /// An image, with its format's name ("JPEG").
    Image {
        format: String,
    },
    /// Named as an image, and its bytes do not decode as one.
    DamagedImage,
    /// A CUE sheet, whether or not it parsed as a track sheet.
    CueSheet,
    /// A ripper's log of how it read the disc.
    RipLog,
    /// The report of a rip checked against AccurateRip's database.
    AccurateRipReport,
    Playlist,
    Text,
    Pdf,
    Other,
}

impl FileKind {
    pub(crate) fn of(role: ReleaseFileRole, content_type: &ContentType) -> Self {
        match role {
            ReleaseFileRole::Audio => {
                Self::Audio { format: content_type.display_name().to_string() }
            }
            ReleaseFileRole::Artwork => {
                Self::Image { format: content_type.display_name().to_string() }
            }
            ReleaseFileRole::DamagedImage => Self::DamagedImage,
            ReleaseFileRole::TrackSheet | ReleaseFileRole::Document(DocumentKind::CueSheet) => {
                Self::CueSheet
            }
            ReleaseFileRole::Document(DocumentKind::RipLog) => Self::RipLog,
            ReleaseFileRole::Document(DocumentKind::AccurateRipReport) => Self::AccurateRipReport,
            ReleaseFileRole::Document(DocumentKind::Playlist) => Self::Playlist,
            ReleaseFileRole::Document(DocumentKind::Text) => Self::Text,
            // Whatever else the folder carried: a PDF, or an image the scan
            // offers as no artwork, such as an SVG, says so by its content type.
            ReleaseFileRole::Other => match content_type {
                ContentType::Pdf => Self::Pdf,
                content_type if content_type.is_image() => {
                    Self::Image { format: content_type.display_name().to_string() }
                }
                _ => Self::Other,
            },
        }
    }
}

/// A file's place in its release's folder: its own name, and the folders
/// above it (`Disc 1`), `None` for a file at the folder's top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePlace {
    pub name: String,
    pub folder: Option<String>,
}

impl FilePlace {
    /// `original_filename` is the file's path in its release's folder, with
    /// `/` between folders, as import stores it.
    pub(crate) fn of(original_filename: &str) -> Self {
        match original_filename.rsplit_once('/') {
            Some((folder, name)) => {
                Self { name: name.to_string(), folder: Some(folder.to_string()) }
            }
            None => Self { name: original_filename.to_string(), folder: None },
        }
    }
}

#[cfg(test)]
#[path = "file_kind_tests.rs"]
mod tests;
