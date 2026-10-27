//! Discogs operations owned by [`LibraryManager`].
//!
//! The client for the stored key and its validation callback stay inside a
//! [`DiscogsSession`]. Callers ask the manager for search results, payloads,
//! covers, or images; no caller receives the client or the config handle the
//! callback updates.

use super::*;
use crate::config::DiscogsValidation;
#[cfg(desktop)]
use bae_catalogs::discogs::client::{
    DiscogsClient, DiscogsError, DiscogsKeySignal, DiscogsSearchParams,
};
#[cfg(desktop)]
use bae_catalogs::rate_limiter::CallPriority;

/// Nothing panics while the stored key's client is locked, so a poisoned lock
/// is a bug and fails loudly.
#[cfg(desktop)]
const DISCOGS_CLIENT_LOCK: &str = "the Discogs client lock is never held across a panic";

/// One operation's view of Discogs: the client for the stored key, when there
/// is one Discogs may be asked with, and the other providers a release fetch
/// follows links into.
#[cfg(desktop)]
struct DiscogsSession {
    client: Option<Arc<DiscogsClient>>,
    providers: crate::providers::Providers,
}

#[cfg(desktop)]
impl DiscogsSession {
    #[cfg(desktop)]
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

    #[cfg(desktop)]
    async fn fetch_payloads(
        &self,
        release: &crate::import::MetadataRef,
        depth: crate::import::payloads::ReadDepth,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        self.providers
            .fetch_payloads(self.client.as_deref(), release, depth, priority)
            .await
    }

    #[cfg(desktop)]
    async fn read_disc_id_release(
        &self,
        answered: &crate::import::payloads::DiscIdRelease,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        self.providers
            .read_disc_id_release(self.client.as_deref(), answered, priority)
            .await
    }

    #[cfg(desktop)]
    async fn release_covers(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        let Some(client) = self.client.as_ref() else {
            return Err(crate::import::ImportError::DiscogsNotConfigured);
        };
        let (release, _) = client.get_release(release_id, priority).await?;
        let mut covers = release.covers;
        if let Some(master_id) = release.master_id {
            let (_, json) = client.get_master(&master_id, priority).await?;
            for cover in bae_catalogs::discogs::client::parse_discogs_master_covers(&json)? {
                bae_catalogs::remote_image::push_unique_cover(&mut covers, cover);
            }
        }
        Ok(covers)
    }

    #[cfg(desktop)]
    async fn master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        let client = self
            .client
            .as_ref()
            .ok_or(crate::import::ImportError::DiscogsNotConfigured)?;
        let (master, _) = client.get_master(master_id, priority).await?;
        Ok(master.covers)
    }

    #[cfg(desktop)]
    async fn artist_image_url(
        &self,
        artist_id: &str,
    ) -> Result<Option<String>, crate::import::ImportError> {
        let Some(client) = self.client.as_ref() else {
            warn!(
                discogs_artist_id = %artist_id,
                "Discogs has no usable key to ask for this artist's picture, so the artist gets none"
            );
            return Ok(None);
        };
        Ok(client
            .get_artist_image(artist_id, CallPriority::Interactive)
            .await?)
    }

    #[cfg(desktop)]
    async fn validate(&self) -> Result<DiscogsCheck, LibraryError> {
        let client = self.client.as_ref().ok_or_else(|| {
            LibraryError::Internal(
                "config says a Discogs key is stored but the keyring has none".to_string(),
            )
        })?;
        Ok(DiscogsCheck::of(
            client.validate_token(CallPriority::Interactive).await,
        ))
    }
}

/// Fold one Discogs call's outcome into the stored key's validation: a 401
/// rejects it, and a success confirms a key nothing had confirmed yet.
/// Blocking — it persists the change — so the client's observer runs it on a
/// blocking thread.
#[cfg(desktop)]
fn record_discogs_validation_signal(config_handle: &ConfigHandle, signal: DiscogsKeySignal) {
    let next_of = |current: Option<DiscogsValidation>| match (current, &signal) {
        (None, _) => None,
        (Some(_), DiscogsKeySignal::Rejected) => Some(DiscogsValidation::Rejected),
        (Some(DiscogsValidation::Unvalidated), DiscogsKeySignal::Accepted) => {
            Some(DiscogsValidation::Valid)
        }
        (Some(_), DiscogsKeySignal::Accepted) => None,
    };
    let current = config_handle.config().prefs.discogs;
    let Some(next) = next_of(current) else {
        debug!("discogs validation signal changes nothing");
        return;
    };
    if current == Some(next) {
        return;
    }
    // Re-decided under the writer lock against the value it edits, so a
    // signal racing a key change applies to the key it finds.
    if let Err(error) = config_handle.update_preferences_now(|prefs| {
        if let Some(next) = next_of(prefs.discogs) {
            prefs.discogs = Some(next);
        }
    }) {
        warn!("failed to persist discogs validation {next:?}: {error}");
    }
}

/// What a token-validation request proves about a key: Discogs accepted it,
/// rejected it, or could not be asked — a failure that says nothing about the
/// key, kept so the person is told why the key went unchecked.
#[cfg(desktop)]
#[derive(Debug)]
pub(crate) enum DiscogsCheck {
    Valid,
    Rejected,
    Unchecked(DiscogsError),
}

#[cfg(desktop)]
impl DiscogsCheck {
    /// Read a token-validation request's outcome. A failure that leaves the
    /// key unchecked is warned with its cause.
    pub(crate) fn of(result: Result<(), DiscogsError>) -> Self {
        match result {
            Ok(()) => Self::Valid,
            Err(DiscogsError::InvalidApiKey) => Self::Rejected,
            Err(
                error @ (DiscogsError::Transport(_)
                | DiscogsError::Provider { .. }
                | DiscogsError::NotFound
                | DiscogsError::Serialization(_)),
            ) => {
                warn!("Discogs couldn't check the key, which stays unvalidated: {error}");
                Self::Unchecked(error)
            }
        }
    }

    /// The validation the key is stored under.
    pub(crate) fn validation(&self) -> DiscogsValidation {
        match self {
            Self::Valid => DiscogsValidation::Valid,
            Self::Rejected => DiscogsValidation::Rejected,
            Self::Unchecked(_) => DiscogsValidation::Unvalidated,
        }
    }
}

impl LibraryManager {
    pub async fn get_discogs_token(&self) -> Result<Option<String>, LibraryError> {
        self.host_secret(crate::keys::DISCOGS_API_KEY).await
    }

    /// Store the keyring bytes before recording the config state. A failure
    /// between those writes leaves Discogs disabled until the caller retries;
    /// config never claims a key that the keyring lacks. Both writes block,
    /// so they run on a blocking thread.
    pub async fn set_discogs_key(
        &self,
        token: &str,
        validation: DiscogsValidation,
    ) -> Result<(), LibraryError> {
        let database = self.database.clone();
        let config_handle = Arc::clone(&self.config_handle);
        #[cfg(desktop)]
        let discogs_client = Arc::clone(&self.discogs_client);
        let token = token.to_string();
        super::local_files::blocking_io(move || {
            // Held across the writes, so no call builds a client from a key
            // halfway through being replaced.
            #[cfg(desktop)]
            let mut client = discogs_client.lock().expect(DISCOGS_CLIENT_LOCK);
            database.set_host_secret(crate::keys::DISCOGS_API_KEY, &token)?;
            // The cached client asks with the key just replaced.
            #[cfg(desktop)]
            {
                *client = None;
            }
            config_handle.update_preferences_now(|prefs| prefs.discogs = Some(validation))?;
            Ok(())
        })
        .await
    }

    /// Clear the config state before deleting the keyring bytes, so a failure
    /// between the writes leaves Discogs disabled rather than half-enabled.
    pub async fn clear_discogs_key(&self) -> Result<(), LibraryError> {
        let database = self.database.clone();
        let config_handle = Arc::clone(&self.config_handle);
        #[cfg(desktop)]
        let discogs_client = Arc::clone(&self.discogs_client);
        super::local_files::blocking_io(move || {
            #[cfg(desktop)]
            let mut client = discogs_client.lock().expect(DISCOGS_CLIENT_LOCK);
            config_handle.update_preferences_now(|prefs| prefs.discogs = None)?;
            #[cfg(desktop)]
            {
                *client = None;
            }
            #[cfg(desktop)]
            database.delete_host_secret(crate::keys::DISCOGS_API_KEY)?;
            Ok(())
        })
        .await
    }

    pub async fn set_discogs_validation(
        &self,
        validation: DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update_preferences(move |prefs| {
                if prefs.discogs.is_some() {
                    prefs.discogs = Some(validation);
                }
            })
            .await
    }

    pub fn discogs_validation(&self) -> Option<DiscogsValidation> {
        self.config_handle.config().prefs.discogs
    }

    #[cfg(desktop)]
    pub(crate) async fn search_discogs(
        &self,
        params: DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::search::MetadataResult>, crate::import::ImportError> {
        self.discogs_session()?.search(params, priority).await
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_release_payloads(
        &self,
        release: &crate::import::MetadataRef,
        depth: crate::import::payloads::ReadDepth,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        self.release_read_session(release)?
            .fetch_payloads(release, depth, priority)
            .await
    }

    /// A release a Disc ID lookup returned, read for an identify run from
    /// that answer (see [`crate::import::payloads::read_disc_id_release`]).
    #[cfg(desktop)]
    pub(crate) async fn read_disc_id_release_payloads(
        &self,
        answered: &crate::import::payloads::DiscIdRelease,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        self.release_read_session(answered.release())?
            .read_disc_id_release(answered, priority)
            .await
    }

    /// The Discogs session a read of `release` asks with. A MusicBrainz
    /// release whose session cannot be had is read without its Discogs
    /// cross-reference, as it is without a key.
    #[cfg(desktop)]
    fn release_read_session(
        &self,
        release: &crate::import::MetadataRef,
    ) -> Result<DiscogsSession, crate::import::ImportError> {
        match self.discogs_session() {
            Ok(session) => Ok(session),
            Err(error) if release.catalog == bae_catalogs::catalog::Catalog::MusicBrainz => {
                warn!(
                    release_id = %release.key,
                    "Discogs cross-reference unavailable while fetching MusicBrainz release: {error}"
                );
                Ok(DiscogsSession {
                    client: None,
                    providers: self.providers.clone(),
                })
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
        kept: Vec<(String, Vec<crate::import::album_links::AlbumLink>)>,
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
        self.discogs_session()?
            .release_covers(release_id, priority)
            .await
    }

    #[cfg(desktop)]
    pub(crate) async fn fetch_discogs_master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<bae_catalogs::remote_image::RemoteCover>, crate::import::ImportError> {
        self.discogs_session()?
            .master_covers(master_id, priority)
            .await
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
        let session = self.discogs_session()?;
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
            let jpeg = tokio::task::spawn_blocking(move || {
                bae_util::stored_image::stored_jpeg(&image.bytes)
            })
            .await
            .map_err(|error| crate::import::ImportError::Internal {
                detail: format!("artist picture resize task failed: {error}"),
            })?
            .map_err(|detail| crate::import::ImportError::CoverArt { detail })?;
            answers.push(crate::import::DiscogsArtistImage::Image {
                discogs_artist_id,
                source_url,
                jpeg,
            });
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
        let by_discogs_id: HashMap<_, _> = fetched
            .iter()
            .map(|answer| (answer.discogs_artist_id(), answer))
            .collect();
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
                return Err(crate::db::ArtistWriteError::LibraryChanged.into());
            };
            let crate::import::DiscogsArtistImage::Image {
                source_url, jpeg, ..
            } = answer
            else {
                continue;
            };
            let row = DbLibraryImage::stored_jpeg(
                LibraryImageType::Artist,
                &artist.id,
                bae_catalogs::catalog::Catalog::Discogs.as_str(),
                Some(source_url.clone()),
                jpeg,
                self.now(),
                self.new_id(),
            );
            images.push((row, jpeg.clone()));
        }
        Ok(images)
    }

    /// Check an unvalidated key again. A check Discogs could not answer
    /// leaves the key unvalidated and fails with why, so the person who asked
    /// is told rather than shown the same state again.
    #[cfg(desktop)]
    pub(crate) async fn revalidate_discogs_token(&self) -> Result<(), crate::import::ImportError> {
        if self.discogs_validation() != Some(DiscogsValidation::Unvalidated) {
            return Ok(());
        }
        let check = self.discogs_session()?.validate().await?;
        self.set_discogs_validation(check.validation())
            .await
            .map_err(LibraryError::from)?;
        match check {
            DiscogsCheck::Valid | DiscogsCheck::Rejected => Ok(()),
            DiscogsCheck::Unchecked(error) => Err(error.into()),
        }
    }

    #[cfg(all(test, desktop))]
    pub(super) fn discogs_available_for_test(&self) -> Result<bool, LibraryError> {
        Ok(self.discogs_session()?.client.is_some())
    }

    #[cfg(all(test, desktop))]
    pub(super) async fn record_discogs_validation_for_test(&self, signal: DiscogsKeySignal) {
        let config_handle = Arc::clone(&self.config_handle);
        super::local_files::blocking_io(move || {
            record_discogs_validation_signal(&config_handle, signal);
            Ok(())
        })
        .await
        .expect("recording a validation signal does not fail");
    }

    /// Discogs as this library may ask it right now. No client while no key is
    /// stored or the stored one was rejected. Otherwise the one client for the
    /// stored key, built the first time it is needed — reading the key off the
    /// keyring once rather than on every call — and reused until the key is
    /// set or cleared.
    /// Whether a release on `catalog` can be fetched now: MusicBrainz always
    /// can, and Discogs can when this library holds a key it may ask with.
    #[cfg(desktop)]
    pub(crate) fn can_fetch_releases_from(
        &self,
        catalog: bae_catalogs::catalog::Catalog,
    ) -> Result<bool, LibraryError> {
        match catalog {
            bae_catalogs::catalog::Catalog::MusicBrainz => Ok(true),
            bae_catalogs::catalog::Catalog::Discogs => Ok(self.discogs_session()?.client.is_some()),
            other => unreachable!("nothing fetches releases from {}", other.as_str()),
        }
    }

    #[cfg(desktop)]
    fn discogs_session(&self) -> Result<DiscogsSession, LibraryError> {
        let providers = self.providers.clone();
        // Held while the stored key is read, so a key being set or cleared is
        // seen whole or not at all.
        let mut cached = self.discogs_client.lock().expect(DISCOGS_CLIENT_LOCK);
        let validation = self.config_handle.config().prefs.discogs;
        if matches!(validation, None | Some(DiscogsValidation::Rejected)) {
            return Ok(DiscogsSession {
                client: None,
                providers,
            });
        }
        if cached.is_none() {
            let config_handle = Arc::clone(&self.config_handle);
            // The client reports from its request path; persisting the
            // signal is a file write, so it goes to a blocking thread, and a
            // failure to persist is logged there.
            let observer = Arc::new(move |signal| {
                let config_handle = Arc::clone(&config_handle);
                tokio::task::spawn_blocking(move || {
                    record_discogs_validation_signal(&config_handle, signal);
                });
            });
            *cached = self
                .database
                .host_secret(crate::keys::DISCOGS_API_KEY)?
                .map(|key| Arc::new(providers.discogs_client(key, Some(observer))));
        }
        Ok(DiscogsSession {
            client: cached.clone(),
            providers,
        })
    }

    /// Ask Discogs whether it accepts `key`, which this library does not
    /// store: how a key is tried before it is saved.
    #[cfg(desktop)]
    pub(crate) async fn try_discogs_key(
        &self,
        key: &str,
        priority: CallPriority,
    ) -> Result<(), DiscogsError> {
        self.providers
            .discogs_client(key.to_string(), None)
            .validate_token(priority)
            .await
    }
}
