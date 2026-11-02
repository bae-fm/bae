//! Discogs operations owned by [`LibraryManager`]: the stored key, and the
//! reads that ask Discogs through the library's
//! [`crate::import::CatalogSessions`].

use super::*;
use crate::config::DiscogsValidation;
#[cfg(all(test, desktop))]
use bae_catalogs::discogs::client::DiscogsKeySignal;
#[cfg(desktop)]
use bae_catalogs::rate_limiter::CallPriority;
use std::collections::HashMap;

impl LibraryManager {
    pub async fn get_discogs_token(&self) -> Result<Option<String>, LibraryError> {
        self.host_secret(crate::keys::DISCOGS_API_KEY).await
    }

    /// Store the keyring bytes before recording the key's state. A failure
    /// between those writes leaves Discogs disabled until the caller retries;
    /// the preferences never claim a key that the keyring lacks. The keyring
    /// write blocks, so it runs on a blocking thread.
    pub async fn set_discogs_key(
        &self,
        token: &str,
        validation: DiscogsValidation,
    ) -> Result<(), LibraryError> {
        // No session is built from a key halfway through being replaced.
        self.catalogs
            .change_discogs_key(async {
                let database = self.database.clone();
                let token = token.to_string();
                blocking("store the Discogs key", move || {
                    database.set_host_secret(crate::keys::DISCOGS_API_KEY, &token)
                })
                .await
                .map_err(|failed| self.bug(failed))??;
                self.update_preferences(move |prefs| {
                    prefs.discogs = Some(validation);
                    Ok(())
                })
                .await?;
                Ok(())
            })
            .await
    }

    /// Clear the key's state before deleting the keyring bytes, so a failure
    /// between the writes leaves Discogs disabled rather than half-enabled.
    pub async fn clear_discogs_key(&self) -> Result<(), LibraryError> {
        self.catalogs
            .change_discogs_key(async {
                self.update_preferences(|prefs| {
                    prefs.discogs = None;
                    Ok(())
                })
                .await?;
                let database = self.database.clone();
                blocking("clear the Discogs key", move || {
                    database.delete_host_secret(crate::keys::DISCOGS_API_KEY)
                })
                .await
                .map_err(|failed| self.bug(failed))??;
                Ok(())
            })
            .await
    }

    pub async fn set_discogs_validation(
        &self,
        validation: DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.update_preferences(move |prefs| {
            if prefs.discogs.is_some() {
                prefs.discogs = Some(validation);
            }
            Ok(())
        })
        .await
    }

    pub async fn discogs_validation(
        &self,
    ) -> Result<Option<DiscogsValidation>, crate::db::DbError> {
        Ok(self.preferences().await?.discogs)
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_release_payloads(
        &self,
        release: &crate::catalog_model::records::LookupRef,
        depth: crate::catalog_model::payloads::ReadDepth,
        priority: CallPriority,
    ) -> Result<crate::catalog_model::payloads::ReleasePayloads, crate::import::ImportError> {
        Ok(self
            .catalogs
            .release_read_session(release)
            .await?
            .fetch_payloads(release, depth, priority)
            .await?)
    }

    /// A release a Disc ID lookup returned, read for an identify run from
    /// that answer (see [`crate::catalog_model::payloads::read_disc_id_release`]).
    #[cfg(desktop)]
    pub(crate) async fn read_disc_id_release_payloads(
        &self,
        answered: &crate::catalog_model::payloads::DiscIdRelease,
        priority: CallPriority,
    ) -> Result<crate::catalog_model::payloads::ReleasePayloads, crate::import::ImportError> {
        Ok(self
            .catalogs
            .release_read_session(answered.release())
            .await?
            .read_disc_id_release(answered, priority)
            .await?)
    }

    /// Keep what release groups were read to be beyond the list that read
    /// them, so a stored release of either album names the other among its
    /// records. Keeping it failing leaves the list's answer standing, and is
    /// logged.
    #[cfg(desktop)]
    pub(crate) async fn keep_album_links(
        &self,
        kept: Vec<(String, Vec<crate::catalog_model::records::MetadataRef>)>,
    ) {
        if let Err(error) = self.database.replace_group_album_links(kept).await {
            tracing::error!("What the release groups were read to be was not kept: {error}");
        }
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_discogs_release_covers(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        Ok(self.catalogs.session().await?.release_covers(release_id, priority).await?)
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_discogs_master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        Ok(self.catalogs.session().await?.master_covers(master_id, priority).await?)
    }

    /// Discogs' answer for the picture of each artist `ids` names, asked now,
    /// one artist at a time, each picture downscaled to the JPEG bae stores.
    /// A provider or download failure, or a picture that does not decode,
    /// fails the caller with it.
    #[cfg(desktop)]
    pub(crate) async fn fetch_discogs_artist_images(
        &self,
        ids: std::collections::BTreeSet<String>,
    ) -> Result<Vec<crate::import::DiscogsArtistImage>, crate::import::ImportError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let session = self.catalogs.session().await?;
        let mut answers = Vec::with_capacity(ids.len());
        for discogs_artist_id in ids {
            let Some(source_url) = session.artist_image_url(&discogs_artist_id).await? else {
                answers.push(crate::import::DiscogsArtistImage::Nothing { discogs_artist_id });
                continue;
            };
            let Some(image) = self.fetch_remote_image(&source_url).await? else {
                answers.push(crate::import::DiscogsArtistImage::Nothing { discogs_artist_id });
                continue;
            };
            // Decoding and re-encoding the image is CPU work.
            let jpeg = blocking("resize an artist picture", move || {
                bae_util::stored_image::stored_jpeg(&image.bytes)
            })
            .await
            .map_err(|failed| self.bug(failed))?
            .map_err(|detail| crate::import::ImportError::CoverArt { detail })?;
            answers.push(crate::import::DiscogsArtistImage::Image { discogs_artist_id, jpeg });
        }
        Ok(answers)
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

    /// Check an unvalidated key again. A check Discogs could not answer
    /// leaves the key unvalidated and fails with why, so the person who asked
    /// is told rather than shown the same state again.
    #[cfg(desktop)]
    pub(crate) async fn revalidate_discogs_token(&self) -> Result<(), crate::import::ImportError> {
        if self.discogs_validation().await.map_err(LibraryError::from)?
            != Some(DiscogsValidation::Unvalidated)
        {
            return Ok(());
        }
        let check = self.catalogs.session().await?.validate().await.ok_or_else(|| {
            LibraryError::Internal(
                "config says a Discogs key is stored but the keyring has none".to_string(),
            )
        })?;
        self.set_discogs_validation(DiscogsValidation::of(&check))
            .await
            .map_err(LibraryError::from)?;
        match check {
            crate::catalog_model::providers::DiscogsCheck::Valid
            | crate::catalog_model::providers::DiscogsCheck::Rejected => Ok(()),
            crate::catalog_model::providers::DiscogsCheck::Unchecked(error) => {
                Err(crate::catalog_model::CatalogError::from(error).into())
            }
        }
    }

    #[cfg(all(test, desktop))]
    pub(super) async fn discogs_available_for_test(&self) -> Result<bool, LibraryError> {
        Ok(self.catalogs.session().await?.asks_discogs())
    }

    #[cfg(all(test, desktop))]
    pub(super) async fn record_discogs_validation_for_test(&self, signal: DiscogsKeySignal) {
        self.catalogs.record_discogs_validation_for_test(signal).await;
    }

    /// Whether a release on `catalog` can be fetched now: MusicBrainz always
    /// can, and Discogs can when this library holds a key it may ask with.
    #[cfg(desktop)]
    pub(crate) async fn can_fetch_releases_from(
        &self,
        catalog: bae_catalogs::catalog::LookupCatalog,
    ) -> Result<bool, LibraryError> {
        match catalog {
            bae_catalogs::catalog::LookupCatalog::MusicBrainz => Ok(true),
            bae_catalogs::catalog::LookupCatalog::Discogs => {
                Ok(self.catalogs.session().await?.asks_discogs())
            }
        }
    }
}
