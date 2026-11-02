//! Discogs operations owned by [`LibraryManager`]: the stored key, and the
//! reads that ask Discogs through the library's
//! [`crate::import::CatalogSessions`].

use super::*;
use crate::config::DiscogsValidation;
#[cfg(all(test, desktop))]
use bae_catalogs::discogs::client::DiscogsKeySignal;
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
}
