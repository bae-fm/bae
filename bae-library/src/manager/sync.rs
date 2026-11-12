//! Cloud sync surface for [`LibraryManager`]: provider connection, the upload
//! pipeline, and pause state.

use super::*;

/// Whether a storage setup running `step` is past its point of no return:
/// creating the library in storage, or recording changed access. From there
/// coven finishes it rather than undoing it, so its screen offers no Cancel.
pub fn storage_setup_finishing(step: &coven::StorageSetupStep) -> bool {
    matches!(
        step,
        coven::StorageSetupStep::CreatingStore | coven::StorageSetupStep::RecordingAccess
    )
}

impl LibraryManager {
    pub async fn is_sync_configured(&self) -> Result<bool, LibraryError> {
        Ok(self.store.storage_config().await?.is_some())
    }

    /// Whether coven's sync loop is running now: storage is set up and
    /// syncing isn't stopped.
    pub async fn is_sync_ready(&self) -> Result<bool, LibraryError> {
        Ok(self.sync_status().await?.running())
    }

    /// The sync status now, as [`Self::subscribe_sync_status`] makes each.
    pub async fn sync_status(&self) -> Result<bae_sync_status::SyncStatusSnapshot, LibraryError> {
        Ok(self
            .store
            .read_with_sync_state(
                bae_sync_status::sync_status_rows_on,
                bae_sync_status::SyncStatusSnapshot::read,
            )
            .await?)
    }

    /// Stop the sync loop: returns once no pass runs.
    pub async fn stop_sync(&self) -> Result<(), LibraryError> {
        Ok(self.store.stop_sync().await?)
    }

    /// The person asked to retry everything now, such as by pulling to
    /// refresh. coven ends every ordinary backoff, answered or not: the
    /// connection's, operations', uploads' and downloads', each keeping its
    /// failure count, and runs a pass. Provider cooldowns, failures that
    /// never retry, and waits for new credentials hold. Automatic triggers
    /// use [`Store::sync_now`](bae_store::Store::sync_now), which leaves every
    /// backoff in place.
    ///
    /// The retry is in place once this returns, with its answer to await:
    /// the first pass after it ends, with what came of it. Deciding the
    /// answer writes coven's retry state; a failed write is returned.
    pub async fn retry_now(
        &self,
    ) -> Result<
        impl std::future::Future<Output = bae_sync_status::SyncRetryOutcome> + Send + 'static,
        LibraryError,
    > {
        let answer = self.store.retry_now().await?;
        Ok(async move { answer.await.into() })
    }

    /// Set S3 up as the library's storage, telling `steps` each step coven
    /// runs.
    pub async fn save_s3_config(
        &self,
        data: S3ConfigData,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let (storage, credentials) = data
            .into_location()
            .map_err(|error| LibraryError::Validation(format!("invalid S3 endpoint: {error}")))?;
        let setup = self
            .heard_setup(self.store.setup_s3_storage(
                storage,
                device_name,
                credentials.access_key_id,
                credentials.secret_access_key,
                steps,
            ))
            .await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected { provider: coven::CloudProvider::S3 });
        Ok(setup)
    }

    /// Sign in to an account provider before choosing a new location on it.
    /// bae registers a Full Dropbox app, since a household's library lives in
    /// a folder its members share.
    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in(&self, provider: coven::CloudProvider) -> Result<(), LibraryError> {
        let access = match provider {
            coven::CloudProvider::Dropbox => {
                coven::OAuthAccess::Dropbox(coven::DropboxAccess::FullDropbox)
            }
            coven::CloudProvider::OneDrive => coven::OAuthAccess::OneDrive,
            coven::CloudProvider::S3 | coven::CloudProvider::CloudKit => {
                return Err(LibraryError::Internal(format!("{provider:?} has no account sign-in")))
            }
        };
        self.store.authenticate(access).await?;
        Ok(())
    }

    /// Sign in again to the provider this library's location is on, with the
    /// access that location needs, and reconnect the location with the new
    /// sign-in.
    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in_again(
        &self,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let storage = self.store.storage_config().await?.ok_or_else(|| {
            LibraryError::Internal("signing in again needs a library with storage".into())
        })?;
        let access = storage.oauth_access().ok_or_else(|| {
            LibraryError::Internal(format!(
                "{:?} storage has no account sign-in",
                storage.provider()
            ))
        })?;
        self.store.authenticate(access).await?;
        self.setup_oauth_storage(storage, device_name, steps).await
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let provider = storage.provider();
        let setup =
            self.heard_setup(self.store.setup_oauth_storage(storage, device_name, steps)).await?;
        self.diagnostics.event(TelemetryEvent::CloudProviderConnected { provider });
        Ok(setup)
    }

    pub async fn use_cloudkit(
        &self,
        device_name: &str,
        steps: crate::StorageSetupSteps<'_>,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let setup = self.heard_setup(self.store.setup_cloudkit_storage(device_name, steps)).await?;
        self.diagnostics.event(TelemetryEvent::CloudProviderConnected {
            provider: coven::CloudProvider::CloudKit,
        });
        Ok(setup)
    }

    /// Run a storage setup call. Every setup that ended before it is
    /// acknowledged first, since the new attempt supersedes what they said:
    /// when that fails, the setup is refused before it starts, with nothing
    /// done and no restore code to hide, and a retry is safe. The call
    /// acknowledges nothing after it: one that returned `Ok` left a settled
    /// row, which says nothing and goes when the library opens; one that
    /// failed left its undone row, which repeats the error the call returned
    /// and stays until the person dismisses it or a later setup is begun.
    ///
    /// Dropping the call, as leaving a setup's screen does, cancels the
    /// setup: coven runs a dropped setup to its end, with nobody waiting
    /// for it. A setup past its point of no return finishes anyway.
    async fn heard_setup<T>(
        &self,
        setup: impl std::future::Future<Output = Result<T, coven::StorageSetupError>>,
    ) -> Result<T, LibraryError> {
        self.sync_status.acknowledge_storage_setups().await?;
        let mut dropped = CancelSetupOnDrop { store: &self.store, returned: false };
        let result = setup.await;
        dropped.returned = true;
        Ok(result?)
    }

    /// Whether a storage setup runs on this device. As the library opens,
    /// before any setup call, one that runs is past its point of no return,
    /// and starting sync finishes it.
    pub async fn storage_setup_running(&self) -> Result<bool, LibraryError> {
        Ok(self.sync_status.storage_setup_running().await?)
    }

    /// The library opened: a storage setup that finished, or that the
    /// person cancelled, with nobody to hear it tells them nothing, so it
    /// leaves `coven_operations`. One that ended undone stays for the
    /// status to show.
    pub async fn acknowledge_settled_storage_setups(&self) -> Result<(), LibraryError> {
        Ok(self.sync_status.acknowledge_settled_storage_setups().await?)
    }

    /// Start coven's sync loop with the store keys this session unlocked. Its
    /// first pass connects, and a pass that can't reach storage backs off and
    /// retries like any other; starting fails only when the session lacks a
    /// store key or custody can't be read.
    pub async fn attach_and_start_sync(&self) -> Result<(), LibraryError> {
        self.store.start_sync().await?;
        Ok(())
    }

    /// Start sync for a returning user without withholding local library
    /// services. [`Self::start`] must already have installed the status,
    /// pending subscriptions so the connection's first events
    /// have consumers.
    /// The task runs on the runtime retained by the app and is supervised like
    /// the manager's other background workers.
    pub fn start_sync_at_startup(&self) -> Result<(), LibraryError> {
        let manager = self.clone();
        let failed_start = self.clone();
        self.spawn_supervised_task(
            "startup sync attachment",
            async move {
                manager.attach_and_start_sync_at_startup().await;
            },
            // A start that panicked started no loop, like one that failed:
            // the status line says why and offers Reconnect.
            move |failed| {
                let manager = failed_start.clone();
                let error = LibraryError::from(failed);
                let recorded = failed_start.spawn_supervised_task(
                    "record the failed sync start",
                    async move { manager.set_sync_error(&error).await },
                    |_| {},
                );
                if let Err(error) = recorded {
                    tracing::warn!(%error, "could not record the failed sync start");
                }
            },
        )
    }

    /// Start sync at startup, tolerating a failure.
    ///
    /// A returning user who launches offline still opens their library — local
    /// browse and pinned playback need no network: the loop starts, the status
    /// line reads offline, and the loop retries on its own. A start that fails
    /// outright, such as a keychain locked for the session, starts no loop and
    /// is recorded as the sync-status error in its own category, so the status
    /// line says why and offers Reconnect.
    pub async fn attach_and_start_sync_at_startup(&self) {
        let started = std::time::Instant::now();
        tracing::info!("startup sync attachment started");
        if let Err(error) = self.attach_and_start_sync().await {
            warn!("startup sync connect failed; opening library not connected: {error}");
            self.set_sync_error(&error).await;
        }
        tracing::info!(
            attachment_ms = started.elapsed().as_millis(),
            "startup sync attachment returned"
        );
    }

    /// Retry sync against the provider this library already has configured, so a
    /// failure a user has since fixed — an offline launch, an unreachable
    /// endpoint, a bad object the cloud side no longer serves — clears without
    /// relaunching the app or re-entering the provider settings.
    ///
    /// Two failures land here and each needs a different move. A start that
    /// failed left no loop running, and only starting it again builds one; a
    /// running loop whose pass failed is backing off, and a retry ends its
    /// backoff and runs its next pass now instead of when the backoff ends. `is_sync_ready` is what
    /// separates them: it is true only while a loop is running.
    ///
    /// A start that fails becomes this retry's recorded sync-status error, so
    /// the status line shows why the retry didn't take; it is cleared the moment a retry is under way. A library
    /// with no provider, and no setup running toward one, is not failing sync,
    /// it has none, so that refusal goes back to the caller alone rather than
    /// posing as a sync failure. A setup past its point of no return commits
    /// its provider only once sync's connection finishes it, so it starts sync
    /// as a provider does.
    pub async fn reconnect_sync(&self) -> Result<(), LibraryError> {
        if !self.is_sync_configured().await? && !self.storage_setup_running().await? {
            return Err(coven::SyncError::NoStorage.into());
        }
        if !self.is_sync_ready().await? {
            if let Err(error) = self.attach_and_start_sync().await {
                warn!("sync reconnect failed: {error}");
                self.set_sync_error(&error).await;
                return Err(error);
            }
        }
        self.sync_status.clear_sync_start_failure().await?;
        drop(self.retry_now().await?);
        Ok(())
    }

    /// Record the failed start the status line reads, in the failure's own
    /// category, as the sync loop's failures are: a locked keychain reads as
    /// locked, a refused sign-in as credentials. A failure to record it is
    /// logged and reported as a bug: the status line can't say why sync
    /// didn't start.
    async fn set_sync_error(&self, error: &LibraryError) {
        let failure = bae_diagnostics::UiError::diagnostic(error.category(), error);
        if let Err(error) = self.sync_status.record_sync_start_failure(failure).await {
            self.diagnostics.report_failure("record why sync failed to start", &error);
        }
    }
}

impl LibraryManager {}

/// Cancels the storage setup a call runs when the call is dropped before it
/// returns: see [`LibraryManager::heard_setup`].
struct CancelSetupOnDrop<'a> {
    store: &'a bae_store::Store,
    returned: bool,
}

impl Drop for CancelSetupOnDrop<'_> {
    fn drop(&mut self) {
        if !self.returned {
            let answer = self.store.cancel_storage_setup();
            tracing::info!(?answer, "a dropped storage setup call cancelled its setup");
        }
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod tests;
