//! The cloud-sync responsibility extracted from [`LibraryManager`]: the upload
//! pipeline's projection and pause command (over the shared
//! [`LiveUploads`](crate::library::live_uploads::LiveUploads) the observer
//! writes), the connection lifecycle and provider configuration, and the
//! membership operations.
//!
//! `LibraryManager` holds one `SyncController` and delegates its public sync API
//! to it. The controller never references the manager back.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::config::{CloudProvider, ConfigHandle};
use crate::db::{Database, DbOutboxQueue, OutboxDisplayContext, OutboxDisplayRequest};
use crate::diagnostics::{Diagnostics, TelemetryEvent};
use crate::library::live_uploads::LiveUploads;
use crate::library::{LibraryError, OutboxSnapshot};
use crate::sync::S3ConfigData;
#[cfg(any(test, feature = "test-utils"))]
use coven::ExactCloudHome;

/// Owns the outbox projection and the cloud-connection lifecycle. Holds clones
/// of the handles the sync paths need (config, database, and diagnostics) plus
/// the live upload state. Cloned alongside the manager — every field is itself
/// a clone-shared handle or `Arc`.
#[derive(Clone)]
pub(crate) struct SyncController {
    config_handle: Arc<ConfigHandle>,
    outbox_values: tokio::sync::watch::Sender<Option<Result<OutboxSnapshot, String>>>,
    /// Serializes every outbox publication and numbers them in the order they
    /// reach subscribers, and holds the durable queue each one is built from.
    outbox: Arc<tokio::sync::Mutex<OutboxProjection>>,
    /// How many times the projection has read coven's durable queue itself
    /// rather than taking the live query's delivery. The durable reader notes
    /// this count before each wait, so a delivery that a read may have
    /// overtaken is recognised and not published over it.
    outbox_reads: Arc<AtomicU64>,
    database: Database,
    /// In-flight bytes, rate, and pause state of the upload pipeline, shared
    /// with the sync loop's `ReleaseUploadObserver`, which writes them. This
    /// side reads them into every outbox snapshot and drives the pause.
    uploads: LiveUploads,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    /// Typed telemetry sink, shared with the owning manager. The
    /// provider-connect/disconnect completions emit through it.
    diagnostics: Diagnostics,
}

/// What the outbox stream is built from, under the one lock that numbers its
/// values.
#[derive(Default)]
pub(crate) struct OutboxProjection {
    revision: u64,
    /// The newest durable queue coven delivered or was read for, with the
    /// display rows it needs labelled.
    durable: Option<(coven::CloudOutboxSnapshot, OutboxDisplayRequest)>,
    /// The display query's latest answer and the request it answered.
    names: Option<(OutboxDisplayRequest, OutboxDisplayContext)>,
    /// The newest durable queue joined to its display names — what a live
    /// upload change is republished over.
    queue: Option<DbOutboxQueue>,
    /// Points the display query at the rows the held durable queue needs;
    /// present once the outbox subscription runs.
    display_requests: Option<coven::LiveQueryRequests<OutboxDisplayRequest>>,
}

impl OutboxProjection {
    fn hold_durable(
        &mut self,
        snapshot: coven::CloudOutboxSnapshot,
        request: OutboxDisplayRequest,
    ) {
        if let Some(requests) = &self.display_requests {
            requests
                .set(request.clone())
                .expect("the outbox display subscription is retained");
        }
        self.durable = Some((snapshot, request));
    }
}

impl SyncController {
    pub(crate) fn new(
        config_handle: Arc<ConfigHandle>,
        database: Database,
        uploads: LiveUploads,
        cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
        diagnostics: Diagnostics,
    ) -> Self {
        let (outbox_values, _) = tokio::sync::watch::channel(None);
        Self {
            config_handle,
            outbox_values,
            outbox: Arc::new(tokio::sync::Mutex::new(OutboxProjection::default())),
            outbox_reads: Arc::new(AtomicU64::new(0)),
            database,
            uploads,
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
    pub(crate) fn clear_upload_observation_for_test(&self, file_id: &str) {
        self.uploads
            .clear_release_file_observation_for_test(file_id);
    }

    /// Pause or resume the cloud-upload pipeline. New enqueues still land in
    /// the outbox; coven suspends active preparation/provider futures and keeps
    /// their open upload sessions for resume.
    pub(crate) async fn set_sync_paused(&self, paused: bool) {
        // The outbox projection sees the pause through the live upload state
        // and republishes.
        self.uploads.set_paused(paused);
        if !paused {
            // Kick the loop so the queue starts draining immediately on resume
            // rather than waiting for the next idle tick.
            self.database.sync_now();
        }
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub(crate) fn is_sync_paused(&self) -> bool {
        self.uploads.is_paused()
    }

    /// The stream the outbox pane and upload standing read; each value is the
    /// latest snapshot or the failure that kept one from being built.
    pub(crate) fn subscribe_outbox_values(
        &self,
    ) -> tokio::sync::watch::Receiver<Option<Result<OutboxSnapshot, String>>> {
        self.outbox_values.subscribe()
    }

    /// Read coven's durable queue now and publish the snapshot built from it.
    /// A command that changed the queue calls this so the value it hands back
    /// a revision for already shows its change; the live query's delivery of
    /// the same change follows.
    pub(crate) async fn emit_outbox_changed(&self) -> u64 {
        let mut projection = self.outbox.lock().await;
        self.read_and_publish(&mut projection).await
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary.
    pub(crate) async fn outbox_snapshot(
        &self,
    ) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        let projection = self.outbox.lock().await;
        let queue = self.database.outbox_queue().await?;
        let mut snapshot = self.uploads.outbox_snapshot(queue);
        snapshot.revision = projection.revision;
        Ok(snapshot)
    }

    #[cfg(test)]
    pub(crate) async fn hold_outbox_projection_for_test(
        &self,
    ) -> tokio::sync::OwnedMutexGuard<OutboxProjection> {
        self.outbox.clone().lock_owned().await
    }

    /// Keep the outbox stream current: coven's durable queue as its live query
    /// delivers it, the display rows that label that queue, and every change
    /// to the live upload state. Each value is built from what was delivered
    /// and what the live state holds; the database is read again only where a
    /// delivery cannot be trusted to be the newest.
    pub(super) async fn run_cloud_outbox_subscription(
        &self,
        mut subscription: coven::CloudOutboxLiveQuery,
    ) {
        let mut display = self.database.subscribe_outbox_display(Default::default());
        self.outbox.lock().await.display_requests = Some(display.requests());
        let mut live = self.uploads.subscribe_changes();
        let (durable_tx, mut durable_rx) = tokio::sync::mpsc::unbounded_channel();
        let (names_tx, mut names_rx) = tokio::sync::mpsc::unbounded_channel();
        let outbox_reads = self.outbox_reads.clone();
        // Neither live query can sit in the `select!` below: dropping
        // `CloudOutboxLiveQuery::next` mid-read loses the change it was woken
        // for, and dropping `ReconfigurableLiveQuery::next` mid-read throws the
        // read away and starts it over, so upload ticks arriving faster than
        // one read would keep the display names from ever landing. Each query
        // runs in a loop of its own that nothing races and hands its results
        // over a channel, whose receive loses nothing when it is dropped.
        let read_durable = async move {
            loop {
                let reads_before = outbox_reads.load(Ordering::Acquire);
                let delivered = subscription.next().await;
                if durable_tx.send((reads_before, delivered)).is_err() {
                    return;
                }
            }
        };
        let read_names = async move {
            loop {
                let event = display.next().await;
                let request = event.request().clone();
                if names_tx.send((request, event.into_result())).is_err() {
                    return;
                }
            }
        };
        let project = async {
            loop {
                tokio::select! {
                    Some((reads_before, delivered)) = durable_rx.recv() => {
                        self.durable_delivered(reads_before, delivered).await;
                    }
                    Some((request, names)) = names_rx.recv() => {
                        self.names_answered(request, names).await;
                    }
                    changed = live.changed() => {
                        changed.expect("the sync controller retains the live upload state");
                        self.publish_live().await;
                    }
                }
            }
        };
        tokio::join!(read_durable, read_names, project);
    }

    async fn durable_delivered(
        &self,
        reads_before: u64,
        delivered: Result<coven::CloudOutboxSnapshot, coven::DbError>,
    ) {
        let mut projection = self.outbox.lock().await;
        let snapshot = match delivered {
            Ok(snapshot) => snapshot,
            Err(error) => {
                warn!(%error, "Failed to read the durable cloud outbox");
                self.publish(&mut projection, Err(error.to_string()));
                return;
            }
        };
        // A read of its own since the reader started waiting may have seen a
        // newer queue than this delivery; reading again settles which is
        // newest.
        if self.outbox_reads.load(Ordering::Acquire) != reads_before {
            debug!("an outbox read overtook this delivery; reading the durable queue again");
            self.read_and_publish(&mut projection).await;
            return;
        }
        match Database::outbox_display_request(&snapshot) {
            Ok(request) => {
                projection.hold_durable(snapshot, request);
                self.publish_if_labelled(&mut projection);
            }
            Err(error) => {
                warn!(%error, "Failed to identify durable outbox display rows");
                self.publish(&mut projection, Err(error.to_string()));
            }
        }
    }

    async fn names_answered(
        &self,
        request: OutboxDisplayRequest,
        names: coven::CovenResult<OutboxDisplayContext>,
    ) {
        let mut projection = self.outbox.lock().await;
        match names {
            Ok(names) => {
                projection.names = Some((request, names));
                self.publish_if_labelled(&mut projection);
            }
            Err(error) => {
                warn!(%error, "Failed to read durable outbox display rows");
                self.publish(&mut projection, Err(error.to_string()));
            }
        }
    }

    /// Republish after the live upload state changed, over the durable queue
    /// already held.
    async fn publish_live(&self) {
        let mut projection = self.outbox.lock().await;
        match projection.queue.clone() {
            Some(queue) => {
                let snapshot = self.uploads.outbox_snapshot(queue);
                self.publish(&mut projection, Ok(snapshot));
            }
            None => {
                debug!("no durable outbox delivered yet; reading it for a live upload change");
                self.read_and_publish(&mut projection).await;
            }
        }
    }

    /// Join the held durable snapshot to its display names and publish, once
    /// the display query has answered for the request that snapshot needs.
    fn publish_if_labelled(&self, projection: &mut OutboxProjection) {
        let (Some((snapshot, request)), Some((named, names))) =
            (&projection.durable, &projection.names)
        else {
            return;
        };
        if request != named {
            debug!("durable outbox waits for the display rows of its new request");
            return;
        }
        match Database::outbox_queue_from_context(snapshot.clone(), names.clone()) {
            Ok(queue) => {
                projection.queue = Some(queue.clone());
                let snapshot = self.uploads.outbox_snapshot(queue);
                self.publish(projection, Ok(snapshot));
            }
            Err(error) => {
                warn!(%error, "Failed to label the durable cloud outbox");
                self.publish(projection, Err(error.to_string()));
            }
        }
    }

    /// Read coven's durable queue and its display names directly, hold them,
    /// and publish. Counted in `outbox_reads` once the read is done, so any
    /// delivery whose wait began before it is recognised as possibly older.
    async fn read_and_publish(&self, projection: &mut OutboxProjection) -> u64 {
        let read = self.database.outbox_queue_parts().await;
        self.outbox_reads.fetch_add(1, Ordering::AcqRel);
        match read {
            Ok((snapshot, request, names)) => {
                projection.hold_durable(snapshot.clone(), request);
                match Database::outbox_queue_from_context(snapshot, names) {
                    Ok(queue) => {
                        projection.queue = Some(queue.clone());
                        let snapshot = self.uploads.outbox_snapshot(queue);
                        self.publish(projection, Ok(snapshot))
                    }
                    Err(error) => {
                        warn!(%error, "Failed to label the durable cloud outbox");
                        self.publish(projection, Err(error.to_string()))
                    }
                }
            }
            Err(error) => {
                warn!(%error, "Failed to read the durable cloud outbox");
                self.publish(projection, Err(error.to_string()))
            }
        }
    }

    /// Number and send one outbox value.
    fn publish(
        &self,
        projection: &mut OutboxProjection,
        value: Result<OutboxSnapshot, String>,
    ) -> u64 {
        projection.revision = projection
            .revision
            .checked_add(1)
            .expect("outbox projection revision overflow");
        let revision = projection.revision;
        let value = value.map(|mut snapshot| {
            snapshot.revision = revision;
            snapshot
        });
        self.outbox_values.send_replace(Some(value));
        revision
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

    pub(crate) async fn start_device_pairing(
        &self,
    ) -> Result<crate::library::DevicePairingSession, LibraryError> {
        let host = self.database.start_device_pairing().await?;
        Ok(crate::library::DevicePairingSession::new(
            self.database.clone(),
            host,
        ))
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
            .update_store_off_runtime(move |config| config.cloud_home = connected.cloud_home)
            .await?;
        info!("Saved S3 sync configuration");
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::S3,
            });
        Ok(())
    }

    /// OAuth sign-in + persist for a browsable/opaque provider, then connect.
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
            .update_store_off_runtime(move |config| config.cloud_home = connected.cloud_home)
            .await?;
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
            .update_store_off_runtime(move |config| config.cloud_home = connected.cloud_home)
            .await?;
        info!("Configured CloudKit cloud provider");
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::CloudKit,
            });
        Ok(())
    }

    /// Ask Coven to stop the sync loop and forget the cloud-home credentials,
    /// then clear bae's cloud-home config. The status stream then says the
    /// library is disconnected, which every view re-resolves on.
    pub(crate) async fn disconnect_cloud_provider(&self) -> Result<(), LibraryError> {
        // Capture the provider before the config clear below drops it, so the
        // telemetry names which provider was disconnected.
        let provider = self.config_handle.config().cloud_home.provider.clone();

        self.database.disconnect_cloud_home().await?;
        self.config_handle
            .update_store_off_runtime(|c| c.cloud_home = Default::default())
            .await?;
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
    /// established, or the home is keyless/browsable). The sync-status listener
    /// may already be running: its receiver follows this handle across provider
    /// connection.
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
    /// does not happen here, so a test that needs it keeps the loop-driven
    /// connect. Coven's status stream reads as connected either way.
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
