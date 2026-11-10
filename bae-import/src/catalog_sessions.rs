//! Where import and the library get the catalog session an operation asks
//! with.
//!
//! CatalogSessions owns the library's Discogs key and what Discogs said of
//! it: every write of either is made here, under one lock. The client for
//! the stored key, with its validation callback, is built here and handed
//! out inside a session; no caller receives the store the callback
//! writes to. Each key stored has a generation of its own, and what a check
//! or a call says about a key is recorded only while that key is still the
//! one stored: an answer about a key replaced since says nothing about the
//! new one.

use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::discogs::client::{DiscogsClient, DiscogsError, DiscogsKeySignal};
use bae_catalogs::providers::{CatalogSession, DiscogsCheck, Providers};
use bae_catalogs::rate_limiter::{CallClass, CallWork};
use bae_diagnostics::{lock_or_crash, Diagnostics};
use bae_preferences::DiscogsValidation;
use bae_store::DbError;
use bae_store::Store;
use std::future::Future;
use std::sync::Arc;
use tracing::debug;

/// The name the stored Discogs key is kept under through coven's host
/// secrets.
const DISCOGS_API_KEY: &str = "discogs_api_key";

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

/// Why the Discogs key could not be stored or checked: the keyring or the
/// preferences row refused the write, the keyring write ran apart and gave no
/// result, or Discogs gave no verdict on the key.
#[derive(Debug, thiserror::Error)]
pub enum DiscogsKeyError {
    #[error("Keyring error: {0}")]
    Keyring(#[from] coven::KeyError),
    #[error(transparent)]
    Preferences(#[from] bae_preferences::PreferencesError),
    #[error(transparent)]
    Task(#[from] bae_util::task_failure::TaskFailed),
    /// The preferences say a key is stored, and the keyring holds none.
    #[error("the preferences say a Discogs key is stored but the keyring has none")]
    Missing,
    /// Discogs answered the check with something that is no verdict on the
    /// key (an error status, an unreadable body). The key's state is left as
    /// it was, and whoever asked is told, so they ask again.
    #[error(transparent)]
    NoVerdict(DiscogsError),
}

/// The catalogs as this library may ask them: the providers, and the one
/// Discogs client for the key it stores.
#[derive(Clone)]
pub struct CatalogSessions {
    providers: Providers,
    /// Where the stored key's state is read and its validation recorded, and
    /// the keyring the key is read from.
    store: Store,
    /// The row the key's validation is recorded in.
    preferences: bae_preferences::PreferencesRow,
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    /// The stored key's client and generation, under the one lock every
    /// write of the key and its validation holds.
    discogs: Arc<tokio::sync::Mutex<DiscogsKey>>,
    /// The writes of what calls prove about the key, in flight: each runs as
    /// a task of its own, which closing the library waits for
    /// ([`Self::wait_for_writes`]) before the store closes under it.
    writes: Arc<std::sync::Mutex<Vec<bae_diagnostics::ReportedTask>>>,
}

/// The stored Discogs key as the sessions hold it.
#[derive(Default)]
struct DiscogsKey {
    /// The client for the stored key, built on first use and dropped when
    /// the key is set or cleared, so every Discogs call shares one key read
    /// and one client.
    client: Option<Arc<DiscogsClient>>,
    /// Which key is stored: it moves with every set or clear, so what is
    /// learned about a key is written only while it is still the one stored.
    generation: u64,
}

impl CatalogSessions {
    /// The sessions a library asks the catalogs through: `providers`, and the
    /// Discogs key `store` stores.
    pub fn new(
        providers: Providers,
        store: Store,
        preferences: bae_preferences::PreferencesRow,
        diagnostics: Diagnostics,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            providers,
            store,
            preferences,
            diagnostics,
            runtime_handle,
            discogs: Arc::default(),
            writes: Arc::default(),
        }
    }

    /// The catalogs as this library may ask them right now: those the person
    /// has switched on, and Discogs as the stored key allows. No client while
    /// no key is stored or the stored one was rejected. Otherwise the one
    /// client for the stored key, built the first time it is needed — reading
    /// the key off the keyring once rather than on every call — and reused
    /// until the key is set or cleared.
    pub(crate) async fn session(&self) -> Result<CatalogSession, CatalogSessionError> {
        // Held while the stored key is read, so a key being set or cleared is
        // seen whole or not at all.
        let mut key = self.discogs.lock().await;
        let preferences = bae_preferences::preferences(&self.store).await?;
        let asked = bae_preferences::asked_sources(&preferences.metadata_sources());
        if matches!(preferences.discogs, None | Some(DiscogsValidation::Rejected)) {
            return Ok(CatalogSession::new(None, asked, self.providers.clone()));
        }
        Ok(self.session_with(&mut key, asked)?)
    }

    /// A session asking the catalogs of `asked`, Discogs with the client for
    /// the stored key, built the first time it is needed with an observer
    /// recording what each call proves about that key; one without Discogs
    /// while the keyring holds no key.
    fn session_with(
        &self,
        key: &mut DiscogsKey,
        asked: Vec<LookupCatalog>,
    ) -> Result<CatalogSession, coven::KeyError> {
        if key.client.is_none() {
            // The client reports from its request path; recording the signal
            // is a write, so it runs as its own task. Nothing waits on it, so
            // a write that fails, or panics, is reported as a bug. The key it
            // is recorded under is held weakly: the client lives inside it.
            let held = Arc::downgrade(&self.discogs);
            let generation = key.generation;
            let preferences = self.preferences.clone();
            let diagnostics = self.diagnostics.clone();
            let runtime = self.runtime_handle.clone();
            let writes = self.writes.clone();
            let reporter = self.diagnostics.clone();
            let observer = Arc::new(move |signal| {
                let (held, preferences, diagnostics) =
                    (held.clone(), preferences.clone(), diagnostics.clone());
                let mut writes = lock_or_crash(&reporter, "the Discogs signal writes", &writes);
                // Writes that ended since the last leave the set here.
                writes.retain(|write| !write.is_finished());
                writes.push(reporter.spawn_ending(
                    &runtime,
                    "record a Discogs validation signal",
                    async move { record_signal(&held, &preferences, generation, signal).await },
                    // A panic was reported as it happened.
                    move |recorded| {
                        if let Ok(Err(error)) = recorded {
                            diagnostics.report_failure("record a Discogs validation signal", &error)
                        }
                    },
                ));
            });
            key.client = self
                .store
                .host_secret(DISCOGS_API_KEY)?
                .map(|stored| Arc::new(self.providers.discogs_client(stored, Some(observer))));
        }
        Ok(CatalogSession::new(key.client.clone(), asked, self.providers.clone()))
    }

    /// Wait until every signal write started so far has ended: what closing
    /// the library does after the calls that make them have stopped, before
    /// the store closes under them. A write started after this takes the set
    /// is waited for by the next call.
    pub async fn wait_for_writes(&self) {
        let writes = std::mem::take(&mut *lock_or_crash(
            &self.diagnostics,
            "the Discogs signal writes",
            &self.writes,
        ));
        for write in writes {
            write.ended().await;
        }
    }

    /// The session a search of `catalog` asks with, holding only what that
    /// catalog needs: MusicBrainz needs no key, so its session reads neither
    /// the preferences nor the keyring.
    pub(crate) async fn search_session(
        &self,
        catalog: LookupCatalog,
    ) -> Result<CatalogSession, CatalogSessionError> {
        match catalog {
            LookupCatalog::MusicBrainz => Ok(self.musicbrainz()),
            LookupCatalog::Discogs => self.session().await,
        }
    }

    /// A piece of work no other catalog call is part of, whose calls a
    /// person can come to await.
    pub(crate) fn new_work(&self) -> CallWork {
        self.providers.new_work()
    }

    /// Make `works` the work a person awaits now: their calls go ahead of
    /// every other, those already waiting included.
    pub(crate) fn await_work(&self, works: &std::collections::BTreeSet<CallWork>) {
        self.providers.await_work(works);
    }

    /// The session that asks MusicBrainz alone, which needs no key.
    pub(crate) fn musicbrainz(&self) -> CatalogSession {
        CatalogSession::new(None, vec![LookupCatalog::MusicBrainz], self.providers.clone())
    }

    /// Ask Discogs whether it accepts `key`, which this library does not
    /// store: how a key is tried before it is saved, while the person who
    /// submitted it waits on the answer.
    pub(crate) async fn try_discogs_key(&self, key: &str) -> Result<(), DiscogsError> {
        self.providers
            .discogs_client(key.to_string(), None)
            .validate_token(CallClass::AWAITED)
            .await
    }

    /// The library's stored Discogs key, read from the keychain on a blocking
    /// thread: a keychain call can wait on the system, and on a prompt.
    pub async fn stored_discogs_key(&self) -> Result<Option<String>, DiscogsKeyError> {
        let store = self.store.clone();
        let key = self
            .diagnostics
            .blocking("read the Discogs key", move || store.host_secret(DISCOGS_API_KEY))
            .await??;
        Ok(key)
    }

    /// Store `token` as the library's Discogs key, with what Discogs said of
    /// it. The keyring bytes are written before the key's state is recorded:
    /// a failure between the writes leaves Discogs disabled until the caller
    /// retries, and the preferences never claim a key the keyring lacks. The
    /// keyring write blocks, so it runs on a blocking thread.
    pub async fn store_discogs_key(
        &self,
        token: &str,
        validation: DiscogsValidation,
    ) -> Result<(), DiscogsKeyError> {
        self.change_discogs_key(async {
            let store = self.store.clone();
            let token = token.to_string();
            self.diagnostics
                .blocking("store the Discogs key", move || {
                    store.set_host_secret(DISCOGS_API_KEY, &token)
                })
                .await??;
            self.preferences
                .edit(move |prefs| {
                    prefs.discogs = Some(validation);
                    Ok(())
                })
                .await?;
            Ok(())
        })
        .await
    }

    /// Remove the library's Discogs key and its state. The state is cleared
    /// before the keyring bytes, so a failure between the writes leaves
    /// Discogs disabled rather than half-enabled.
    pub(crate) async fn clear_discogs_key(&self) -> Result<(), DiscogsKeyError> {
        self.change_discogs_key(async {
            self.preferences
                .edit(|prefs| {
                    prefs.discogs = None;
                    Ok(())
                })
                .await?;
            let store = self.store.clone();
            self.diagnostics
                .blocking("clear the Discogs key", move || {
                    store.delete_host_secret(DISCOGS_API_KEY)
                })
                .await??;
            Ok(())
        })
        .await
    }

    /// Run `change`, the writes that set or clear the stored key, with no
    /// session built and no validation recorded in between, and drop the
    /// client built for the key it replaces: what is learned about that key
    /// from here on is recorded nowhere.
    async fn change_discogs_key<T>(&self, change: impl Future<Output = T>) -> T {
        let mut key = self.discogs.lock().await;
        let changed = change.await;
        key.client = None;
        key.generation += 1;
        changed
    }

    /// Check `token`, a key a person submitted, against Discogs, and store it
    /// with what the check said unless Discogs rejected it. A key checked
    /// while offline is stored unvalidated; Discogs answering with no verdict
    /// stores nothing and fails, so the person submits it again.
    pub(crate) async fn save_discogs_key(
        &self,
        token: &str,
    ) -> Result<DiscogsCheck, DiscogsKeyError> {
        let check = DiscogsCheck::of(self.try_discogs_key(token).await)
            .map_err(DiscogsKeyError::NoVerdict)?;
        if !matches!(check, DiscogsCheck::Rejected) {
            self.store_discogs_key(token, DiscogsValidation::of(&check)).await?;
        }
        Ok(check)
    }

    /// Check the stored key against Discogs again when it is `Unvalidated`,
    /// and record what Discogs said while that key is still the one stored.
    /// `None`, asking nothing, while no key is stored or it is in any other
    /// state. An answer that is no verdict on the key leaves it as it is and
    /// fails.
    pub(crate) async fn revalidate_discogs_key(
        &self,
    ) -> Result<Option<DiscogsCheck>, DiscogsKeyError> {
        let (session, generation) = {
            let mut key = self.discogs.lock().await;
            let preferences = bae_preferences::preferences(&self.store)
                .await
                .map_err(bae_preferences::PreferencesError::from)?;
            if preferences.discogs != Some(DiscogsValidation::Unvalidated) {
                return Ok(None);
            }
            let asked = bae_preferences::asked_sources(&preferences.metadata_sources());
            (self.session_with(&mut key, asked)?, key.generation)
        };
        let check = session
            .validate()
            .await
            .ok_or(DiscogsKeyError::Missing)?
            .map_err(DiscogsKeyError::NoVerdict)?;
        let key = self.discogs.lock().await;
        if key.generation == generation {
            let validation = DiscogsValidation::of(&check);
            self.preferences
                .edit(move |prefs| {
                    if prefs.discogs.is_some() {
                        prefs.discogs = Some(validation);
                    }
                    Ok(())
                })
                .await?;
        } else {
            debug!("the Discogs key checked was replaced while Discogs answered");
        }
        drop(key);
        Ok(Some(check))
    }
}

/// Record what one Discogs call proved about the key of `generation`, while
/// that key is still the one stored; nothing once the sessions are gone.
async fn record_signal(
    held: &std::sync::Weak<tokio::sync::Mutex<DiscogsKey>>,
    preferences: &bae_preferences::PreferencesRow,
    generation: u64,
    signal: DiscogsKeySignal,
) -> Result<(), bae_preferences::PreferencesError> {
    let Some(held) = held.upgrade() else {
        return Ok(());
    };
    let key = held.lock().await;
    if key.generation != generation {
        debug!("a Discogs signal about a key replaced since changes nothing");
        return Ok(());
    }
    record_discogs_validation_signal(preferences, signal).await
}

/// Fold one Discogs call's outcome into the stored key's validation: a 401
/// rejects it, and a success confirms a key nothing had confirmed yet.
/// Decided in the write against the state it finds.
async fn record_discogs_validation_signal(
    preferences: &bae_preferences::PreferencesRow,
    signal: DiscogsKeySignal,
) -> Result<(), bae_preferences::PreferencesError> {
    preferences
        .edit(move |prefs| {
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
        .await
}

#[cfg(test)]
#[path = "catalog_sessions_tests.rs"]
pub(crate) mod tests;
