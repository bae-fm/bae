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

use tracing::{info, warn};

use crate::config::{CloudProvider, ConfigHandle};
use crate::db::Database;
use crate::diagnostics::{Diagnostics, TelemetryEvent};
use crate::keys::StoreKeys;
use crate::library::{LibraryError, OutboxSnapshot, UploadThroughput};
use crate::sync::upload_observer::UploadObserverEvent;
use crate::sync::S3ConfigData;
use coven::ClockRef;
#[cfg(any(test, feature = "test-utils"))]
use coven::ExactCloudHome;

/// How a device join this library invited ended. The payloads coven returns
/// (the activation, the abandonment) are protocol records bae keeps nothing
/// from — the UI only distinguishes "the device is in" from "the attempt ended
/// without it".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceJoinOutcome {
    Joined,
    Abandoned,
}

use crate::sync::device_join_timing;

/// Run one of coven's device-join futures to completion and hand back its
/// `Send` result.
///
/// These futures are `!Send` (they hold coven's custody trait objects across
/// awaits) and deeply nested (each transport step polls the step under it), so
/// they get a dedicated thread with its own current-thread runtime and a large
/// stack rather than a blocking-pool thread — the same shape coven's own tests
/// use to drive them. `label` names the operation in a failure message.
async fn run_device_join_future<T, F, Fut>(label: &'static str, build: F) -> Result<T, LibraryError>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<T, LibraryError>>,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name(format!("bae-{label}"))
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    LibraryError::Internal(format!("failed to build {label} runtime: {e}"))
                })
                .and_then(|runtime| runtime.block_on(Box::pin(build())));
            // The receiver is gone only if the caller was dropped mid-join;
            // the work is already done, so there is nothing to report to.
            let _ = tx.send(result);
        })
        .map_err(|e| LibraryError::Internal(format!("failed to spawn {label} thread: {e}")))?;
    rx.await
        .map_err(|_| LibraryError::Internal(format!("{label} task failed to complete")))?
}

/// Owns the sync/upload state and the cloud-connection lifecycle. Holds clones of
/// the handles the sync paths need (coven handle, config, keys, clock, event bus,
/// database) plus the transient upload-pipeline state. Cloned alongside the
/// manager — every field is itself a clone-shared handle or `Arc`.
#[derive(Clone)]
pub(crate) struct SyncController {
    config_handle: Arc<ConfigHandle>,
    key_service: StoreKeys,
    /// This installation's clock, shared with the owning manager. coven's cloud
    /// homes take it for the OAuth sessions that refresh their own tokens.
    #[cfg(feature = "oauth-providers")]
    clock: ClockRef,
    outbox_values: tokio::sync::watch::Sender<Option<Result<OutboxSnapshot, String>>>,
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
        config_handle: Arc<ConfigHandle>,
        key_service: StoreKeys,
        clock: ClockRef,
        outbox_values: tokio::sync::watch::Sender<Option<Result<OutboxSnapshot, String>>>,
        database: Database,
        outbox_in_flight: Arc<Mutex<HashMap<String, u64>>>,
        upload_sessions: Arc<crate::library::UploadSessions>,
        upload_throughput: Arc<UploadThroughput>,
        sync_paused: Arc<AtomicBool>,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        diagnostics: Diagnostics,
    ) -> Self {
        #[cfg(not(feature = "oauth-providers"))]
        let _ = clock;
        Self {
            config_handle,
            key_service,
            #[cfg(feature = "oauth-providers")]
            clock,
            outbox_values,
            database,
            outbox_in_flight,
            upload_sessions,
            upload_throughput,
            sync_paused,
            cloudkit_ops,
            diagnostics,
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
        match self.database.master_key_fingerprint() {
            Ok(fingerprint) => fingerprint.is_some(),
            Err(error) => {
                warn!("failed to read master-key fingerprint: {error}");
                false
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn set_upload_progress_for_test(&self, file_id: &str, bytes_done: u64) {
        self.outbox_in_flight
            .lock()
            .unwrap()
            .insert(file_id.to_string(), bytes_done);
    }

    #[cfg(test)]
    pub(crate) fn clear_upload_progress_for_test(&self, file_id: &str) {
        self.outbox_in_flight.lock().unwrap().remove(file_id);
    }

    #[cfg(test)]
    pub(crate) fn upload_bytes_per_second_for_test(&self) -> u64 {
        self.upload_throughput.bytes_per_sec()
    }

    pub(crate) fn clear_upload_session(&self, release_id: &str) {
        self.upload_sessions.clear_group(Some(release_id));
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
            self.database.sync_now();
        }
        self.emit_outbox_changed().await;
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub(crate) fn is_sync_paused(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Build and publish the current outbox snapshot. Called at every outbox
    /// mutation, once per sync cycle, and on each upload lifecycle callback.
    pub(crate) async fn emit_outbox_changed(&self) {
        let value = self
            .build_outbox_snapshot()
            .await
            .map_err(|error| error.to_string());
        if let Err(error) = &value {
            warn!("Failed to build outbox snapshot: {error}");
        }
        self.outbox_values.send_replace(Some(value));
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary.
    pub(crate) async fn outbox_snapshot(
        &self,
    ) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        Ok(self.build_outbox_snapshot().await?)
    }

    async fn build_outbox_snapshot(&self) -> Result<OutboxSnapshot, coven::DbError> {
        let queue = self.database.outbox_queue().await?;
        let in_flight = { self.outbox_in_flight.lock().unwrap().clone() };
        let paused = self.is_sync_paused();
        Ok(crate::library::outbox_snapshot::build_outbox_snapshot(
            queue,
            &in_flight,
            &self.upload_sessions,
            &self.upload_throughput,
            paused,
        ))
    }

    pub(super) async fn process_upload_observer_event(&self, event: UploadObserverEvent) {
        match event {
            UploadObserverEvent::OutboxChanged => {}
            UploadObserverEvent::BlobUploaded {
                file_id,
                already_counted,
            } => self.record_uploaded_blob(&file_id, already_counted).await,
            UploadObserverEvent::ReleaseMadeRemote { release_id } => {
                self.upload_sessions.clear_group(Some(&release_id));
            }
            UploadObserverEvent::ReleaseMadeLocal => {}
        }
        self.emit_outbox_changed().await;
    }

    async fn record_uploaded_blob(&self, file_id: &str, already_counted: u64) {
        match self.database.find_file_by_id(file_id).await {
            Ok(Some(file)) => {
                let remaining = (file.file_size as u64).saturating_sub(already_counted);
                if remaining > 0 {
                    self.upload_throughput.record(remaining);
                }
                self.upload_sessions.record_done(
                    Some(file.release_id),
                    crate::library::upload_sessions::DoneFile {
                        file_id: file_id.to_string(),
                        display_name: file.original_filename,
                        bytes: file.file_size as u64,
                    },
                );
            }
            Ok(None) => {
                warn!("on_blob_uploaded: no file row for {file_id}; tallying unattributed");
                self.upload_sessions.record_done(
                    None,
                    crate::library::upload_sessions::DoneFile {
                        file_id: file_id.to_string(),
                        display_name: file_id.to_string(),
                        bytes: already_counted,
                    },
                );
            }
            Err(error) => warn!("on_blob_uploaded: looking up {file_id}: {error}"),
        }
    }

    /// The library's membership: its devices (with this device flagged, each
    /// member's fingerprint, and whether it can be removed) and whether the
    /// running device is an owner.
    pub(crate) async fn get_members(
        &self,
    ) -> Result<crate::sync::membership::Membership, LibraryError> {
        let members = self.database.get_members().await?;
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
            .database
            .invite_member(
                public_key_hex,
                provider_account_email,
                crate::sync::membership::MemberRole::Member,
            )
            .await?)
    }

    /// Mint the scannable device-join invite for a device that has asked to join,
    /// returning the payload bytes the UI renders as a QR code.
    ///
    /// `join_request_code` is the code the *joining* device produced with
    /// [`crate::sync::membership::generate_join_request`] and handed over first:
    /// the offer is signed for that device's key, so the owner cannot mint this
    /// payload without it. The returned bytes carry both the invite code (the
    /// joining device's provider credentials) and the join offer with its
    /// transport slots, so scanning it is everything that device needs.
    ///
    /// Minting only publishes the offer. [`drive_device_join`](Self::drive_device_join)
    /// must then run on this device to admit the joiner.
    pub(crate) async fn begin_device_invite(
        &self,
        join_request_code: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        let database = self.database.clone();
        let join_request_code = join_request_code.to_string();
        run_device_join_future("device-invite", move || async move {
            Ok(database
                .begin_device_invite(
                    &join_request_code,
                    crate::sync::membership::MemberRole::Member,
                )
                .await?
                .to_bytes())
        })
        .await
    }

    /// Drive this device's side of a join it invited, to the attempt's end.
    ///
    /// Runs the admitting half of the handshake — approving the joiner's provider
    /// access, registering it, and finalizing its activation — publishing each
    /// artifact through the transport and waiting for the joiner's next one. The
    /// approval is [`AutoApproveSelfIssued`](coven::DeviceJoinApprovalPolicy::AutoApproveSelfIssued):
    /// the attempt is one this device itself minted, and the user already approved
    /// it by choosing to show the QR — coven still refuses any request that does
    /// not match that attempt.
    ///
    /// Returns when the attempt reaches a terminal state this side owns; the
    /// joining device completes its own last step. A joining device that never
    /// scans the code leaves this waiting until the transport deadline, which
    /// surfaces as [`LibraryError::Sync`].
    pub(crate) async fn drive_device_join(
        &self,
        invite_bytes: Vec<u8>,
    ) -> Result<DeviceJoinOutcome, LibraryError> {
        let database = self.database.clone();
        run_device_join_future("device-join drive", move || async move {
            let invite = coven::DeviceJoinInvite::from_bytes(&invite_bytes)
                .map_err(|e| LibraryError::Internal(format!("invalid device invite: {e}")))?;
            let outcome = database
                .drive_device_join(
                    &invite,
                    coven::DeviceJoinApprovalPolicy::AutoApproveSelfIssued,
                    device_join_timing(),
                )
                .await?;
            Ok(match outcome {
                coven::DeviceJoinDriveOutcome::Activated(_) => DeviceJoinOutcome::Joined,
                coven::DeviceJoinDriveOutcome::Abandoned(_) => DeviceJoinOutcome::Abandoned,
            })
        })
        .await
    }

    /// Withdraw an invite this device minted, unwinding the attempt through the
    /// transport so a joining device that already scanned it is told to stop.
    pub(crate) async fn cancel_device_invite(
        &self,
        invite_bytes: Vec<u8>,
    ) -> Result<(), LibraryError> {
        let database = self.database.clone();
        run_device_join_future("device-invite cancel", move || async move {
            let invite = coven::DeviceJoinInvite::from_bytes(&invite_bytes)
                .map_err(|e| LibraryError::Internal(format!("invalid device invite: {e}")))?;
            database
                .cancel_device_invite(&invite, device_join_timing())
                .await?;
            Ok(())
        })
        .await
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
        let database = self.database.clone();
        let public_key_hex = public_key_hex.to_string();
        let fingerprint = tokio::task::spawn_blocking(move || -> Result<String, LibraryError> {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| {
                    LibraryError::Internal(format!("failed to build member-removal runtime: {e}"))
                })?
                .block_on(database.remove_member(&public_key_hex))
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

        // The cloud-home settings the form proposes, decided once here so the
        // probe reaches exactly the home the library would keep.
        let mut proposed = self.config_handle.config().cloud_home.clone();
        proposed.provider = Some(CloudProvider::S3);
        proposed.s3_bucket = Some(data.bucket);
        proposed.s3_region = Some(data.region);
        proposed.s3_endpoint = data.endpoint.filter(|s| !s.is_empty());
        proposed.s3_key_prefix = data.key_prefix.filter(|s| !s.is_empty());
        proposed.storage = data.storage;
        // Probe the bucket before the library records the provider: a typo or a
        // missing bucket would otherwise leave the UI showing "Connected", with
        // the user learning sync is broken only from the reconnect banner after
        // the first failed cycle. The probe's typed outcome — bad
        // credentials/bucket vs unreachable endpoint — reaches the UI as distinct
        // error classes.
        //
        // coven builds a cloud home from a store's config plus the credentials in
        // its key service, so the proposed credentials go into the keyring first
        // and the probe runs against a config that exists only here, in memory. A
        // failed probe puts the previous credentials back, so the library is left
        // exactly as it was found.
        let previous_credentials = self.key_service.get_cloud_home_credentials()?;
        self.key_service
            .set_cloud_home_credentials(&CloudHomeCredentials::S3 {
                access_key: data.access_key,
                secret_key: data.secret_key,
            })?;
        if let Err(probe_failure) = self.probe_cloud_home(&proposed).await {
            let restored = match previous_credentials {
                Some(previous) => self.key_service.set_cloud_home_credentials(&previous),
                None => self.key_service.delete_cloud_home_credentials(),
            };
            // The credentials the probe rejected are still in the keyring and the
            // keyring itself is refusing writes, so nothing here can put the
            // library back. Report that over the probe failure — it is the state
            // the user has to fix first.
            restored.map_err(|error| {
                coven::KeyError::Custody {
                    operation: "restore previous S3 credentials after the proposed credentials failed their probe",
                    source: Box::new(error),
                }
            })?;
            return Err(probe_failure);
        }

        self.config_handle
            .update(move |c| c.cloud_home = proposed)?;

        self.ensure_sync_manager_and_start().await?;
        info!("Saved S3 sync configuration");
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::S3,
            });
        Ok(())
    }

    /// Reach `home` with the credentials already staged in the key service. The
    /// config it probes against exists only for the call — the stored one still
    /// names whatever provider the library had.
    async fn probe_cloud_home(&self, home: &coven::CloudHomeConfig) -> Result<(), LibraryError> {
        let mut probe_config = self.config_handle.config().to_coven();
        probe_config.cloud_home = home.clone();
        self.database.probe_cloud_home(&probe_config).await?;
        Ok(())
    }

    /// OAuth sign-in + persist for a browsable/opaque provider, then connect. The
    /// manager re-emits every album after this returns.
    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn sign_in_cloud_provider(
        &self,
        provider: CloudProvider,
        storage: crate::config::HomeStorage,
    ) -> Result<(), LibraryError> {
        // The OAuth applications this installation registered at startup: coven
        // ships none, so the sign-in runs under bae's own client credentials.
        let oauth_clients = crate::oauth::clients();

        // Hold the sender alive across the await so the cancel wait inside
        // `oauth::authorize` never fires — this fn surfaces no cancel signal. If this
        // future is dropped, `oauth::authorize`'s own AbortOnDrop guard tears the
        // listener task down.
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let library_name = self.config_handle.config().store_name.clone();
        let clock = self.clock.as_ref();
        // coven's sign-ins authorize, resolve the cloud folder, and save tokens to
        // the keyring, returning the folder identifiers; bae persists them here.
        match provider {
            CloudProvider::GoogleDrive => {
                let folder_id = oauth_clients
                    .sign_in_google_drive(&self.key_service, &library_name, cancel_rx, clock)
                    .await
                    .map_err(|e| LibraryError::CloudSetup(e.to_string()))?;
                self.config_handle.update(move |c| {
                    c.cloud_home.provider = Some(CloudProvider::GoogleDrive);
                    c.cloud_home.google_drive_folder_id = Some(folder_id);
                    c.cloud_home.storage = storage;
                })?;
            }
            CloudProvider::Dropbox => {
                let folder_path = oauth_clients
                    .sign_in_dropbox(&self.key_service, &library_name, cancel_rx, clock)
                    .await
                    .map_err(|e| LibraryError::CloudSetup(e.to_string()))?;
                self.config_handle.update(move |c| {
                    c.cloud_home.provider = Some(CloudProvider::Dropbox);
                    c.cloud_home.dropbox_folder_path = Some(folder_path);
                    c.cloud_home.storage = storage;
                })?;
            }
            CloudProvider::OneDrive => {
                let (drive_id, folder_id) = oauth_clients
                    .sign_in_onedrive(&self.key_service, cancel_rx, clock)
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
        self.database.disconnect_sync();

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
        self.connect_provider().await?;
        Ok(())
    }

    /// Connect the configured provider: CloudKit needs its host-supplied driver
    /// handed in, every other provider is built by coven from the config alone.
    /// coven resolves the at-rest cipher from the master-key custody itself, so no
    /// key material passes through here.
    async fn connect_provider(&self) -> Result<(), LibraryError> {
        // Read the provider out before the awaits below: `config()` hands back a read
        // guard, and holding one across an await makes the future !Send — which the
        // bridge's uniffi export requires.
        let provider = self.config_handle.config().cloud_home.provider.clone();
        match provider {
            Some(CloudProvider::CloudKit) => {
                let ops = self.cloudkit_ops.clone().ok_or_else(|| {
                    LibraryError::Internal("CloudKit driver not provided".to_string())
                })?;
                self.database.connect_sync_with_cloudkit(ops).await?;
            }
            _ => {
                self.database.connect_sync().await?;
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
        cloud_home: Arc<dyn ExactCloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), LibraryError> {
        self.establish_test_store_security(&cipher)?;
        self.database
            .connect_sync_with_test_home(cloud_home, cipher)
            .await?;
        Ok(())
    }

    /// Connect over an injected cloud home the way
    /// [`connect_test_cloud_home`](Self::connect_test_cloud_home) does, but with
    /// no sync loop behind it: the caller's own `drain_uploads_for_test` is the
    /// only thing that drains the upload queue.
    ///
    /// A running loop drains every cycle, so a test that also drains explicitly
    /// has two drainers on one queue and reads whichever answer the race leaves
    /// it. Without the loop the test's drain is the whole truth. What the loop
    /// would have done — publishing a transition's Store write, which is what
    /// finishes a make-Remote and gives a host-provided blob its cloud locator —
    /// does not happen here, and `is_sync_ready` stays false, so a test that
    /// needs either keeps the loop-driven connect.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) async fn connect_test_cloud_home_caller_driven(
        &self,
        cloud_home: Arc<dyn ExactCloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), LibraryError> {
        self.establish_test_store_security(&cipher)?;
        self.database
            .connect_sync_with_test_home_caller_driven(cloud_home, cipher)
            .await?;
        Ok(())
    }

    /// Establish the security state an injected test home requires through the
    /// database that owns its coven handle. Every home needs a device identity;
    /// an encrypted home also needs routing encryption.
    #[cfg(any(test, feature = "test-utils"))]
    fn establish_test_store_security(
        &self,
        cipher: &crate::sync::CloudCipher,
    ) -> Result<(), LibraryError> {
        self.database.establish_test_identity()?;
        if matches!(cipher, crate::sync::CloudCipher::Encrypted(_)) {
            self.database.establish_test_master_key()?;
        }
        Ok(())
    }

    /// Ensure a sync manager exists — minting the encryption key if the home needs
    /// one — and start the sync loop.
    async fn ensure_sync_manager_and_start(&self) -> Result<(), LibraryError> {
        if self.database.is_connected() {
            self.database.start_sync().await?;
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
            Some(match self.database.master_key_fingerprint()? {
                Some(fingerprint) => fingerprint,
                None => self.database.initialize_master_key()?,
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
        self.connect_provider().await?;

        self.database.sync_now();

        Ok(())
    }
}
