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
    fn open(config_handle: &Arc<ConfigHandle>, database: &Database) -> Result<Self, LibraryError> {
        let validation = config_handle.config().discogs;
        if matches!(validation, None | Some(DiscogsValidation::Rejected)) {
            return Ok(Self { client: None });
        }

        let config_handle = Arc::clone(config_handle);
        let observer = Arc::new(move |signal| {
            Self::record_validation_signal(&config_handle, signal);
        });
        let client = database
            .host_secret(crate::keys::DISCOGS_API_KEY)?
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
    async fn release_covers(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        let Some(client) = self.client.as_ref() else {
            return Err(crate::import::ImportError::DiscogsNotConfigured);
        };
        let (release, _) = client.get_release(release_id, priority).await?;
        let mut covers = release.covers;
        if let Some(master_id) = release.master_id {
            let (_, json) = client.get_master(&master_id, priority).await?;
            for cover in crate::discogs::client::parse_discogs_master_covers(&json)? {
                crate::import::cover_art::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
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
    pub(crate) fn discogs_is_usable(&self) -> bool {
        self.config_handle
            .config()
            .discogs_token_status()
            .is_usable()
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, LibraryError> {
        Ok(self.database.host_secret(crate::keys::DISCOGS_API_KEY)?)
    }

    /// Store the keyring bytes before recording the config state. A failure
    /// between those writes leaves Discogs disabled until the caller retries;
    /// config never claims a key that the keyring lacks.
    pub fn set_discogs_key(
        &self,
        token: &str,
        validation: DiscogsValidation,
    ) -> Result<(), LibraryError> {
        self.database
            .set_host_secret(crate::keys::DISCOGS_API_KEY, token)?;
        self.config_handle
            .update(|config| config.discogs = Some(validation))?;
        Ok(())
    }

    /// Clear the config state before deleting the keyring bytes, so a failure
    /// between the writes leaves Discogs disabled rather than half-enabled.
    pub fn clear_discogs_key(&self) -> Result<(), LibraryError> {
        self.config_handle.update(|config| config.discogs = None)?;
        self.database
            .delete_host_secret(crate::keys::DISCOGS_API_KEY)?;
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
        DiscogsSession::open(&self.config_handle, &self.database)?
            .search(params, priority)
            .await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_release_payloads(
        &self,
        release: &crate::import::MetadataRef,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        match DiscogsSession::open(&self.config_handle, &self.database) {
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
    pub(crate) async fn fetch_discogs_release_covers(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        DiscogsSession::open(&self.config_handle, &self.database)?
            .release_covers(release_id, priority)
            .await
    }

    /// Resolve every Discogs artist image answer referenced by a candidate
    /// draft before that draft is committed. Provider and download failures
    /// fail the caller, leaving the prior candidate revision intact.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn prepare_discogs_artist_images(
        &self,
        ids: std::collections::BTreeSet<String>,
    ) -> Result<Vec<crate::import::PreparedArtistImage>, crate::import::ImportError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let session = DiscogsSession::open(&self.config_handle, &self.database)?;
        let mut answers = Vec::with_capacity(ids.len());
        for discogs_artist_id in ids {
            let Some(source_url) = session.artist_image_url(&discogs_artist_id).await? else {
                answers.push(crate::import::PreparedArtistImage::Nothing { discogs_artist_id });
                continue;
            };
            let Some(image) = self.fetch_remote_image(&source_url).await? else {
                answers.push(crate::import::PreparedArtistImage::Nothing { discogs_artist_id });
                continue;
            };
            answers.push(crate::import::PreparedArtistImage::Image {
                discogs_artist_id,
                source_url,
                image,
            });
        }
        Ok(answers)
    }

    /// Turn candidate-owned image bytes into library image rows after artist
    /// identities have been resolved. Existing library images win; this does
    /// database reads and row construction only.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn materialize_prepared_artist_images(
        &self,
        inserted_artists: &[DbArtist],
        prepared: &[crate::import::PreparedArtistImage],
    ) -> Result<Vec<(DbLibraryImage, Vec<u8>)>, crate::import::ImportError> {
        let by_discogs_id: HashMap<_, _> = prepared
            .iter()
            .map(|answer| (answer.discogs_artist_id(), answer))
            .collect();
        if by_discogs_id.len() != prepared.len() {
            return Err(crate::import::ImportError::Internal {
                detail: "prepared artist images contain a duplicate Discogs artist ID".into(),
            });
        }
        let mut images = Vec::new();
        for artist in inserted_artists {
            let Some(discogs_artist_id) = artist.discogs_artist_id.as_deref() else {
                continue;
            };
            let answer = by_discogs_id.get(discogs_artist_id).ok_or_else(|| {
                crate::import::ImportError::Internal {
                    detail: format!(
                        "new Discogs artist {discogs_artist_id} has no prepared image answer"
                    ),
                }
            })?;
            let crate::import::PreparedArtistImage::Image {
                source_url, image, ..
            } = answer
            else {
                continue;
            };
            let row = DbLibraryImage {
                id: artist.id.clone(),
                blob_id: self.new_id(),
                image_type: LibraryImageType::Artist,
                content_type: image.content_type.clone(),
                file_size: image.bytes.len() as i64,
                width: None,
                height: None,
                source: crate::import::MetadataSource::Discogs.as_str().to_string(),
                source_url: Some(source_url.clone()),
                cloud_path: None,
                content_hash: crate::util::fs::hash_bytes(&image.bytes),
                created_at: self.now(),
            };
            images.push((row, image.bytes.clone()));
        }
        Ok(images)
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn revalidate_discogs_token(&self) -> Result<(), LibraryError> {
        if self.discogs_validation() != Some(DiscogsValidation::Unvalidated) {
            return Ok(());
        }
        let validation = DiscogsSession::open(&self.config_handle, &self.database)?
            .validate()
            .await?;
        self.set_discogs_validation(validation)?;
        Ok(())
    }

    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    pub(super) fn discogs_available_for_test(&self) -> Result<bool, LibraryError> {
        Ok(DiscogsSession::open(&self.config_handle, &self.database)?
            .client
            .is_some())
    }

    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    pub(super) fn record_discogs_validation_for_test(&self, signal: DiscogsKeySignal) {
        DiscogsSession::record_validation_signal(&self.config_handle, signal);
    }
}
