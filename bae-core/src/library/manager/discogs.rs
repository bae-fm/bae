//! Discogs operations owned by [`LibraryManager`]: the stored key's read,
//! and the rows of the artist pictures an import fetched from Discogs.

use super::*;
use std::collections::HashMap;

impl LibraryManager {
    pub async fn get_discogs_token(&self) -> Result<Option<String>, LibraryError> {
        self.host_secret(crate::keys::DISCOGS_API_KEY).await
    }

    /// The library image rows of the pictures `fetched` for the new artists
    /// an import creates. An artist the import links to keeps its own
    /// picture; this constructs rows only.
    ///
    /// A new Discogs artist with no answer among `fetched` was a library
    /// artist when the import fetched its pictures and is not one now: the
    /// library's artists changed under the import, which fails so it can be
    /// run again.
    #[cfg(desktop)]
    pub(crate) fn artist_image_rows(
        &self,
        inserted_artists: &[DbArtist],
        fetched: &[crate::import::DiscogsArtistImage],
    ) -> Result<Vec<(DbLibraryImage, Vec<u8>)>, LibraryError> {
        let by_discogs_id: HashMap<_, _> =
            fetched.iter().map(|answer| (answer.discogs_artist_id(), answer)).collect();
        if by_discogs_id.len() != fetched.len() {
            return Err(LibraryError::Internal(
                "fetched artist images contain a duplicate Discogs artist ID".into(),
            ));
        }
        let mut images = Vec::new();
        for artist in inserted_artists {
            let Some(discogs_artist_id) = artist.discogs_artist_id.as_deref() else {
                continue;
            };
            let Some(answer) = by_discogs_id.get(discogs_artist_id) else {
                return Err(crate::db::IdentityWriteError::LibraryChanged.into());
            };
            let crate::import::DiscogsArtistImage::Image { jpeg, .. } = answer else {
                continue;
            };
            let row = DbLibraryImage::stored_jpeg(LibraryImageType::Artist, &artist.id, self.now());
            images.push((row, jpeg.clone()));
        }
        Ok(images)
    }
}
