//! The pictures of the artists an import creates: which of its new artists
//! are Discogs artists, Discogs' answer for each, fetched before the import
//! writes anything, and the library image rows the import's write stages for
//! them.

use super::ImportService;
use std::collections::HashMap;

/// Discogs' answer for the picture of one artist an import is about to
/// create, fetched by that import before it writes anything. `Nothing` is a
/// real answer: Discogs has no picture of that artist.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum DiscogsArtistImage {
    Image {
        discogs_artist_id: String,
        /// The picture as bae stores it: the JPEG
        /// [`bae_util::stored_image::stored_jpeg`] made of what Discogs
        /// served.
        jpeg: Vec<u8>,
    },
    Nothing {
        discogs_artist_id: String,
    },
}

impl DiscogsArtistImage {
    fn discogs_artist_id(&self) -> &str {
        match self {
            Self::Image { discogs_artist_id, .. } | Self::Nothing { discogs_artist_id } => {
                discogs_artist_id
            }
        }
    }
}

/// The pictures an import's write stages for the artists it creates, read
/// against the library just before the write: the artists it expects to
/// create, and a library image row with its bytes for each that has one.
pub(super) struct StagedArtistPictures {
    pub(super) expected_new_artists: Vec<String>,
    pub(super) images: Vec<(bae_db::DbLibraryImage, Vec<u8>)>,
}

impl ImportService {
    /// The Discogs identities of the artists an import of `credits` would
    /// create, whose pictures it fetches before it writes anything. An artist
    /// the credits resolve to in the library keeps its own picture, so it is
    /// not among them.
    pub(super) async fn new_discogs_artists(
        &self,
        credits: &[bae_db::DbArtist],
        picked: &[String],
    ) -> Result<std::collections::BTreeSet<String>, crate::ImportError> {
        let resolved = self.tables.resolve_artists(credits, picked).await?;
        Ok(resolved.inserts.iter().filter_map(|artist| artist.discogs_artist_id.clone()).collect())
    }

    /// The pictures `fetched` staged for the artists `artists` would create
    /// as the library stands now. The write resolves the credits again and
    /// refuses to commit if it would create others.
    pub(super) async fn staged_artist_pictures(
        &self,
        artists: &bae_db::ArtistCredits<'_>,
        fetched: &[DiscogsArtistImage],
    ) -> Result<StagedArtistPictures, crate::ImportError> {
        let expected = self.tables.resolve_artists(artists.credits, artists.picked).await?;
        Ok(StagedArtistPictures {
            expected_new_artists: expected.inserts.iter().map(|artist| artist.id.clone()).collect(),
            images: artist_image_rows(&expected.inserts, fetched)?,
        })
    }

    /// Discogs' answer for the picture of each artist `ids` names, asked now,
    /// one artist at a time, each picture downscaled to the JPEG bae stores.
    /// A provider or download failure, or a picture that does not decode,
    /// fails the caller with it.
    pub(super) async fn fetch_discogs_artist_images(
        &self,
        ids: std::collections::BTreeSet<String>,
    ) -> Result<Vec<DiscogsArtistImage>, crate::ImportError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let session = self.catalogs.session().await?;
        let mut answers = Vec::with_capacity(ids.len());
        for discogs_artist_id in ids {
            let Some(source_url) = session.artist_image_url(&discogs_artist_id).await? else {
                answers.push(DiscogsArtistImage::Nothing { discogs_artist_id });
                continue;
            };
            let Some(image) = self.remote_images.fetch(&source_url).await? else {
                answers.push(DiscogsArtistImage::Nothing { discogs_artist_id });
                continue;
            };
            // Decoding and re-encoding the image is CPU work.
            let jpeg = bae_util::task_failure::blocking("resize an artist picture", move || {
                bae_util::stored_image::stored_jpeg(&image.bytes)
            })
            .await
            .map_err(|failed| self.diagnostics.bug(failed))?
            .map_err(|detail| crate::ImportError::CoverArt { detail })?;
            answers.push(DiscogsArtistImage::Image { discogs_artist_id, jpeg });
        }
        Ok(answers)
    }
}

/// The library image rows of the pictures `fetched` for the new artists an
/// import creates. An artist the import links to keeps its own
/// picture; this constructs rows only.
///
/// A new Discogs artist with no answer among `fetched` was a library artist
/// when the import fetched its pictures and is not one now: the library's
/// artists changed under the import, which fails so it can be run again.
fn artist_image_rows(
    inserted_artists: &[bae_db::DbArtist],
    fetched: &[DiscogsArtistImage],
) -> Result<Vec<(bae_db::DbLibraryImage, Vec<u8>)>, crate::ImportError> {
    let by_discogs_id: HashMap<_, _> =
        fetched.iter().map(|answer| (answer.discogs_artist_id(), answer)).collect();
    if by_discogs_id.len() != fetched.len() {
        return Err(crate::ImportError::Internal {
            detail: "fetched artist images contain a duplicate Discogs artist ID".into(),
        });
    }
    let mut images = Vec::new();
    for artist in inserted_artists {
        let Some(discogs_artist_id) = artist.discogs_artist_id.as_deref() else {
            continue;
        };
        let Some(answer) = by_discogs_id.get(discogs_artist_id) else {
            return Err(crate::ImportError::LibraryChanged);
        };
        let DiscogsArtistImage::Image { jpeg, .. } = answer else {
            continue;
        };
        let row = bae_db::DbLibraryImage::stored_jpeg(bae_db::LibraryImageType::Artist, &artist.id);
        images.push((row, jpeg.clone()));
    }
    Ok(images)
}

#[cfg(test)]
#[path = "artist_pictures_tests.rs"]
pub(crate) mod tests;
