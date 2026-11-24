//! What the app asks core to make a picture of. The types are in every app's
//! bindings, the shared Swift package's among them; only the desktop core reads
//! them.

#[cfg(desktop)]
use super::*;

/// Whether a screen shows a picture now, or may show it soon. Core reads what
/// is shown first.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgePictureTurn {
    Visible,
    Prefetch,
}

mirror_enum! {
    #[cfg(desktop)]
    BridgePictureTurn = bae_import::PicturePriority,
    into_core: pub(crate) fn,
    variants: { Visible, Prefetch },
}

#[cfg(desktop)]
impl BridgeCoverImageSource {
    /// The picture of the import list this source names: an image file of a
    /// candidate's folder or art in its tags. A library or catalog image is
    /// not one, and the app asks core for it another way.
    pub(crate) fn into_picture(self) -> Result<bae_import::ImportPicture, BridgeError> {
        match self {
            Self::Local { file } => {
                Ok(bae_import::ImportPicture::Folder { file: file.into_core()? })
            }
            Self::Embedded { source, candidate_key, source_file_id } => {
                Ok(bae_import::ImportPicture::Embedded {
                    source: source.into_core()?,
                    candidate: candidate_key.into(),
                    source_file_id,
                })
            }
            Self::Library { .. } | Self::ReleaseImage { .. } | Self::Remote { .. } => {
                Err(BridgeError::internal(
                    "a library or catalog image is no picture of the import list".to_string(),
                ))
            }
        }
    }
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
