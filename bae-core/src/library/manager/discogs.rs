//! Discogs operations owned by [`LibraryManager`].
//!
//! The client for the stored key and its validation callback stay inside a
//! [`DiscogsSession`]. Callers ask the manager for search results, payloads,
//! covers, or images; no caller receives the client or the config handle the
//! callback updates.

use super::*;
use crate::config::DiscogsValidation;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::discogs::client::{DiscogsClient, DiscogsError, DiscogsKeySignal, DiscogsSearchParams};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::util::rate_limiter::CallPriority;

/// Nothing panics while the stored key's client is locked, so a poisoned lock
/// is a bug and fails loudly.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const DISCOGS_CLIENT_LOCK: &str = "the Discogs client lock is never held across a panic";

/// One operation's view of Discogs: the client for the stored key, when there
/// is one Discogs may be asked with, and the other providers a release fetch
/// follows links into.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct DiscogsSession {
    client: Option<Arc<DiscogsClient>>,
    providers: crate::providers::Providers,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl DiscogsSession {
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
        self.providers
            .fetch_payloads(self.client.as_deref(), release, priority)
            .await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn read_album_links(
        &self,
        to_read: &crate::import::album_links::ToRead,
        priority: CallPriority,
    ) -> Vec<crate::import::album_links::GroupReading<()>> {
        self.providers
            .read_album_links(self.client.as_deref(), to_read, priority)
            .await
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
    async fn master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        let client = self
            .client
            .as_ref()
            .ok_or(crate::import::ImportError::DiscogsNotConfigured)?;
        let (master, _) = client.get_master(master_id, priority).await?;
        Ok(master.covers)
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

/// Fold one Discogs call's outcome into the stored key's validation: a 401
/// rejects it, and a success confirms a key nothing had confirmed yet.
/// Blocking — it persists the change — so the client's observer runs it on a
/// blocking thread.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
            error @ (DiscogsError::RateLimit { .. }
            | DiscogsError::Transport(_)
            | DiscogsError::Provider { .. }
            | DiscogsError::NotFound
            | DiscogsError::Serialization(_)),
        ) => {
            debug!("Discogs validation couldn't confirm the key ({error}); leaving it unvalidated");
            DiscogsValidation::Unvalidated
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
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let discogs_client = Arc::clone(&self.discogs_client);
        let token = token.to_string();
        blocking(move || {
            // Held across the writes, so no call builds a client from a key
            // halfway through being replaced.
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            let mut client = discogs_client.lock().expect(DISCOGS_CLIENT_LOCK);
            database.set_host_secret(crate::keys::DISCOGS_API_KEY, &token)?;
            // The cached client asks with the key just replaced.
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let discogs_client = Arc::clone(&self.discogs_client);
        blocking(move || {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            let mut client = discogs_client.lock().expect(DISCOGS_CLIENT_LOCK);
            config_handle.update_preferences_now(|prefs| prefs.discogs = None)?;
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            {
                *client = None;
            }
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
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

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn search_discogs(
        &self,
        params: DiscogsSearchParams,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::search::MetadataResult>, crate::import::ImportError> {
        self.discogs_session()?.search(params, priority).await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_release_payloads(
        &self,
        release: &crate::import::MetadataRef,
        priority: CallPriority,
    ) -> Result<crate::import::payloads::ReleasePayloads, crate::import::ImportError> {
        match self.discogs_session() {
            Ok(session) => session.fetch_payloads(release, priority).await,
            Err(error) if release.catalog == crate::import::Catalog::MusicBrainz => {
                warn!(
                    release_id = %release.key,
                    "Discogs cross-reference unavailable while fetching MusicBrainz release: {error}"
                );
                self.providers.fetch_payloads(None, release, priority).await
            }
            Err(error) => Err(error.into()),
        }
    }

    /// What each MusicBrainz release group to read is on Discogs, with each
    /// twin the reading read checked against the library the way a lookup's
    /// answers are.
    ///
    /// A library whose Discogs key cannot be read reads without Discogs: a
    /// release link is then followed only to a release already on the list.
    /// A twin whose library status cannot be read leaves its group unread
    /// rather than on the list with a status nothing checked.
    ///
    /// What each group was read to be is kept beyond the list that asked, so
    /// a stored release of either album names the other among its records.
    /// Keeping it failing leaves the list's answer standing, and is logged.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn read_album_links(
        &self,
        to_read: &crate::import::album_links::ToRead,
        priority: CallPriority,
    ) -> Vec<crate::import::album_links::GroupReading> {
        let read = self.read_album_statements(to_read, priority).await;
        let kept: Vec<(String, Vec<crate::import::album_links::AlbumLink>)> = read
            .iter()
            .filter_map(|reading| match &reading.links {
                crate::import::album_links::AlbumLinks::Read(links) => {
                    Some((reading.group.clone(), links.clone()))
                }
                crate::import::album_links::AlbumLinks::NotAsked
                | crate::import::album_links::AlbumLinks::Unread => None,
            })
            .collect();
        if let Err(error) = self.database.replace_group_album_links(kept).await {
            tracing::error!("What the release groups were read to be was not kept: {error}");
        }
        read
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    async fn read_album_statements(
        &self,
        to_read: &crate::import::album_links::ToRead,
        priority: CallPriority,
    ) -> Vec<crate::import::album_links::GroupReading> {
        use crate::import::album_links::{AlbumLinks, GroupReading};
        let read = match self.discogs_session() {
            Ok(session) => session.read_album_links(to_read, priority).await,
            Err(error) => {
                warn!("Discogs unavailable while reading album links: {error}");
                self.providers
                    .read_album_links(None, to_read, priority)
                    .await
            }
        };
        let twins: Vec<crate::import::search::MetadataResult> = read
            .iter()
            .filter_map(|reading| reading.twin.as_ref().map(|twin| twin.result.clone()))
            .collect();
        let checked = if twins.is_empty() {
            Ok(Vec::new())
        } else {
            crate::identify::annotate_with_library_status(twins, self).await
        };
        match checked {
            Ok(annotated) => {
                let mut statuses = annotated.into_iter().map(|(_, status)| status);
                read.into_iter()
                    .map(|reading| {
                        reading.with_status(|_| {
                            statuses
                                .next()
                                .expect("each twin was checked against the library")
                        })
                    })
                    .collect()
            }
            Err(detail) => {
                tracing::error!(
                    "Twins' library status unread; their groups read as unread: {detail}"
                );
                read.into_iter()
                    .map(|reading| GroupReading {
                        links: match reading.twin {
                            Some(_) => AlbumLinks::Unread,
                            None => reading.links,
                        },
                        group: reading.group,
                        release_links: reading.release_links,
                        twin: None,
                    })
                    .collect()
            }
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_discogs_release_covers(
        &self,
        release_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        self.discogs_session()?
            .release_covers(release_id, priority)
            .await
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn fetch_discogs_master_covers(
        &self,
        master_id: &str,
        priority: CallPriority,
    ) -> Result<Vec<crate::import::cover_art::RemoteCover>, crate::import::ImportError> {
        self.discogs_session()?
            .master_covers(master_id, priority)
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
        let session = self.discogs_session()?;
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

    /// Turn candidate-owned image bytes into library image rows for the new
    /// artists an import creates. An artist the import links to keeps its own
    /// picture; this constructs rows only.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn materialize_prepared_artist_images(
        &self,
        inserted_artists: &[DbArtist],
        prepared: &[crate::import::PreparedArtistImage],
    ) -> Result<Vec<(DbLibraryImage, Vec<u8>)>, LibraryError> {
        let by_discogs_id: HashMap<_, _> = prepared
            .iter()
            .map(|answer| (answer.discogs_artist_id(), answer))
            .collect();
        if by_discogs_id.len() != prepared.len() {
            return Err(LibraryError::Internal(
                "prepared artist images contain a duplicate Discogs artist ID".into(),
            ));
        }
        let mut images = Vec::new();
        for artist in inserted_artists {
            let Some(discogs_artist_id) = artist.discogs_artist_id.as_deref() else {
                continue;
            };
            let answer = by_discogs_id.get(discogs_artist_id).ok_or_else(|| {
                LibraryError::Internal(format!(
                    "new Discogs artist {discogs_artist_id} has no prepared image answer"
                ))
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
                source: crate::import::Catalog::Discogs.as_str().to_string(),
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
        let validation = self.discogs_session()?.validate().await?;
        self.set_discogs_validation(validation).await?;
        Ok(())
    }

    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    pub(super) fn discogs_available_for_test(&self) -> Result<bool, LibraryError> {
        Ok(self.discogs_session()?.client.is_some())
    }

    #[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
    pub(super) async fn record_discogs_validation_for_test(&self, signal: DiscogsKeySignal) {
        let config_handle = Arc::clone(&self.config_handle);
        blocking(move || {
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
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn can_fetch_releases_from(
        &self,
        catalog: crate::import::Catalog,
    ) -> Result<bool, LibraryError> {
        match catalog {
            crate::import::Catalog::MusicBrainz => Ok(true),
            crate::import::Catalog::Discogs => Ok(self.discogs_session()?.client.is_some()),
            other => unreachable!("nothing fetches releases from {}", other.as_str()),
        }
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
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

/// Run keychain and config writes on a blocking thread.
async fn blocking(
    work: impl FnOnce() -> Result<(), LibraryError> + Send + 'static,
) -> Result<(), LibraryError> {
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}
