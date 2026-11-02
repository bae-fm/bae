//! Where import and the library get the catalog session an operation asks
//! with.
//!
//! The client for the stored Discogs key, with its validation callback, is
//! built here and handed out inside a session; no caller receives the
//! database the callback writes to.

use crate::catalog_model::providers::{CatalogSession, Providers};
use crate::config::DiscogsValidation;
use crate::db::{Database, DbError};
use crate::diagnostics::Diagnostics;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::discogs::client::{DiscogsClient, DiscogsError, DiscogsKeySignal};
use bae_catalogs::rate_limiter::CallPriority;
use std::future::Future;
use std::sync::Arc;
use tracing::{debug, warn};

/// Why a session that asks Discogs could not be had: the library's
/// preferences, which say whether a key is stored, or the keyring holding it,
/// could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CatalogSessionError {
    #[error(transparent)]
    Preferences(#[from] DbError),
    #[error("Keyring error: {0}")]
    Keyring(#[from] coven::KeyError),
}

/// The catalogs as this library may ask them: the providers, and the one
/// Discogs client for the key it stores.
#[derive(Clone)]
pub struct CatalogSessions {
    providers: Providers,
    /// Where the stored key's state is read and its validation recorded, and
    /// the keyring the key is read from.
    database: Database,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    /// The client for the stored key, built on first use and dropped when
    /// the key is set or cleared, so every Discogs call shares one key read
    /// and one client. Held under one lock with the key's own writes, so no
    /// client outlives the key it was built with.
    discogs_client: Arc<tokio::sync::Mutex<Option<Arc<DiscogsClient>>>>,
}

impl CatalogSessions {
    pub(crate) fn new(
        providers: Providers,
        database: Database,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            providers,
            database,
            diagnostics,
            runtime_handle,
            discogs_client: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Discogs as this library may ask it right now. No client while no key
    /// is stored or the stored one was rejected. Otherwise the one client for
    /// the stored key, built the first time it is needed — reading the key
    /// off the keyring once rather than on every call — and reused until the
    /// key is set or cleared.
    pub(crate) async fn session(&self) -> Result<CatalogSession, CatalogSessionError> {
        let providers = self.providers.clone();
        // Held while the stored key is read, so a key being set or cleared is
        // seen whole or not at all.
        let mut cached = self.discogs_client.lock().await;
        let validation = self.database.preferences().await?.discogs;
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
                    let recorded = bae_util::task_failure::caught(
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

    /// The session a search of `catalog` asks with, holding only what that
    /// catalog needs: MusicBrainz needs no key, so its session reads neither
    /// the preferences nor the keyring.
    pub(crate) async fn search_session(
        &self,
        catalog: LookupCatalog,
    ) -> Result<CatalogSession, CatalogSessionError> {
        match catalog {
            LookupCatalog::MusicBrainz => Ok(CatalogSession::new(None, self.providers.clone())),
            LookupCatalog::Discogs => self.session().await,
        }
    }

    /// The session a read of `release` asks with. A MusicBrainz release
    /// whose Discogs session cannot be had is read without its Discogs
    /// cross-reference, as it is without a key.
    pub(crate) async fn release_read_session(
        &self,
        release: &crate::catalog_model::records::LookupRef,
    ) -> Result<CatalogSession, CatalogSessionError> {
        match self.session().await {
            Ok(session) => Ok(session),
            Err(error) if release.catalog == LookupCatalog::MusicBrainz => {
                warn!(
                    release_id = %release.key,
                    "Discogs cross-reference unavailable while fetching MusicBrainz release: {error}"
                );
                Ok(CatalogSession::new(None, self.providers.clone()))
            }
            Err(error) => Err(error),
        }
    }

    /// Ask Discogs whether it accepts `key`, which this library does not
    /// store: how a key is tried before it is saved.
    pub(crate) async fn try_discogs_key(
        &self,
        key: &str,
        priority: CallPriority,
    ) -> Result<(), DiscogsError> {
        self.providers.discogs_client(key.to_string(), None).validate_token(priority).await
    }

    /// Run `change`, the writes that set or clear the stored key, with no
    /// session built in between, and drop the client built for the key it
    /// replaces.
    pub(crate) async fn change_discogs_key<T>(&self, change: impl Future<Output = T>) -> T {
        let mut client = self.discogs_client.lock().await;
        let changed = change.await;
        *client = None;
        changed
    }

    #[cfg(test)]
    pub(crate) async fn record_discogs_validation_for_test(&self, signal: DiscogsKeySignal) {
        record_discogs_validation_signal(&self.database, signal).await;
    }
}

/// Fold one Discogs call's outcome into the stored key's validation: a 401
/// rejects it, and a success confirms a key nothing had confirmed yet.
/// Decided in the write against the state it finds, so a signal racing a key
/// change applies to the key it finds. A failure to write it is logged; the
/// next call's outcome is recorded again.
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
