//! The cloud-sync responsibility extracted from [`LibraryManager`]: the upload
//! pipeline (outbox in-flight + throughput + pause), the connection lifecycle and
//! provider configuration, the encryption-service cell, the membership ops, and
//! the coven make-Remote / make-Local primitives.
//!
//! `LibraryManager` holds one `SyncController` and delegates its public sync API
//! to it. The controller never references the manager back; the resolver-side
//! work that a few sync entry points also need (re-emitting every album after a
//! cloud-home change) stays on the manager, which calls the controller for the
//! sync part and does the re-emit itself.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::config::{CloudProvider, ConfigHandle};
use crate::db::Database;
use crate::keys::KeyService;
use crate::library::{LibraryError, LibraryEvent, UploadThroughput};
use crate::sync::sync_manager::S3ConfigData;
#[cfg(feature = "oauth-providers")]
use coven::ClockRef;
#[cfg(any(test, feature = "test-utils"))]
use coven::CloudHome;
use coven::CovenHandle;
use coven::EncryptionService;

/// Owns the sync/upload state and the cloud-connection lifecycle. Holds clones of
/// the handles the sync paths need (coven handle, config, keys, clock, event bus,
/// database) plus the transient upload-pipeline state. Cloned alongside the
/// manager — every field is itself a clone-shared handle or `Arc`.
#[derive(Clone)]
pub(crate) struct SyncController {
    handle: CovenHandle,
    config_handle: Arc<ConfigHandle>,
    key_service: KeyService,
    event_tx: broadcast::Sender<LibraryEvent>,
    database: Database,
    /// `file_id`s whose upload is in flight right now, mapped to the live count
    /// of encrypted bytes that have reached the cloud for that file. Shared with
    /// the sync loop's `ReleaseUploadObserver`. Read by `outbox_snapshot`.
    outbox_in_flight: Arc<Mutex<HashMap<String, u64>>>,
    /// Rolling-window upload-throughput tracker. The observer records bytes; the
    /// snapshot builder reads the rate.
    upload_throughput: Arc<UploadThroughput>,
    /// User-driven pause flag for the cloud-upload pipeline.
    sync_paused: Arc<AtomicBool>,
}

impl SyncController {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        handle: CovenHandle,
        config_handle: Arc<ConfigHandle>,
        key_service: KeyService,
        event_tx: broadcast::Sender<LibraryEvent>,
        database: Database,
        outbox_in_flight: Arc<Mutex<HashMap<String, u64>>>,
        upload_throughput: Arc<UploadThroughput>,
        sync_paused: Arc<AtomicBool>,
    ) -> Self {
        Self {
            handle,
            config_handle,
            key_service,
            event_tx,
            database,
            outbox_in_flight,
            upload_throughput,
            sync_paused,
        }
    }

    /// Emit a library event to all subscribers. Mirrors `LibraryManager::emit` so
    /// the controller's outbox-snapshot push reaches the same bus.
    fn emit(&self, event: LibraryEvent) {
        if let Err(err) = self.event_tx.send(event) {
            warn!("library event broadcast had no subscribers: {err}");
        }
    }

    pub(crate) fn has_encryption(&self) -> bool {
        self.handle.encryption_service().is_some()
    }

    pub(crate) fn get_encryption_service(&self) -> Option<EncryptionService> {
        self.handle.encryption_service()
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

    // =========================================================================
    // Connection state
    // =========================================================================

    /// Whether a cloud provider is connected. Reads config, not manager presence:
    /// the connected provider lives in config and is known synchronously from the
    /// first read, whereas the `SyncManager` (and its cloud client) is built lazily
    /// once connected and still absent on mobile when the first listing query runs.
    pub(crate) fn is_sync_configured(&self) -> bool {
        self.config_handle.config().cloud_home.provider.is_some()
    }

    pub(crate) fn has_cloud_home(&self) -> bool {
        self.handle.is_connected()
    }

    /// Whether the background sync loop is running and draining uploads. The
    /// manage gate requires this: managing has no inline remote flip — the
    /// release only becomes remote once the upload observer (which fires from
    /// inside the running loop) confirms the last upload landed.
    pub(crate) fn is_sync_ready(&self) -> bool {
        self.handle.is_syncing()
    }

    pub(crate) fn trigger_sync(&self) {
        self.handle.sync_now();
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
            self.trigger_sync();
        }
        self.emit_outbox_changed().await;
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub(crate) fn is_sync_paused(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    // =========================================================================
    // Outbox snapshot
    // =========================================================================

    /// Build the current outbox snapshot and emit it as `OutboxChanged`. Called
    /// at every outbox mutation, once per sync cycle, and on each upload
    /// lifecycle callback so the Storage Manager's queue panel stays current.
    pub(crate) async fn emit_outbox_changed(&self) {
        let in_flight = { self.outbox_in_flight.lock().unwrap().clone() };
        let paused = self.is_sync_paused();
        match crate::library::outbox_snapshot::build_outbox_snapshot(
            &self.database,
            &in_flight,
            &self.upload_throughput,
            paused,
        )
        .await
        {
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
        let in_flight = { self.outbox_in_flight.lock().unwrap().clone() };
        let paused = self.is_sync_paused();
        Ok(crate::library::outbox_snapshot::build_outbox_snapshot(
            &self.database,
            &in_flight,
            &self.upload_throughput,
            paused,
        )
        .await?)
    }

    // =========================================================================
    // coven-owned locality transitions (make-Remote / make-Local)
    // =========================================================================

    /// The make-Local destination map: each release file's blob id → the user path
    /// (`new_path/original_filename`) its bytes go back to. Host-provided blobs (the
    /// cover) take no dest — coven restores them to its local store. The single
    /// place the dest shape is built, shared by the production transition and the
    /// test driver.
    async fn make_local_dest(
        &self,
        release_id: &str,
        new_path: &str,
    ) -> Result<HashMap<String, PathBuf>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        Ok(files
            .iter()
            .map(|f| {
                (
                    f.id.clone(),
                    std::path::Path::new(new_path).join(&f.original_filename),
                )
            })
            .collect())
    }

    /// Bridge bae's `CancellationToken` to the `watch::Receiver<bool>` coven's
    /// make_local polls between blobs: when the token fires, flip the watch. Returns
    /// the receiver and the bridge task's handle (abort it once make_local returns).
    /// The single place the bridge lives.
    fn cancel_token_to_watch(
        cancel: &crate::library::CancellationToken,
    ) -> (
        tokio::sync::watch::Receiver<bool>,
        tokio::task::JoinHandle<()>,
    ) {
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(cancel.is_cancelled());
        let token = cancel.clone();
        let handle = tokio::spawn(async move {
            token.cancelled().await;
            let _ = cancel_tx.send(true);
        });
        (cancel_rx, handle)
    }

    /// Map a coven `make_local` result to bae's: a cancel before the commit is a
    /// clean stop (coven rolled back the partial copies and left the release
    /// Remote), every other error is surfaced. Shared by the production transition
    /// and the test driver.
    fn map_make_local_result(
        release_id: &str,
        result: Result<(), coven::MakeLocalError>,
    ) -> Result<(), LibraryError> {
        match result {
            Ok(()) => Ok(()),
            Err(coven::MakeLocalError::Cancelled) => {
                debug!(
                    release_id,
                    "make-local cancelled before commit; release stays Remote"
                );
                Ok(())
            }
            Err(e) => Err(LibraryError::Storage(format!(
                "make release {release_id} local: {e}"
            ))),
        }
    }

    /// Make a release Remote (Local → Remote) through coven: coven enqueues an
    /// upload per user-provided blob from its external file, uploads each, and on
    /// the last flips the `remote` gate true, drops the external refs, deletes the
    /// source files, and re-emits the subtree (the host-provided cover then rides
    /// along). Returns once enqueued; completion fires `on_root_made_remote`.
    pub(crate) async fn coven_make_remote(
        &self,
        release_id: &str,
        pin: bool,
    ) -> Result<(), LibraryError> {
        self.handle
            .make_remote("releases", release_id, pin)
            .await
            .map_err(|e| LibraryError::Storage(format!("make release {release_id} remote: {e}")))
    }

    /// Cancel an in-flight make-Remote of `release_id` through coven: clears the
    /// intent and pending uploads and tombstones any blob already in the cloud.
    /// The gate never flips, so the release stays Local.
    pub(crate) async fn coven_cancel_make_remote(
        &self,
        release_id: &str,
    ) -> Result<(), LibraryError> {
        self.handle
            .cancel_make_remote("releases", release_id)
            .await
            .map_err(|e| {
                LibraryError::Storage(format!("cancel make release {release_id} remote: {e}"))
            })
    }

    /// Make a release Local (Remote → Local) through coven: coven materializes each
    /// blob back to a local file durability-first — every release file (a
    /// user-provided blob) to `new_path/{original_filename}`, the host-provided
    /// cover to coven's local store (no dest) — then flips the `remote` gate false,
    /// registers the external refs, and enqueues the cloud deletes in one atomic
    /// commit. `cancel` aborts before the commit (the release stays Remote).
    pub(crate) async fn coven_make_local(
        &self,
        release_id: &str,
        new_path: &str,
        cancel: &crate::library::CancellationToken,
    ) -> Result<(), LibraryError> {
        let dest = self.make_local_dest(release_id, new_path).await?;
        let (cancel_rx, bridge) = Self::cancel_token_to_watch(cancel);
        let result = self
            .handle
            .make_local("releases", release_id, &dest, &cancel_rx)
            .await;
        bridge.abort();
        Self::map_make_local_result(release_id, result)
    }

    // =========================================================================
    // Membership
    // =========================================================================

    pub(crate) fn generate_restore_code(&self) -> Result<String, String> {
        self.handle.generate_restore_code()
    }

    /// The library's membership: its devices (with this device flagged, each
    /// member's fingerprint, and whether it can be removed) and whether the
    /// running device is an owner.
    pub(crate) async fn get_members(
        &self,
    ) -> Result<crate::sync::sync_manager::Membership, String> {
        let members = self.handle.get_members().await?;
        Ok(crate::sync::sync_manager::Membership::from_members(members))
    }

    /// Approve a device into the library by its public key, wrapping the library
    /// key to it and signing a membership entry. Returns the invite code to hand
    /// back to the joining device. bae adds every device as a `Member`; the
    /// founding device is the `Owner`. `invitee_email` is the joiner's OAuth
    /// account address (decoded from its join-request) that coven shares the
    /// OAuth folder to; `None` for S3.
    pub(crate) async fn invite_member(
        &self,
        public_key_hex: &str,
        invitee_email: Option<&str>,
    ) -> Result<String, String> {
        self.handle
            .invite_member(
                public_key_hex,
                invitee_email,
                crate::sync::sync_manager::MemberRole::Member,
            )
            .await
    }

    /// Remove a device from the library and rotate the library key so the removed
    /// device can no longer read new data. Records the rotated key's fingerprint
    /// in this device's config.
    pub(crate) async fn remove_member(&self, public_key_hex: &str) -> Result<(), String> {
        let fingerprint = self.handle.remove_member(public_key_hex).await?;
        self.config_handle
            .record_encryption_key_fingerprint(fingerprint)
            .map_err(|e| e.to_string())
    }

    // =========================================================================
    // Provider configuration / connection lifecycle
    // =========================================================================

    /// Probe, persist, and connect an S3 cloud home. The resolver-side re-emit of
    /// every album (storage actions gained) is the manager's job after this
    /// returns.
    pub(crate) async fn save_s3_config(&self, data: S3ConfigData) -> Result<(), String> {
        use crate::keys::CloudHomeCredentials;
        use coven::CloudHome;
        use coven::S3CloudHome;

        // Probe the bucket with the proposed credentials *before* persisting
        // anything. A typo or a missing bucket would otherwise leave the UI
        // showing "Connected" and the user discovering broken sync only via
        // the reconnect banner after the first failed cycle.
        let probe_home = S3CloudHome::new(
            data.bucket.clone(),
            data.region.clone(),
            data.endpoint.clone(),
            data.access_key.clone(),
            data.secret_key.clone(),
            data.key_prefix.clone(),
        )
        .await
        .map_err(|e| format!("Failed to build S3 client: {e}"))?;
        probe_home.probe().await.map_err(|e| format!("{e}"))?;

        let creds = CloudHomeCredentials::S3 {
            access_key: data.access_key,
            secret_key: data.secret_key,
        };
        self.key_service
            .set_cloud_home_credentials(&creds)
            .map_err(|e| format!("Failed to save credentials: {e}"))?;

        self.config_handle
            .update(move |c| {
                c.cloud_home.provider = Some(CloudProvider::S3);
                c.cloud_home.s3_bucket = Some(data.bucket);
                c.cloud_home.s3_region = Some(data.region);
                c.cloud_home.s3_endpoint = data.endpoint.filter(|s| !s.is_empty());
                c.cloud_home.s3_key_prefix = data.key_prefix.filter(|s| !s.is_empty());
                c.cloud_home.storage = data.storage;
            })
            .map_err(|e| format!("Failed to save config: {e}"))?;

        self.ensure_sync_manager_and_start().await?;
        info!("Saved S3 sync configuration");
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
    ) -> Result<(), String> {
        use coven::{sign_in_dropbox, sign_in_google_drive, sign_in_onedrive};

        // Hold the sender alive across the await so cancel.wait_for inside
        // oauth::authorize never fires (this fn doesn't surface a cancel
        // signal). When this future is dropped, oauth::authorize's own
        // AbortOnDrop guard tears the listener task down.
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let library_name = self.config_handle.config().library_name.clone();
        let clock = clock.as_ref();
        // coven's sign-ins authorize, resolve the cloud folder, and save tokens to
        // the keyring, returning the folder identifiers; bae persists them here.
        match provider {
            CloudProvider::GoogleDrive => {
                let folder_id =
                    sign_in_google_drive(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::GoogleDrive);
                        c.cloud_home.google_drive_folder_id = Some(folder_id);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            CloudProvider::Dropbox => {
                let folder_path =
                    sign_in_dropbox(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::Dropbox);
                        c.cloud_home.dropbox_folder_path = Some(folder_path);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            CloudProvider::OneDrive => {
                let (drive_id, folder_id) = sign_in_onedrive(&self.key_service, cancel_rx, clock)
                    .await
                    .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::OneDrive);
                        c.cloud_home.onedrive_drive_id = Some(drive_id);
                        c.cloud_home.onedrive_folder_id = Some(folder_id);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            _ => return Err("This provider does not use OAuth sign-in".to_string()),
        }
        self.ensure_sync_manager_and_start().await?;
        Ok(())
    }

    /// Persist the CloudKit provider and connect. The manager re-emits every album
    /// after this returns.
    pub(crate) async fn use_cloudkit(
        &self,
        storage: crate::config::HomeStorage,
    ) -> Result<(), String> {
        self.config_handle
            .update(move |c| {
                c.cloud_home.provider = Some(CloudProvider::CloudKit);
                c.cloud_home.storage = storage;
            })
            .map_err(|e| format!("Failed to save CloudKit config: {e}"))?;
        self.ensure_sync_manager_and_start().await?;
        info!("Configured CloudKit cloud provider");
        Ok(())
    }

    /// Stop the sync loop, clear the cloud-home config and credentials, and drop
    /// the encryption service. The manager re-emits every album (storage actions
    /// lost) after this returns.
    pub(crate) fn disconnect_cloud_provider(&self) -> Result<(), String> {
        // Stop the sync loop and drop the installed manager; the library becomes
        // home-less until the next connect.
        self.handle.disconnect_sync();

        // Connecting fills the whole cloud home; disconnecting clears it as a unit.
        self.config_handle
            .update(|c| c.cloud_home = Default::default())
            .map_err(|e| e.to_string())?;

        // Clearing the cloud-home credentials from the keyring is coven's concern.
        if let Err(e) = self.key_service.delete_cloud_home_credentials() {
            tracing::warn!("Failed to delete cloud home credentials: {e}");
        }
        Ok(())
    }

    /// Build, start, and attach a sync manager. Used once at startup for a
    /// returning user with a configured cloud home: an unlocked key for an opaque
    /// home (`Some`), or no key for a browsable home (`None`). Shares this
    /// controller's outbox in-flight set and event channel with the sync loop's
    /// upload observer. Call before starting the sync-status listener.
    pub(crate) async fn attach_and_start_sync(
        &self,
        encryption_service: Option<EncryptionService>,
    ) -> Result<(), String> {
        self.handle.connect_sync(encryption_service).await?;
        Ok(())
    }

    /// Connect a real `SyncManager` over an injected cloud home for tests, so the
    /// handle's make-Remote / make-Local / upload-drain / read paths all run
    /// against a mock cloud with no live provider — the test counterpart of
    /// `attach_and_start_sync`. `cipher` is the home's at-rest protection:
    /// `Plaintext` for a browsable mock, `Encrypted(service)` for an opaque one.
    /// After this, `has_cloud_home`, `get_encryption_service`, and `is_sync_ready`
    /// all resolve off the connected manager, no override needed.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn connect_test_cloud_home(
        &self,
        cloud_home: Arc<dyn CloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), String> {
        self.handle
            .connect_sync_with_test_home(cloud_home, cipher)
            .await?;
        Ok(())
    }

    /// Ensure a SyncManager exists (creating encryption key if needed) and start sync.
    async fn ensure_sync_manager_and_start(&self) -> Result<(), String> {
        // If we already have a sync manager, just (re)start its loop.
        if self.handle.is_connected() {
            self.handle.start_sync().await?;
            return Ok(());
        }

        // An opaque home mints (or reuses) the library key and seals every object
        // under it; a browsable home stores in the clear and has no key at all.
        // Build the encryption service only for an opaque home, so a browsable
        // home never mints a key it would never use. `get_or_create_encryption_key`
        // is idempotent, so a retry after a failed sync init reuses the key.
        let storage = self.config_handle.config().cloud_home.storage;
        let (enc_service, fingerprint) = if storage.is_opaque() {
            let enc_key_hex = self
                .key_service
                .get_or_create_encryption_key()
                .map_err(|e| format!("Failed to create encryption key: {e}"))?;
            let enc = EncryptionService::new(&enc_key_hex)
                .map_err(|e| format!("Failed to create encryption service: {e}"))?;
            let fingerprint = enc.fingerprint();
            (Some(enc), Some(fingerprint))
        } else {
            (None, None)
        };

        // Connect the provider: build the cloud home, start the loop, and install
        // the manager. A cloud-home build or loop-start failure returns `Err` with
        // nothing installed, so it surfaces here rather than leaving a dead manager
        // — and the encryption-key fingerprint below is reached only on success, so
        // a failed setup stays a clean retry (no fingerprint telling the next
        // launch's unlock flow "encryption is set up" while sync is still broken).
        self.handle.connect_sync(enc_service).await?;

        // Sync started. For an opaque home, persist the encryption-key hint flag so
        // the next launch's unlock flow knows this library has encryption set up. A
        // browsable home records nothing — it has no key, so `encryption_key_stored`
        // stays false and the next launch builds it keyless.
        if let Some(fingerprint) = fingerprint {
            if let Err(e) = self
                .config_handle
                .record_encryption_key_fingerprint(fingerprint)
            {
                self.handle.disconnect_sync();
                return Err(format!("Failed to save config: {e}"));
            }
        }

        self.trigger_sync();

        Ok(())
    }
}
