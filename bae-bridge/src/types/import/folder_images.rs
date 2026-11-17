//! A candidate's folder images as the Mac's lightbox reads them.

use crate::types::BridgeMappingImage;

/// Which candidate's folder images a lightbox shows, and the file id of the
/// image the person moved to last.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFolderImagesRequest {
    pub candidate_key: Vec<u8>,
    pub focus: Option<String>,
}

impl BridgeFolderImagesRequest {
    pub(crate) fn from_core(request: bae_import::FolderImagesRequest) -> Self {
        Self { candidate_key: request.candidate.into(), focus: request.focus }
    }

    pub(crate) fn into_core(self) -> bae_import::FolderImagesRequest {
        bae_import::FolderImagesRequest { candidate: self.candidate_key.into(), focus: self.focus }
    }
}

/// The file ids of the images before and after the one a lightbox shows,
/// wrapping around the ends.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderImageNeighbors {
    pub previous: String,
    pub next: String,
}

/// The image a lightbox shows, and the images before and after it, which a
/// lone image lacks.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderImageFocus {
    pub image: BridgeMappingImage,
    pub neighbors: Option<BridgeFolderImageNeighbors>,
}

/// A candidate's folder images as its lightbox shows them, and the image it
/// shows, or none while the folder holds no image.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderImages {
    pub images: Vec<BridgeMappingImage>,
    pub focus: Option<BridgeFolderImageFocus>,
}

/// One value the read delivered: the request it answered, and the images.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderImagesSnapshot {
    pub request: Option<BridgeFolderImagesRequest>,
    pub images: Option<BridgeFolderImages>,
}

impl BridgeFolderImagesSnapshot {
    pub(crate) fn from_core(snapshot: bae_import::FolderImagesSnapshot) -> Self {
        Self {
            request: snapshot.request.map(BridgeFolderImagesRequest::from_core),
            images: snapshot.images.map(|images| BridgeFolderImages {
                images: images.images.into_iter().map(BridgeMappingImage::from_core).collect(),
                focus: images.focus.map(|focus| BridgeFolderImageFocus {
                    image: BridgeMappingImage::from_core(focus.image),
                    neighbors: focus.neighbors.map(|neighbors| BridgeFolderImageNeighbors {
                        previous: neighbors.previous,
                        next: neighbors.next,
                    }),
                }),
            }),
        }
    }
}
