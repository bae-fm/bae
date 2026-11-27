//! What the app asks core to make a picture of. The types are in every app's
//! bindings, the shared Swift package's among them; only the desktop core reads
//! them.

#[cfg(desktop)]
use super::*;
use bae_types::picture_turn::PictureTurn;
use tokio::sync::watch;

/// How soon a request wants its picture: `Prefetch` for one a screen may show
/// soon, `Visible` for one a screen shows, and `Focused` for the picture the
/// person looks at among the several a screen shows. A later variant goes
/// first, in core's reads and in the apps' decodes.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgePictureTurn {
    Prefetch,
    Visible,
    Focused,
}

mirror_enum! {
    BridgePictureTurn = PictureTurn,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Prefetch, Visible, Focused },
}

/// The turn of one picture request, which the app hands the call and changes
/// while the call runs, as what the person looks at changes.
#[derive(uniffi::Object)]
pub struct BridgePictureTurnSignal {
    turn: watch::Sender<PictureTurn>,
}

#[uniffi::export]
impl BridgePictureTurnSignal {
    #[uniffi::constructor]
    pub fn new(turn: BridgePictureTurn) -> std::sync::Arc<Self> {
        let (turn, _) = watch::channel(turn.into_core());
        std::sync::Arc::new(Self { turn })
    }

    pub fn set(&self, turn: BridgePictureTurn) {
        self.turn.send_replace(turn.into_core());
    }

    /// The turn the request asks now.
    pub fn turn(&self) -> BridgePictureTurn {
        BridgePictureTurn::from_core(*self.turn.borrow())
    }
}

#[cfg(desktop)]
impl BridgePictureTurnSignal {
    /// The turn for one call to watch.
    pub(crate) fn watched(&self) -> bae_import::AskedTurn {
        self.turn.subscribe().into()
    }
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
