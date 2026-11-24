//! A candidate's pictures as a screen asks for them: at a pixel size, or in
//! full, through the reader in `crate::pictures`.

use super::*;

impl ImportServiceHandle {
    /// The bytes of the import-list picture `source`: a copy at the step that
    /// covers `pixels`, or the original's own bytes for `None`. The picture
    /// must be an image of a candidate in the import list, or art in one of
    /// its audio files. Dropping the call ends the read when no other request
    /// wants the same picture and size.
    pub async fn read_picture(
        &self,
        source: crate::ImportPicture,
        pixels: Option<u32>,
        priority: bae_pictures::Priority,
    ) -> Result<Vec<u8>, crate::ImportError> {
        self.pictures.read(&self.services, source, pixels, priority).await
    }
}

#[cfg(test)]
#[path = "pictures_tests.rs"]
mod tests;
