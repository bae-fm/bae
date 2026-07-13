//! The cloud-sync responsibility extracted from [`LibraryManager`]: the upload
//! pipeline (outbox in-flight + throughput + pause), the connection lifecycle and
//! provider configuration, the encryption-service cell, and the membership ops.
//!
//! `LibraryManager` holds one `SyncController` and delegates its public sync API
//! to it. The controller never references the manager back; the resolver-side
//! work that a few sync entry points also need (re-emitting every album after a
//! cloud-home change) stays on the manager, which calls the controller for the
//! sync part and does the re-emit itself.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::config::{CloudProvider, ConfigHandle};
use crate::db::Database;
use crate::diagnostics::{Diagnostics, TelemetryEvent};
use crate::keys::StoreKeys;
use crate::library::{LibraryError, LibraryEvent, OutboxSnapshot, UploadThroughput};
use crate::sync::S3ConfigData;
#[cfg(feature = "oauth-providers")]
use coven::ClockRef;
#[cfg(any(test, feature = "test-utils"))]
use coven::CloudHome;
use coven::CovenHandle;

/// Owns the sync/upload state and the cloud-connection lifecycle. Holds clones of
/// the handles the sync paths need (coven handle, config, keys, clock, event bus,
/// database) plus the transient upload-pipeline state. Cloned alongside the
/// manager — every field is itself a clone-shared handle or `Arc`.
#[derive(Clone)]
pub(crate) struct SyncController {
    handle: CovenHandle,
    config_handle: Arc<ConfigHandle>,
    key_service: StoreKeys,
    event_tx: broadcast::Sender<LibraryEvent>,
    database: Database,
    /// `file_id`s whose upload is in flight right now, mapped to the live count
    /// of encrypted bytes that have reached the cloud for that file. Shared with
    /// the sync loop's `ReleaseUploadObserver`. Read by `outbox_snapshot`.
    outbox_in_flight: Arc<Mutex<HashMap<String, u64>>>,
    /// Per-release tallies of uploads completed during the current queue burst.
    /// The observer records completions; the snapshot builder merges them with
    /// the remaining rows and clears them when the queue idles.
    upload_sessions: Arc<crate::library::UploadSessions>,
    /// Rolling-window upload-throughput tracker. The observer records bytes; the
    /// snapshot builder reads the rate.
    upload_throughput: Arc<UploadThroughput>,
    /// User-driven pause flag for the cloud-upload pipeline.
    sync_paused: Arc<AtomicBool>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    /// Typed telemetry sink, shared with the owning manager. The
    /// provider-connect/disconnect completions emit through it.
    diagnostics: Diagnostics,
}

impl SyncController {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        handle: CovenHandle,
        config_handle: Arc<ConfigHandle>,
        key_service: StoreKeys,
        event_tx: broadcast::Sender<LibraryEvent>,
        database: Database,
        outbox_in_flight: Arc<Mutex<HashMap<String, u64>>>,
        upload_sessions: Arc<crate::library::UploadSessions>,
        upload_throughput: Arc<UploadThroughput>,
        sync_paused: Arc<AtomicBool>,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            handle,
            config_handle,
            key_service,
            event_tx,
            database,
            outbox_in_flight,
            upload_sessions,
            upload_throughput,
            sync_paused,
            cloudkit_ops,
            diagnostics,
        }
    }

    /// Emit a library event to all subscribers. Mirrors `LibraryManager::emit` so
    /// the controller's outbox-snapshot push reaches the same bus.
    fn emit(&self, event: LibraryEvent) {
        if let Err(err) = self.event_tx.send(event) {
            warn!("library event broadcast had no subscribers: {err}");
        }
    }

    /// Whether this store's master key is established in the keyring on this
    /// device — the "unlocked" signal, independent of whether a cloud provider
    /// is even configured (a home-less store still resolves this once the key
    /// is established). A read failure (the keyring backend itself is broken)
    /// is logged and treated as not-yet-unlocked rather than propagated: every
    /// caller of this predicate is a plain boolean UI/test signal with no
    /// error channel of its own.
    pub(crate) fn has_encryption(&self) -> bool {
        match self.handle.master_key_fingerprint() {
            Ok(fingerprint) => fingerprint.is_some(),
            Err(error) => {
                warn!("failed to read master-key fingerprint: {error}");
                false
            }
        }
    }

    /// The shared upload-in-flight map, for tests that drive the outbox snapshot
    /// by simulating coven's per-file upload progress directly.
    #[cfg(test)]
    pub(crate) fn outbox_in_flight(&self) -> Arc<Mutex<HashMap<String, u64>>> {
        self.outbox_in_flight.clone()
    }

    /// The shared upload-throughput tracker, for tests that assert the rolling
    /// rate after feeding the observer.
    #[cfg(test)]
    pub(crate) fn upload_throughput(&self) -> Arc<UploadThroughput> {
        self.upload_throughput.clone()
    }

    /// The shared upload-pause flag, for tests that build an observer over the
    /// same pipeline state the controller holds.
    #[cfg(test)]
    pub(crate) fn sync_paused(&self) -> Arc<AtomicBool> {
        self.sync_paused.clone()
    }

    /// The shared completed-upload tallies, so the observer records into the
    /// same state the controller's snapshot reads, and the cancel path can
    /// drop a release's tally.
    pub(crate) fn upload_sessions(&self) -> Arc<crate::library::UploadSessions> {
        self.upload_sessions.clone()
    }

    /// Pause or resume the cloud-upload pipeline. Paused means new enqueues
    /// still land in the outbox but the sync cycle won't drain them; in-flight
    /// uploads finish (coven's `drain_uploads` checks the flag between
    /// entries, not mid-write). Re-emits the outbox snapshot so the UI's
    /// paused indicator and the bottom-panel summary update.
    pub(crate) async fn set_sync_paused(&self, paused: bool) {
        self.sync_paused
            .store(paused, std::sync::atomic::Ordering::SeqCst);
        if !paused {
            // Kick the loop so the queue starts draining immediately on resume
            // rather than waiting for the next idle tick.
            self.handle.sync_now();
        }
        self.emit_outbox_changed().await;
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub(crate) fn is_sync_paused(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Build the current outbox snapshot and emit it as `OutboxChanged`. Called
    /// at every outbox mutation, once per sync cycle, and on each upload
    /// lifecycle callback so the Storage Manager's queue panel stays current.
    pub(crate) async fn emit_outbox_changed(&self) {
        match self.build_outbox_snapshot().await {
            Ok(snapshot) => self.emit(LibraryEvent::OutboxChanged { snapshot }),
            Err(e) => warn!("Failed to build outbox snapshot: {e}"),
        }
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary. Seeds the Storage Manager panel before the first
    /// `OutboxChanged` event arrives.
    pub(crate) async fn outbox_snapshot(
        &self,
    ) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        Ok(self.build_outbox_snapshot().await?)
    }

    async fn build_outbox_snapshot(&self) -> Result<OutboxSnapshot, coven::DbError> {
        let in_flight = { self.outbox_in_flight.lock().unwrap().clone() };
        let paused = self.is_sync_paused();
        crate::library::outbox_snapshot::build_outbox_snapshot(
            &self.database,
            &in_flight,
            &self.upload_sessions,
            &self.upload_throughput,
            paused,
        )
        .await
    }

    /// The library's membership: its devices (with this device flagged, each
    /// member's fingerprint, and whether it can be removed) and whether the
    /// running device is an owner.
    pub(crate) async fn get_members(
        &self,
    ) -> Result<crate::sync::membership::Membership, LibraryError> {
        let members = self.handle.get_members().await?;
        Ok(crate::sync::membership::Membership::from_members(members))
    }

    /// Approve a device into the library by its public key, wrapping the library
    /// key to it and signing a membership entry. Returns the invite code to hand
    /// back to the joining device. bae adds every device as a `Member`; the
    /// founding device is the `Owner`.
    pub(crate) async fn invite_member(
        &self,
        public_key_hex: &str,
        provider_account_email: Option<&str>,
    ) -> Result<String, LibraryError> {
        Ok(self
            .handle
            .invite_member(
                public_key_hex,
                provider_account_email,
                crate::sync::membership::MemberRole::Member,
            )
            .await?)
    }

    /// Remove a device from the library and rotate the library key so the removed
    /// device can no longer read new data. Records the rotated key's fingerprint
    /// in this device's config.
    pub(crate) async fn remove_member(&self, public_key_hex: &str) -> Result<(), LibraryError> {
        // coven's `remove_member` future is `!Send`: internally it holds coven's
        // keyring trait object (`dyn KeyPersistence`, which carries no `Sync`
        // bound) across its awaits while it rotates and re-adopts the library
        // key. uniffi's async bridge export requires the whole call chain to be
        // `Send`, so this future can't be `.await`ed inline. Drive it to
        // completion on a blocking-pool thread via a private current-thread
        // runtime — the same block-on shape coven's own sync loop uses for its
        // cloud work — and hand back only the `Send` fingerprint string.
        let handle = self.handle.clone();
        let public_key_hex = public_key_hex.to_string();
        let fingerprint = tokio::task::spawn_blocking(move || -> Result<String, LibraryError> {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    LibraryError::Internal(format!("failed to build member-removal runtime: {e}"))
                })?
                .block_on(handle.remove_member(&public_key_hex))
                .map_err(LibraryError::from)
        })
        .await
        .map_err(|e| {
            LibraryError::Internal(format!("member-removal task failed to complete: {e}"))
        })??;
        self.config_handle
            .record_encryption_key_fingerprint(fingerprint)?;
        Ok(())
    }

    /// Probe, persist, and connect an S3 cloud home. The manager re-emits every
    /// album afterward — they have gained their storage actions.
    pub(crate) async fn save_s3_config(&self, data: S3ConfigData) -> Result<(), LibraryError> {
        use crate::keys::CloudHomeCredentials;
        use coven::CloudHome;
        use coven::S3CloudHome;

        // Probe the bucket with the proposed credentials *before* persisting
        // anything: a typo or a missing bucket would otherwise leave the UI showing
        // "Connected", with the user learning sync is broken only from the reconnect
        // banner after the first failed cycle. The probe's typed outcome — bad
        // credentials/bucket vs unreachable endpoint — reaches the UI as distinct
        // error classes.
        let probe_home = S3CloudHome::new(
            data.bucket.clone(),
            data.region.clone(),
            data.endpoint.clone(),
            data.access_key.clone(),
            data.secret_key.clone(),
            data.key_prefix.clone(),
        )
        .await?;
        probe_home.probe().await?;

        let creds = CloudHomeCredentials::S3 {
            access_key: data.access_key,
            secret_key: data.secret_key,
        };
        self.key_service.set_cloud_home_credentials(&creds)?;

        self.config_handle.update(move |c| {
            c.cloud_home.provider = Some(CloudProvider::S3);
            c.cloud_home.s3_bucket = Some(data.bucket);
            c.cloud_home.s3_region = Some(data.region);
            c.cloud_home.s3_endpoint = data.endpoint.filter(|s| !s.is_empty());
            c.cloud_home.s3_key_prefix = data.key_prefix.filter(|s| !s.is_empty());
            c.cloud_home.storage = data.storage;
        })?;

        self.ensure_sync_manager_and_start().await?;
        info!("Saved S3 sync configuration");
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::S3,
            });
        Ok(())
    }

    /// OAuth sign-in + persist for a browsable/opaque provider, then connect. The
    /// manager re-emits every album after this returns.
    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn sign_in_cloud_provider(
        &self,
        provider: CloudProvider,
        storage: crate::config::HomeStorage,
        clock: &ClockRef,
    ) -> Result<(), LibraryError> {
        use coven::{sign_in_dropbox, sign_in_google_drive, sign_in_onedrive};

        // Hold the sender alive across the await so the cancel wait inside
        // `oauth::authorize` never fires — this fn surfaces no cancel signal. If this
        // future is dropped, `oauth::authorize`'s own AbortOnDrop guard tears the
        // listener task down.
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let library_name = self.config_handle.config().store_name.clone();
        let clock = clock.as_ref();
        // coven's sign-ins authorize, resolve the cloud folder, and save tokens to
        // the keyring, returning the folder identifiers; bae persists them here.
        match provider {
            CloudProvider::GoogleDrive => {
                let folder_id =
                    sign_in_google_drive(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| LibraryError::CloudSetup(e.to_string()))?;
                self.config_handle.update(move |c| {
                    c.cloud_home.provider = Some(CloudProvider::GoogleDrive);
                    c.cloud_home.google_drive_folder_id = Some(folder_id);
                    c.cloud_home.storage = storage;
                })?;
            }
            CloudProvider::Dropbox => {
                let folder_path =
                    sign_in_dropbox(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| LibraryError::CloudSetup(e.to_string()))?;
                self.config_handle.update(move |c| {
                    c.cloud_home.provider = Some(CloudProvider::Dropbox);
                    c.cloud_home.dropbox_folder_path = Some(folder_path);
                    c.cloud_home.storage = storage;
                })?;
            }
            CloudProvider::OneDrive => {
                let (drive_id, folder_id) = sign_in_onedrive(&self.key_service, cancel_rx, clock)
                    .await
                    .map_err(|e| LibraryError::CloudSetup(e.to_string()))?;
                self.config_handle.update(move |c| {
                    c.cloud_home.provider = Some(CloudProvider::OneDrive);
                    c.cloud_home.onedrive_drive_id = Some(drive_id);
                    c.cloud_home.onedrive_folder_id = Some(folder_id);
                    c.cloud_home.storage = storage;
                })?;
            }
            _ => {
                return Err(LibraryError::Internal(
                    "provider does not use OAuth sign-in".to_string(),
                ))
            }
        }
        self.ensure_sync_manager_and_start().await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected { provider });
        Ok(())
    }

    /// Persist the CloudKit provider and connect. The manager re-emits every album
    /// after this returns.
    pub(crate) async fn use_cloudkit(
        &self,
        storage: crate::config::HomeStorage,
    ) -> Result<(), LibraryError> {
        self.config_handle.update(move |c| {
            c.cloud_home.provider = Some(CloudProvider::CloudKit);
            c.cloud_home.storage = storage;
            c.cloud_home.cloudkit_owner_name = None;
            c.cloud_home.cloudkit_zone_name = None;
        })?;
        self.ensure_sync_manager_and_start().await?;
        info!("Configured CloudKit cloud provider");
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::CloudKit,
            });
        Ok(())
    }

    /// Stop the sync loop, clear the cloud-home config and credentials, and drop
    /// the encryption service. The manager re-emits every album (storage actions
    /// lost) after this returns.
    pub(crate) fn disconnect_cloud_provider(&self) -> Result<(), LibraryError> {
        // Capture the provider before the config clear below drops it, so the
        // telemetry names which provider was disconnected.
        let provider = self.config_handle.config().cloud_home.provider.clone();

        // Stop the sync loop and drop the installed manager; the library becomes
        // home-less until the next connect.
        self.handle.disconnect_sync();

        // Connecting fills the whole cloud home; disconnecting clears it as a unit.
        self.config_handle
            .update(|c| c.cloud_home = Default::default())?;

        // Clearing the cloud-home credentials from the keyring is coven's concern.
        if let Err(e) = self.key_service.delete_cloud_home_credentials() {
            tracing::warn!("Failed to delete cloud home credentials: {e}");
        }
        if let Some(provider) = provider {
            self.diagnostics
                .event(TelemetryEvent::CloudProviderDisconnected { provider });
        }
        Ok(())
    }

    /// Build, start, and attach a sync manager. Used once at startup for a
    /// returning user with a configured cloud home: coven resolves the at-rest
    /// cipher from the master-key custody itself (an opaque home fails
    /// `SyncError::MasterKeyNotEstablished` if this device's keyring lacks the
    /// key — the caller only reaches this once it knows the key is
    /// established, or the home is keyless/browsable). Shares this
    /// controller's outbox in-flight set and event channel with the sync loop's
    /// upload observer. Call before starting the sync-status listener.
    pub(crate) async fn attach_and_start_sync(&self) -> Result<(), LibraryError> {
        let provider = self.config_handle.config().cloud_home.provider.clone();
        match provider {
            Some(CloudProvider::CloudKit) => {
                let ops = self.cloudkit_ops.clone().ok_or_else(|| {
                    LibraryError::Internal("CloudKit driver not provided".to_string())
                })?;
                self.handle.connect_sync_with_cloudkit(ops).await?;
            }
            _ => {
                self.handle.connect_sync().await?;
            }
        }
        Ok(())
    }

    /// Connect a real `SyncManager` over an injected cloud home for tests, so the
    /// handle's make-Remote / make-Local / upload-drain / read paths all run
    /// against a mock cloud with no live provider — the test counterpart of
    /// `attach_and_start_sync`. `cipher` is the home's at-rest protection:
    /// `Plaintext` for a browsable mock, `Encrypted(service)` for an opaque one.
    /// After this, `has_cloud_home` and `is_sync_ready` resolve off the
    /// connected manager, no override needed.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn connect_test_cloud_home(
        &self,
        cloud_home: Arc<dyn CloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), LibraryError> {
        // An opaque home's blob keyspace shards by the uploading device's
        // public key, so establish this store's identity up front if a test
        // building its handle straight off this config (rather than through
        // `Database::new_test`, which already does this under its own fixed
        // store id) hasn't already. Get-or-create: never mint over one
        // already established.
        let config = self.config_handle.config();
        let identity_custody =
            coven::IdentityCustody::Keyring.resolve(&config.store_id, &config.store_dir);
        if identity_custody.unlock()?.is_none() {
            identity_custody.persist(&coven::UserKeypair::generate())?;
        }

        self.handle
            .connect_sync_with_test_home(cloud_home, cipher)
            .await?;
        Ok(())
    }

    /// Ensure a sync manager exists — minting the encryption key if the home needs
    /// one — and start the sync loop.
    async fn ensure_sync_manager_and_start(&self) -> Result<(), LibraryError> {
        if self.handle.is_connected() {
            self.handle.start_sync().await?;
            return Ok(());
        }

        // An opaque home mints (or reuses) the library key and seals every object
        // under it; a browsable home stores in the clear and has no key at all.
        // Establish the master key via custody only for an opaque home, so a
        // browsable home never mints a key it would never use. Reusing an
        // already-established fingerprint rather than unconditionally minting
        // makes this idempotent across a retry after a failed sync init.
        let storage = self.config_handle.config().cloud_home.storage;
        let fingerprint = if storage.is_opaque() {
            Some(match self.handle.master_key_fingerprint()? {
                Some(fingerprint) => fingerprint,
                None => self.handle.initialize_master_key()?,
            })
        } else {
            None
        };

        // Record the fingerprint here — the step that made it true — and not after
        // the connect below. `initialize_master_key` has already put the key in the
        // keyring, so a connect failure cannot un-establish it; withholding the
        // fingerprint until the connect succeeds would only leave the config denying
        // a key the keyring holds, and the launch gate
        // (`config.encryption_key_stored && keyring-has-key`) would then never attach
        // sync again on any later launch, while the provider stays configured.
        //
        // A browsable home records nothing — it has no key, so `encryption_key_stored`
        // stays false and the next launch builds it keyless.
        //
        // Failing to record is fatal to setup, with nothing connected to roll back:
        // the caller retries, and the retry is idempotent because
        // `master_key_fingerprint` returns the key already established rather than
        // minting a second one.
        if let Some(fingerprint) = fingerprint {
            self.config_handle
                .record_encryption_key_fingerprint(fingerprint)?;
        }

        // Connect the provider: build the cloud home, start the loop, and install
        // the manager. coven resolves the at-rest cipher from the master-key
        // custody just established above, so this passes no key material of its
        // own. A cloud-home build or loop-start failure returns `Err` with nothing
        // installed, so it surfaces here rather than leaving a dead manager — and
        // the library is left exactly as a successful setup would leave it minus the
        // running loop: key established, provider configured, sync attached on the
        // next launch or retry.
        let provider = self.config_handle.config().cloud_home.provider.clone();
        match provider {
            Some(CloudProvider::CloudKit) => {
                let ops = self.cloudkit_ops.clone().ok_or_else(|| {
                    LibraryError::Internal("CloudKit driver not provided".to_string())
                })?;
                self.handle.connect_sync_with_cloudkit(ops).await?;
            }
            _ => {
                self.handle.connect_sync().await?;
            }
        }

        self.handle.sync_now();

        Ok(())
    }
}
