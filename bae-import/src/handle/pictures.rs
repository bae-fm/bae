//! A candidate's pictures as a screen asks for them: at a pixel size, or in
//! full, through the reader in `crate::pictures`.

use super::*;

impl ImportServiceHandle {
    /// The bytes of the import-list picture `source`: a copy at the step that
    /// covers `pixels`, or the original's own bytes for `None`. The picture
    /// must be an image of a candidate in the import list, or art in one of
    /// its audio files. `turn` says how soon this request wants the picture,
    /// and may change while the call waits. Dropping the call ends the read
    /// when no other request wants the same picture and size.
    pub async fn read_picture(
        &self,
        source: crate::ImportPicture,
        pixels: u32,
        turn: bae_pictures::AskedTurn,
    ) -> Result<Vec<u8>, crate::ImportError> {
        self.pictures.read(source, pixels, turn).await
    }

    /// The original of the import-list picture `source`, for a read to its
    /// end such as the viewer's tiles: refused when the picture is no image
    /// of a candidate in the import list.
    pub async fn open_picture_original(
        &self,
        source: crate::ImportPicture,
    ) -> Result<Box<dyn bae_pictures::PictureOrigin>, crate::ImportError> {
        self.pictures.open_original(source).await
    }
}

#[cfg(test)]
#[path = "pictures_tests.rs"]
mod tests;
