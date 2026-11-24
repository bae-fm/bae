//! What the app asks core to make a picture of. The types are in every app's
//! bindings, the shared Swift package's among them; only the desktop core reads
//! them.

use super::*;

/// An import candidate's picture, by what it comes from and the version of
/// the file it comes from.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePictureSource {
    /// An image file of the candidate's folder, as the scan read it.
    FolderFile { file: BridgeFileVersion },
    /// Art embedded in the tags of one of the candidate's audio files: `source`
    /// is that file as the scan read it.
    EmbeddedArt { source: BridgeFileVersion, candidate_key: Vec<u8>, source_file_id: String },
}

/// Whether a screen shows a picture now, or may show it soon. Core reads what
/// is shown first.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgePictureTurn {
    Visible,
    Prefetch,
}

#[cfg(desktop)]
impl BridgeFileVersion {
    fn into_core(self) -> Result<bae_import::folder_scanner::FileVersion, BridgeError> {
        Ok(bae_import::folder_scanner::FileVersion {
            path: crate::from_app::path(self.path)?,
            size: self.size,
            modified_at_ns: self.modified_at_ns,
        })
    }
}

#[cfg(desktop)]
impl BridgePictureSource {
    pub(crate) fn into_core(self) -> Result<bae_import::PictureSource, BridgeError> {
        Ok(match self {
            Self::FolderFile { file } => bae_import::PictureSource::FolderFile(file.into_core()?),
            Self::EmbeddedArt { source, candidate_key, source_file_id } => {
                bae_import::PictureSource::EmbeddedArt {
                    candidate: candidate_key.into(),
                    file: source.into_core()?,
                    source_file_id,
                }
            }
        })
    }
}

#[cfg(desktop)]
impl BridgePictureTurn {
    pub(crate) fn into_core(self) -> bae_import::PicturePriority {
        match self {
            Self::Visible => bae_import::PicturePriority::Visible,
            Self::Prefetch => bae_import::PicturePriority::Prefetch,
        }
    }
}
