//! Discogs operations owned by [`LibraryManager`].
//!
//! The configured client and its validation callback stay inside a
//! [`DiscogsSession`]. Callers ask the manager for search results, payloads,
//! covers, or images; no caller receives the client or the config handle the
//! callback updates.

use super::*;
use crate::config::DiscogsValidation;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::discogs::client::{DiscogsClient, DiscogsError, DiscogsKeySignal, DiscogsSearchParams};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::util::rate_limiter::CallPriority;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct DiscogsSession {
    client: Option<DiscogsClient>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl DiscogsSession {
    fn open(
        config_handle: &Arc<ConfigHandle>,
        key_service: &StoreKeys,
    ) -> Result<Self, LibraryError> {
        let validation = config_handle.config().discogs;
        if matches!(validation, None | Some(DiscogsValidation::Rejected)) {
            return Ok(Self { client: None });
        }

        let config_handle = Arc::clone(config_handle);
        let observer = Arc::new(move |signal| {
            Self::record_validation_signal(&config_handle, signal);
        });
        let client = key_service
            .get_discogs_key()?
            .map(|key| DiscogsClient::with_observer(key, observer));
        Ok(Self { client })
    }

    fn record_validation_signal(config_handle: &ConfigHandle, signal: DiscogsKeySignal) {
        let Some(current) = config_handle.config().discogs else {
            debug!("discogs validation signal ignored: no key stored");
            return;
        };
        let next = match signal {
            DiscogsKeySignal::Rejected => DiscogsValidation::Rejected,
            DiscogsKeySignal::Accepted if current == DiscogsValidation::Unvalidated => {
                DiscogsValidation::Valid
            }
            _ => return,
        };
        if current == next {
            return;
        }
        if let Err(error) = config_handle.update(|config| config.discogs = Some(next)) {
            warn!("failed to persist discogs validation {next:?}: {error}");
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn search(
        &self,
        params: DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::search::MetadataResult>, crate::import::ImportError> {
        let client = self
            .client
            .as_ref()
            .ok_or(crate::import::ImportError::DiscogsNotConfigured)?;
        Ok(crate::import::search::search_discogs(client, params, priority).await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn fetch_payloads(
        &self,
        release: &crate::import::MetadataRef,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        crate::import::payloads::fetch(self.client.as_ref(), release, priority).await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn release_cover(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Option<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        let Some(client) = self.client.as_ref() else {
            return Ok(None);
        };
        let (release, _) = client.get_release(release_id, priority).await?;
        Ok(release.remote_cover())
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn artist_image_url(
        &self,
        artist_id: &str,
    ) -> Result<Option<String>, crate::import::ImportError> {
        let Some(client) = self.client.as_ref() else {
            return Ok(None);
        };
        Ok(client
            .get_artist_image(artist_id, CallPriority::Interactive)
            .await?)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn validate(&self) -> Result<DiscogsValidation, LibraryError> {
        let client = self.client.as_ref().ok_or_else(|| {
            LibraryError::Internal(
                "config says a Discogs key is stored but the keyring has none".to_string(),
            )
        })?;
        Ok(discogs_validation_from_result(
            client.validate_token(CallPriority::Interactive).await,
        ))
    }
}

/// What a token-validation request proves about a key. Provider failures that
/// say nothing about the key leave it unvalidated.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn discogs_validation_from_result(
    result: Result<(), DiscogsError>,
) -> DiscogsValidation {
    match result {
        Ok(()) => DiscogsValidation::Valid,
        Err(DiscogsError::InvalidApiKey) => DiscogsValidation::Rejected,
        Err(
            error @ (DiscogsError::RateLimit
            | DiscogsError::Transport(_)
            | DiscogsError::Provider(_)
            | DiscogsError::NotFound
            | DiscogsError::Serialization(_)),
        ) => {
            debug!("Discogs validation couldn't confirm the key ({error}); leaving it unvalidated");
            DiscogsValidation::Unvalidated
        }
    }
}

impl LibraryManager {
    pub fn has_discogs_token(&self) -> bool {
        self.config_handle.has_discogs_key()
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, LibraryError> {
        Ok(self.key_service.get_discogs_key()?)
    }

    /// Store the keyring bytes before recording the config state. A failure
    /// between those writes leaves Discogs disabled until the caller retries;
    /// config never claims a key that the keyring lacks.
    pub fn set_discogs_key(
        &self,
        token: &str,
        validation: DiscogsValidation,
    ) -> Result<(), LibraryError> {
        self.key_service.set_discogs_key(token)?;
        self.config_handle
            .update(|config| config.discogs = Some(validation))?;
        Ok(())
    }

    /// Clear the config state before deleting the keyring bytes, so a failure
    /// between the writes leaves Discogs disabled rather than half-enabled.
    pub fn clear_discogs_key(&self) -> Result<(), LibraryError> {
        self.config_handle.update(|config| config.discogs = None)?;
        self.key_service.delete_discogs_key()?;
        Ok(())
    }

    pub fn set_discogs_validation(
        &self,
        validation: DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|config| {
            if config.discogs.is_some() {
                config.discogs = Some(validation);
            }
        })
    }

    pub fn discogs_validation(&self) -> Option<DiscogsValidation> {
        self.config_handle.config().discogs
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn search_discogs(
        &self,
        params: DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::search::MetadataResult>, crate::import::ImportError> {
        DiscogsSession::open(&self.config_handle, &self.key_service)?
            .search(params, priority)
            .await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_release_payloads(
        &self,
        release: &crate::import::MetadataRef,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        match DiscogsSession::open(&self.config_handle, &self.key_service) {
            Ok(session) => session.fetch_payloads(release, priority).await,
            Err(error) if release.source == crate::import::MetadataSource::MusicBrainz => {
                warn!(
                    release_id = %release.id,
                    "Discogs cross-reference unavailable while fetching MusicBrainz release: {error}"
                );
                crate::import::payloads::fetch(None, release, priority).await
            }
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_discogs_release_cover(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Option<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        DiscogsSession::open(&self.config_handle, &self.key_service)?
            .release_cover(release_id, priority)
            .await
    }

    /// Fetch image rows for newly resolved artists. Existing-image checks and
    /// provider/download failures are per-artist skips; each is logged where it
    /// occurs so the import can still commit its metadata.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_discogs_artist_images(
        &self,
        parsed_artists: &[DbArtist],
        artist_id_map: &HashMap<String, String>,
    ) -> Vec<(DbLibraryImage, Vec<u8>)> {
        let session = match DiscogsSession::open(&self.config_handle, &self.key_service) {
            Ok(session) => session,
            Err(error) => {
                warn!("Discogs artist images unavailable: {error}");
                return Vec::new();
            }
        };
        if session.client.is_none() {
            return Vec::new();
        }

        let mut images = Vec::new();
        for parsed_artist in parsed_artists {
            let Some(actual_id) = artist_id_map.get(&parsed_artist.id) else {
                continue;
            };
            let Some(discogs_artist_id) = parsed_artist.discogs_artist_id.as_deref() else {
                continue;
            };

            match self
                .get_library_image(actual_id, &LibraryImageType::Artist)
                .await
            {
                Ok(Some(_)) => continue,
                Ok(None) => {}
                Err(error) => {
                    warn!("failed to check existing artist image for artist {actual_id}: {error}");
                    continue;
                }
            }

            let image_url = match session.artist_image_url(discogs_artist_id).await {
                Ok(Some(url)) => url,
                Ok(None) => {
                    debug!("No image found for Discogs artist {discogs_artist_id}");
                    continue;
                }
                Err(error) => {
                    warn!(
                        "Failed to fetch artist image URL from Discogs for artist {actual_id} (Discogs {discogs_artist_id}): {error}"
                    );
                    continue;
                }
            };

            let (bytes, content_type) = match crate::import::cover_art::download_image_bytes(
                &image_url,
                "Artist image download",
            )
            .await
            {
                Ok(download) => download,
                Err(error) => {
                    warn!(
                            "Failed to download artist image for artist {actual_id} (Discogs {discogs_artist_id}) from {image_url}: {error}"
                        );
                    continue;
                }
            };

            let image = DbLibraryImage {
                id: actual_id.clone(),
                blob_id: self.new_id(),
                image_type: LibraryImageType::Artist,
                content_type,
                file_size: bytes.len() as i64,
                width: None,
                height: None,
                source: crate::import::MetadataSource::Discogs.as_str().to_string(),
                source_url: Some(image_url),
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(&bytes),
                created_at: self.now(),
            };
            debug!(
                "Fetched artist image ({} bytes) for artist {actual_id}",
                bytes.len()
            );
            images.push((image, bytes));
        }
        images
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn revalidate_discogs_token(&self) -> Result<(), LibraryError> {
        if self.discogs_validation() != Some(DiscogsValidation::Unvalidated) {
            return Ok(());
        }
        let validation = DiscogsSession::open(&self.config_handle, &self.key_service)?
            .validate()
            .await?;
        self.set_discogs_validation(validation)?;
        Ok(())
    }

    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    pub(super) fn discogs_available_for_test(&self) -> Result<bool, LibraryError> {
        Ok(
            DiscogsSession::open(&self.config_handle, &self.key_service)?
                .client
                .is_some(),
        )
    }

    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    pub(super) fn record_discogs_validation_for_test(&self, signal: DiscogsKeySignal) {
        DiscogsSession::record_validation_signal(&self.config_handle, signal);
    }
}
