//! The cloud-sync responsibility extracted from [`LibraryManager`]: the upload
//! pipeline (outbox in-flight + throughput + pause), the connection lifecycle and
//! provider configuration, and the membership operations.
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
use crate::library::{LibraryError, OutboxSnapshot, UploadThroughput};
use crate::sync::S3ConfigData;
#[cfg(any(test, feature = "test-utils"))]
use coven::ExactCloudHome;

use crate::sync::device_join_timing;

/// Owns the sync/upload state and the cloud-connection lifecycle. Holds clones of
/// the handles the sync paths need (config, database, and diagnostics) plus the
/// transient upload-pipeline state. Cloned alongside the
/// manager — every field is itself a clone-shared handle or `Arc`.
#[derive(Clone)]
pub(crate) struct SyncController {
    config_handle: Arc<ConfigHandle>,
    outbox_values: tokio::sync::watch::Sender<Option<Result<OutboxSnapshot, String>>>,
    /// Serializes every durable/transient projection and numbers publications
    /// in the order they reach subscribers. Each projection rereads the current
    /// durable queue while holding this lock, so a delayed trigger cannot
    /// overwrite a newer value with an older queue snapshot.
    outbox_projection_revision: Arc<tokio::sync::Mutex<u64>>,
    database: Database,
    /// Exact blob-bearing rows with preparation or provider work in flight,
    /// mapped to buffer-cadence progress. Shared with the sync loop's
    /// `ReleaseUploadObserver` and read by `outbox_snapshot`.
    transient_uploads: Arc<
        Mutex<
            HashMap<
                crate::library::outbox_snapshot::UploadBlobKey,
                crate::library::outbox_snapshot::TransientUploadState,
            >,
        >,
    >,
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
        outbox_values: tokio::sync::watch::Sender<Option<Result<OutboxSnapshot, String>>>,
        outbox_projection_revision: Arc<tokio::sync::Mutex<u64>>,
        database: Database,
        transient_uploads: Arc<
            Mutex<
                HashMap<
                    crate::library::outbox_snapshot::UploadBlobKey,
                    crate::library::outbox_snapshot::TransientUploadState,
                >,
            >,
        >,
        upload_throughput: Arc<UploadThroughput>,
        sync_paused: Arc<AtomicBool>,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        diagnostics: Diagnostics,
    ) -> Self {
        Self {
            config_handle,
            outbox_values,
            outbox_projection_revision,
            database,
            transient_uploads,
            upload_throughput,
            sync_paused,
            cloudkit_ops,
            diagnostics,
        }
    }

    pub(crate) fn cloud_home_key_state(&self) -> Result<coven::CloudHomeKeyState, LibraryError> {
        if self.config_handle.config().cloud_home.provider.is_none() {
            return Ok(coven::CloudHomeKeyState::NotRequired);
        }
        Ok(self
            .database
            .cloud_home_key_state(self.config_handle.config().cloud_home.storage)?)
    }

    #[cfg(test)]
    pub(crate) fn clear_transient_upload_for_test(&self, file_id: &str) {
        self.transient_uploads.lock().unwrap().remove(
            &crate::library::outbox_snapshot::UploadBlobKey::new(
                crate::sync::RELEASE_FILES_NAMESPACE,
                file_id,
            ),
        );
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

    /// Build and publish the current outbox snapshot. Called by durable outbox
    /// and display-row subscriptions and by each upload lifecycle callback.
    pub(crate) async fn emit_outbox_changed(&self) -> u64 {
        let mut revision = self.outbox_projection_revision.lock().await;
        *revision = revision
            .checked_add(1)
            .expect("outbox projection revision overflow");
        let published_revision = *revision;
        let value = self
            .build_outbox_snapshot()
            .await
            .map(|mut snapshot| {
                snapshot.revision = published_revision;
                snapshot
            })
            .map_err(|error| error.to_string());
        if let Err(error) = &value {
            warn!("Failed to build outbox snapshot: {error}");
        }
        self.outbox_values.send_replace(Some(value));
        published_revision
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary.
    pub(crate) async fn outbox_snapshot(
        &self,
    ) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        let revision = self.outbox_projection_revision.lock().await;
        let mut snapshot = self.build_outbox_snapshot().await?;
        snapshot.revision = *revision;
        Ok(snapshot)
    }

    async fn build_outbox_snapshot(&self) -> Result<OutboxSnapshot, coven::DbError> {
        let queue = self.database.outbox_queue().await?;
        self.retain_current_transient_uploads(&queue.uploads);
        Ok(self.build_outbox_snapshot_from_queue(queue))
    }

    fn build_outbox_snapshot_from_queue(&self, queue: crate::db::DbOutboxQueue) -> OutboxSnapshot {
        let transient = { self.transient_uploads.lock().unwrap().clone() };
        let paused = self.is_sync_paused();
        crate::library::outbox_snapshot::build_outbox_snapshot(
            queue,
            &transient,
            &self.upload_throughput,
            paused,
        )
    }

    pub(super) async fn process_upload_observer_event(&self) {
        self.emit_outbox_changed().await;
    }

    pub(super) async fn run_cloud_outbox_subscription(
        &self,
        mut subscription: coven::CloudOutboxLiveQuery,
    ) {
        let mut display = self.database.subscribe_outbox_display(Default::default());
        let display_requests = display.requests();
        loop {
            tokio::select! {
                durable = subscription.next() => match durable {
                    Ok(snapshot) => {
                        match Database::outbox_display_request(&snapshot) {
                            Ok(request) => {
                                display_requests
                                    .set(request)
                                    .expect("the outbox display subscription is retained");
                                self.emit_outbox_changed().await;
                            }
                            Err(error) => {
                                warn!(%error, "Failed to identify durable outbox display rows");
                                self.outbox_values
                                    .send_replace(Some(Err(error.to_string())));
                            }
                        }
                    }
                    Err(error) => {
                        warn!(%error, "Failed to read the durable cloud outbox");
                        self.outbox_values
                            .send_replace(Some(Err(error.to_string())));
                    }
                },
                event = display.next() => match event.into_result() {
                    Ok(_) => {
                        self.emit_outbox_changed().await;
                    }
                    Err(error) => {
                        warn!(%error, "Failed to read durable outbox display rows");
                        self.outbox_values
                            .send_replace(Some(Err(error.to_string())));
                    }
                },
            }
        }
    }

    /// Drop callback facts when the durable outbox proves their phase has been
    /// superseded. `Created` deliberately retains provider progress: coven
    /// commits that handoff before calling `on_blob_uploaded`, and that callback
    /// owns validation and removal of the exact final byte report.
    fn retain_current_transient_uploads(&self, uploads: &[crate::db::DbOutboxUpload]) {
        let durable: HashMap<_, _> = uploads
            .iter()
            .map(|upload| {
                (
                    crate::library::outbox_snapshot::UploadBlobKey::from_row(&upload.blob),
                    upload.phase,
                )
            })
            .collect();
        self.transient_uploads
            .lock()
            .unwrap()
            .retain(|key, transient| match durable.get(key) {
                None => false,
                Some(coven::QueuedUploadPhase::Created) => true,
                Some(coven::QueuedUploadPhase::Prepared) => !matches!(
                    transient,
                    crate::library::outbox_snapshot::TransientUploadState::Preparing { .. }
                ),
                Some(coven::QueuedUploadPhase::Pending) => true,
            });
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

    /// Mint the scannable device-join invite for a device that has asked to join,
    /// returning the payload bytes the UI renders as a QR code.
    ///
    /// `join_request_code` is the code the *joining* device produced with
    /// [`crate::sync::membership::generate_join_request`] and handed over first:
    /// the offer is signed for that device's key, so the owner cannot mint this
    /// payload without it. The returned bytes carry the joining device's sealed
    /// provider credentials and the join offer with its transport slots, so
    /// scanning it is everything that device needs.
    ///
    /// Minting only publishes the offer. [`drive_device_join`](Self::drive_device_join)
    /// must then run on this device to admit the joiner.
    pub(crate) async fn begin_device_invite(
        &self,
        join_request_code: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        Ok(self
            .database
            .begin_device_invite(
                join_request_code,
                crate::sync::membership::MemberRole::Member,
            )
            .await?
            .to_bytes())
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
    ) -> Result<(), LibraryError> {
        let invite = coven::DeviceJoinInvite::from_bytes(&invite_bytes)
            .map_err(|e| LibraryError::Internal(format!("invalid device invite: {e}")))?;
        let outcome = self
            .database
            .drive_device_join(
                &invite,
                coven::DeviceJoinApprovalPolicy::AutoApproveSelfIssued,
                device_join_timing(),
            )
            .await?;
        match outcome {
            coven::DeviceJoinDriveOutcome::Activated(_) => Ok(()),
            coven::DeviceJoinDriveOutcome::Abandoned(_) => Err(LibraryError::DeviceJoinAbandoned),
        }
    }

    /// Withdraw an invite this device minted, unwinding the attempt through the
    /// transport so a joining device that already scanned it is told to stop.
    pub(crate) async fn cancel_device_invite(
        &self,
        invite_bytes: Vec<u8>,
    ) -> Result<(), LibraryError> {
        let invite = coven::DeviceJoinInvite::from_bytes(&invite_bytes)
            .map_err(|e| LibraryError::Internal(format!("invalid device invite: {e}")))?;
        self.database
            .cancel_device_invite(&invite, device_join_timing())
            .await?;
        Ok(())
    }

    /// Remove a device from the library and rotate the library key so the removed
    /// device can no longer read new data.
    pub(crate) async fn remove_member(&self, public_key_hex: &str) -> Result<(), LibraryError> {
        self.database.remove_member(public_key_hex).await?;
        Ok(())
    }

    /// Connect an S3 cloud home, then persist the completed config Coven returns.
    pub(crate) async fn save_s3_config(&self, data: S3ConfigData) -> Result<(), LibraryError> {
        let mut proposed = self.config_handle.config().cloud_home.clone();
        proposed.provider = Some(CloudProvider::S3);
        proposed.s3_bucket = Some(data.bucket);
        proposed.s3_region = Some(data.region);
        proposed.s3_endpoint = data.endpoint.filter(|s| !s.is_empty());
        proposed.s3_key_prefix = data.key_prefix.filter(|s| !s.is_empty());
        proposed.storage = data.storage;
        let connected = self
            .database
            .setup_s3_cloud_home(proposed, data.access_key, data.secret_key)
            .await?;
        self.config_handle
            .update(move |config| config.cloud_home = connected.cloud_home)?;
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
    ) -> Result<(), LibraryError> {
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        match provider {
            CloudProvider::GoogleDrive | CloudProvider::Dropbox | CloudProvider::OneDrive => {}
            _ => {
                return Err(LibraryError::Internal(
                    "provider does not use OAuth sign-in".to_string(),
                ));
            }
        }
        let mut proposed = self.config_handle.config().cloud_home.clone();
        proposed.provider = Some(provider.clone());
        proposed.storage = storage;
        let connected = self
            .database
            .setup_oauth_cloud_home(proposed, cancel_rx)
            .await?;
        self.config_handle
            .update(move |config| config.cloud_home = connected.cloud_home)?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected { provider });
        Ok(())
    }

    /// Connect CloudKit, then persist the completed config Coven returns.
    pub(crate) async fn use_cloudkit(
        &self,
        storage: crate::config::HomeStorage,
    ) -> Result<(), LibraryError> {
        let mut proposed = self.config_handle.config().cloud_home.clone();
        proposed.provider = Some(CloudProvider::CloudKit);
        proposed.storage = storage;
        proposed.cloudkit_owner_name = None;
        proposed.cloudkit_zone_name = None;
        let ops = self
            .cloudkit_ops
            .clone()
            .ok_or_else(|| LibraryError::Internal("CloudKit driver not provided".to_string()))?;
        let connected = self
            .database
            .setup_cloudkit_cloud_home(proposed, ops)
            .await?;
        self.config_handle
            .update(move |config| config.cloud_home = connected.cloud_home)?;
        info!("Configured CloudKit cloud provider");
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::CloudKit,
            });
        Ok(())
    }

    /// Ask Coven to stop the sync loop and forget the cloud-home credentials,
    /// then clear bae's cloud-home config. The manager re-emits every album
    /// (storage actions lost) after this returns.
    pub(crate) async fn disconnect_cloud_provider(&self) -> Result<(), LibraryError> {
        // Capture the provider before the config clear below drops it, so the
        // telemetry names which provider was disconnected.
        let provider = self.config_handle.config().cloud_home.provider.clone();

        self.database.disconnect_cloud_home().await?;
        self.config_handle
            .update(|c| c.cloud_home = Default::default())?;
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

    pub(crate) async fn unlock_cloud_home(
        &self,
        serialized_master_key: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .unlock_cloud_home(serialized_master_key)
            .await?;
        Ok(())
    }

    pub(crate) async fn forget_master_key(&self) -> Result<(), LibraryError> {
        self.database.forget_master_key().await?;
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
        self.establish_test_identity()?;
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
        self.establish_test_identity()?;
        self.database
            .connect_sync_with_test_home_caller_driven(cloud_home, cipher)
            .await?;
        Ok(())
    }

    /// Establish the device identity an injected test home requires through the
    /// database that owns its Coven handle. Coven prepares any missing master
    /// key together with the injected connection.
    #[cfg(any(test, feature = "test-utils"))]
    fn establish_test_identity(&self) -> Result<(), LibraryError> {
        self.database.establish_test_identity()?;
        Ok(())
    }
}
