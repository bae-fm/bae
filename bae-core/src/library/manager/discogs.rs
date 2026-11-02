//! Discogs operations owned by [`LibraryManager`].
//!
//! The client for the stored key, with its validation callback, is built here
//! and handed out inside a catalog session; no caller receives the database
//! the callback writes to.

use super::*;
#[cfg(desktop)]
use crate::catalog_model::providers::CatalogSession;
use crate::config::DiscogsValidation;
#[cfg(desktop)]
use bae_catalogs::discogs::client::{DiscogsError, DiscogsKeySignal};
#[cfg(desktop)]
use bae_catalogs::rate_limiter::CallPriority;
use std::collections::HashMap;

/// Fold one Discogs call's outcome into the stored key's validation: a 401
/// rejects it, and a success confirms a key nothing had confirmed yet.
/// Decided in the write against the state it finds, so a signal racing a key
/// change applies to the key it finds. A failure to write it is logged; the
/// next call's outcome is recorded again.
#[cfg(desktop)]
async fn record_discogs_validation_signal(database: &Database, signal: DiscogsKeySignal) {
    let recorded = database
        .edit_preferences(move |prefs| {
            let next = match (prefs.discogs, &signal) {
                (None, _) => None,
                (Some(_), DiscogsKeySignal::Rejected) => Some(DiscogsValidation::Rejected),
                (Some(DiscogsValidation::Unvalidated), DiscogsKeySignal::Accepted) => {
                    Some(DiscogsValidation::Valid)
                }
                (Some(_), DiscogsKeySignal::Accepted) => None,
            };
            match next {
                Some(next) => prefs.discogs = Some(next),
                None => debug!("discogs validation signal changes nothing"),
            }
            Ok(())
        })
        .await;
    if let Err(error) = recorded {
        warn!("failed to record the Discogs key's validation: {error}");
    }
}

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
        // Held across the writes, so no call builds a client from a key
        // halfway through being replaced.
        let mut client = self.discogs_client.lock().await;
        let database = self.database.clone();
        let token = token.to_string();
        blocking("store the Discogs key", move || {
            database.set_host_secret(crate::keys::DISCOGS_API_KEY, &token)
        })
        .await
        .map_err(|failed| self.bug(failed))??;
        // The cached client asks with the key just replaced.
        *client = None;
        self.update_preferences(move |prefs| {
            prefs.discogs = Some(validation);
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Clear the key's state before deleting the keyring bytes, so a failure
    /// between the writes leaves Discogs disabled rather than half-enabled.
    pub async fn clear_discogs_key(&self) -> Result<(), LibraryError> {
        let mut client = self.discogs_client.lock().await;
        self.update_preferences(|prefs| {
            prefs.discogs = None;
            Ok(())
        })
        .await?;
        *client = None;
        let database = self.database.clone();
        blocking("clear the Discogs key", move || {
            database.delete_host_secret(crate::keys::DISCOGS_API_KEY)
        })
        .await
        .map_err(|failed| self.bug(failed))??;
        Ok(())
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
            .release_read_session(answered.release())
            .await?
            .read_disc_id_release(answered, priority)
            .await?)
    }

    /// The session a search of `catalog` asks with, holding only what that
    /// catalog needs: MusicBrainz needs no key, so its session reads neither
    /// the preferences nor the keyring.
    #[cfg(desktop)]
    pub(crate) async fn search_session(
        &self,
        catalog: bae_catalogs::catalog::LookupCatalog,
    ) -> Result<CatalogSession, LibraryError> {
        match catalog {
            bae_catalogs::catalog::LookupCatalog::MusicBrainz => {
                Ok(CatalogSession::new(None, self.providers.clone()))
            }
            bae_catalogs::catalog::LookupCatalog::Discogs => self.catalog_session().await,
        }
    }

    /// The Discogs session a read of `release` asks with. A MusicBrainz
    /// release whose session cannot be had is read without its Discogs
    /// cross-reference, as it is without a key.
    #[cfg(desktop)]
    async fn release_read_session(
        &self,
        release: &crate::catalog_model::records::LookupRef,
    ) -> Result<CatalogSession, crate::import::ImportError> {
        match self.catalog_session().await {
            Ok(session) => Ok(session),
            Err(error) if release.catalog == bae_catalogs::catalog::LookupCatalog::MusicBrainz => {
                warn!(
                    release_id = %release.key,
                    "Discogs cross-reference unavailable while fetching MusicBrainz release: {error}"
                );
                Ok(CatalogSession::new(None, self.providers.clone()))
            }
            Err(error) => Err(error.into()),
        }
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
        Ok(self.catalog_session().await?.release_covers(release_id, priority).await?)
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_discogs_master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        Ok(self.catalog_session().await?.master_covers(master_id, priority).await?)
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
        let session = self.catalog_session().await?;
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
        let check = self.catalog_session().await?.validate().await.ok_or_else(|| {
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
        Ok(self.catalog_session().await?.asks_discogs())
    }

    #[cfg(all(test, desktop))]
    pub(super) async fn record_discogs_validation_for_test(&self, signal: DiscogsKeySignal) {
        record_discogs_validation_signal(&self.database, signal).await;
    }

    /// Discogs as this library may ask it right now. No client while no key is
    /// stored or the stored one was rejected. Otherwise the one client for the
    /// stored key, built the first time it is needed — reading the key off the
    /// keyring once rather than on every call — and reused until the key is
    /// set or cleared.
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
                Ok(self.catalog_session().await?.asks_discogs())
            }
        }
    }

    #[cfg(desktop)]
    pub(crate) async fn catalog_session(&self) -> Result<CatalogSession, LibraryError> {
        let providers = self.providers.clone();
        // Held while the stored key is read, so a key being set or cleared is
        // seen whole or not at all.
        let mut cached = self.discogs_client.lock().await;
        let validation = self.preferences().await?.discogs;
        if matches!(validation, None | Some(DiscogsValidation::Rejected)) {
            return Ok(CatalogSession::new(None, providers));
        }
        if cached.is_none() {
            let database = self.database.clone();
            let diagnostics = self.diagnostics.clone();
            let runtime = self.runtime_handle.clone();
            // The client reports from its request path; recording the signal
            // is a write, so it runs as its own task, and a failure to write
            // is logged there. Nothing waits on it, so a panic in it is
            // reported here.
            let observer = Arc::new(move |signal| {
                let database = database.clone();
                let diagnostics = diagnostics.clone();
                runtime.spawn(async move {
                    let recorded = caught(
                        "record a Discogs validation signal",
                        record_discogs_validation_signal(&database, signal),
                    )
                    .await;
                    if let Err(failed) = recorded {
                        diagnostics.bug(failed);
                    }
                });
            });
            *cached = self
                .database
                .host_secret(crate::keys::DISCOGS_API_KEY)?
                .map(|key| Arc::new(providers.discogs_client(key, Some(observer))));
        }
        Ok(CatalogSession::new(cached.clone(), providers))
    }

    /// Ask Discogs whether it accepts `key`, which this library does not
    /// store: how a key is tried before it is saved.
    #[cfg(desktop)]
    pub(crate) async fn try_discogs_key(
        &self,
        key: &str,
        priority: CallPriority,
    ) -> Result<(), DiscogsError> {
        self.providers.discogs_client(key.to_string(), None).validate_token(priority).await
    }
}
