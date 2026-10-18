//! Upload subscriptions and provider operations over coven's store handle.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::{debug, error, warn};

use crate::config::CloudProvider;
use crate::db::{Database, OutboxDisplayContext, OutboxDisplayRequest};
use crate::diagnostics::{Diagnostics, TelemetryEvent};
use crate::library::{LibraryError, OutboxSnapshot};
use crate::sync::S3ConfigData;

/// Owns the outbox projection and the cloud-connection lifecycle. Holds clones
/// of the handles the sync paths need (config, database, and diagnostics) plus
/// the live upload state. Cloned alongside the manager — every field is itself
/// a clone-shared handle or `Arc`.
#[derive(Clone)]
pub(crate) struct SyncController {
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
    clock: coven::ClockRef,
    /// Typed telemetry sink, shared with the owning manager. The
    /// provider-connect/disconnect completions emit through it.
    diagnostics: Diagnostics,
}

/// What the outbox stream is built from, under the one lock that numbers its
/// values.
#[derive(Default)]
pub(crate) struct OutboxProjection {
    revision: u64,
    throughput: crate::library::upload_throughput::UploadThroughput,
    /// The newest durable queue coven delivered or was read for, with the
    /// display rows it needs labelled.
    durable: Option<(coven::UploadQueue, OutboxDisplayRequest)>,
    /// The display query's latest answer and the request it answered.
    names: Option<(OutboxDisplayRequest, OutboxDisplayContext)>,
    /// The newest durable queue joined to its display names — what a live
    /// upload change is republished over.
    /// Points the display query at the rows the held durable queue needs;
    /// present once the outbox subscription runs.
    display_requests: Option<coven::LiveQueryRequests<OutboxDisplayRequest>>,
}

impl OutboxProjection {
    fn hold_durable(&mut self, snapshot: coven::UploadQueue, request: OutboxDisplayRequest) {
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
        database: Database,
        diagnostics: Diagnostics,
        clock: coven::ClockRef,
    ) -> Self {
        let (outbox_values, _) = tokio::sync::watch::channel(None);
        Self {
            outbox_values,
            outbox: Arc::new(tokio::sync::Mutex::new(OutboxProjection::default())),
            outbox_reads: Arc::new(AtomicU64::new(0)),
            database,
            diagnostics,
            clock,
        }
    }

    pub(crate) async fn set_sync_paused(&self, paused: bool) -> Result<(), LibraryError> {
        self.database.set_uploads_paused(paused).await?;
        self.emit_outbox_changed().await;
        Ok(())
    }

    pub(crate) async fn is_sync_paused(&self) -> Result<bool, LibraryError> {
        Ok(self.database.uploads().await?.paused)
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
        let mut projection = self.outbox.lock().await;
        let queue = self.database.outbox_queue().await?;
        let mut snapshot =
            OutboxSnapshot::from_queue(queue, &projection.throughput.rates(self.clock.elapsed()));
        snapshot.revision = projection.revision;
        Ok(snapshot.with_pending(self.database.pending().await?))
    }

    /// Keep the outbox stream current: coven's durable queue as its live query
    /// delivers it, the display rows that label that queue, and every change
    /// to the live upload state. Each value is built from what was delivered
    /// and what the live state holds; the database is read again only where a
    /// delivery cannot be trusted to be the newest.
    pub(super) async fn run_cloud_outbox_subscription(
        &self,
        mut subscription: coven::LiveQuery<coven::UploadQueue>,
    ) {
        let mut display = self.database.subscribe_outbox_display(Default::default());
        let mut pending = self.database.subscribe_pending();
        self.outbox.lock().await.display_requests = Some(display.requests());
        // Keep this marker until the durable query delivers: cancellation of
        // next() retains its run, which may have begun before a direct read.
        let mut reads_before = self.outbox_reads.load(Ordering::Acquire);
        loop {
            tokio::select! {
                records = pending.next() => {
                    let mut projection=self.outbox.lock().await;
                    match records {
                        Ok(_) => self.publish_if_labelled(&mut projection).await,
                        Err(error) => { self.publish(&mut projection,Err(error.to_string())).await; }
                    }
                }
                delivered = subscription.next() => {
                    self.durable_delivered(reads_before, delivered).await;
                    reads_before = self.outbox_reads.load(Ordering::Acquire);
                }
                event = display.next() => {
                    self.names_answered(event.request, event.result).await;
                }
            }
        }
    }

    async fn durable_delivered(
        &self,
        reads_before: u64,
        delivered: coven::CovenResult<coven::UploadQueue>,
    ) {
        let mut projection = self.outbox.lock().await;
        let snapshot = match delivered {
            Ok(snapshot) => snapshot,
            Err(error) => {
                warn!(%error, "Failed to read the durable cloud outbox");
                self.publish(&mut projection, Err(error.to_string())).await;
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
                projection
                    .throughput
                    .observe(&snapshot, self.clock.elapsed());
                projection.hold_durable(snapshot, request);
                self.publish_if_labelled(&mut projection).await;
            }
            Err(error) => {
                warn!(%error, "Failed to identify durable outbox display rows");
                self.publish(&mut projection, Err(error.to_string())).await;
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
                self.publish_if_labelled(&mut projection).await;
            }
            Err(error) => {
                warn!(%error, "Failed to read durable outbox display rows");
                self.publish(&mut projection, Err(error.to_string())).await;
            }
        }
    }

    /// Join the held durable snapshot to its display names and publish, once
    /// the display query has answered for the request that snapshot needs.
    async fn publish_if_labelled(&self, projection: &mut OutboxProjection) {
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
                let snapshot = OutboxSnapshot::from_queue(
                    queue,
                    &projection.throughput.rates(self.clock.elapsed()),
                );
                self.publish(projection, Ok(snapshot)).await;
            }
            Err(error) => {
                error!(%error, "Failed to label the durable cloud outbox");
                self.publish(projection, Err(error.to_string())).await;
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
                projection
                    .throughput
                    .observe(&snapshot, self.clock.elapsed());
                projection.hold_durable(snapshot.clone(), request);
                match Database::outbox_queue_from_context(snapshot, names) {
                    Ok(queue) => {
                        let snapshot = OutboxSnapshot::from_queue(
                            queue,
                            &projection.throughput.rates(self.clock.elapsed()),
                        );
                        self.publish(projection, Ok(snapshot)).await
                    }
                    Err(error) => {
                        error!(%error, "Failed to label the durable cloud outbox");
                        self.publish(projection, Err(error.to_string())).await
                    }
                }
            }
            Err(error) => {
                warn!(%error, "Failed to read the durable cloud outbox");
                self.publish(projection, Err(error.to_string())).await
            }
        }
    }

    /// Number and send one outbox value.
    async fn publish(
        &self,
        projection: &mut OutboxProjection,
        value: Result<OutboxSnapshot, String>,
    ) -> u64 {
        projection.revision = projection
            .revision
            .checked_add(1)
            .expect("outbox projection revision overflow");
        let revision = projection.revision;
        let value = match value {
            Ok(mut snapshot) => {
                snapshot.revision = revision;
                self.database
                    .pending()
                    .await
                    .map(|records| snapshot.with_pending(records))
                    .map_err(|error| error.to_string())
            }
            Err(error) => Err(error),
        };
        self.outbox_values.send_replace(Some(value));
        revision
    }

    pub(crate) async fn save_s3_config(
        &self,
        data: S3ConfigData,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let endpoint = data
            .endpoint
            .map(|value| value.parse())
            .transpose()
            .map_err(|error| LibraryError::Validation(format!("invalid S3 endpoint: {error}")))?;
        let storage = coven::StorageConfig::S3 {
            bucket: data.bucket,
            region: data.region,
            endpoint,
            prefix: data.key_prefix.unwrap_or_default(),
            profile: data.profile,
        };
        let setup = self
            .database
            .setup_s3_storage(
                storage,
                device_name,
                data.access_key,
                coven::SecretText::new(data.secret_key),
            )
            .await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::S3,
            });
        Ok(setup)
    }

    /// Sign in to an account provider before choosing a new location on it.
    /// bae registers a Full Dropbox app, since a household's library lives in
    /// a folder its members share.
    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn sign_in(&self, provider: CloudProvider) -> Result<(), LibraryError> {
        let access = match provider {
            CloudProvider::Dropbox => {
                coven::OAuthAccess::Dropbox(coven::DropboxAccess::FullDropbox)
            }
            CloudProvider::OneDrive => coven::OAuthAccess::OneDrive,
            CloudProvider::S3 | CloudProvider::CloudKit => {
                return Err(LibraryError::Internal(format!(
                    "{provider:?} has no account sign-in"
                )))
            }
        };
        self.database.authenticate(access).await?;
        Ok(())
    }

    /// Sign in again to the provider this library's location is on, with the
    /// access that location needs, and reconnect the location with the new
    /// sign-in.
    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn sign_in_again(
        &self,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let storage = self.database.storage_config().await?.ok_or_else(|| {
            LibraryError::Internal("signing in again needs a library with storage".into())
        })?;
        let access = storage.oauth_access().ok_or_else(|| {
            LibraryError::Internal(format!(
                "{:?} storage has no account sign-in",
                storage.provider()
            ))
        })?;
        self.database.authenticate(access).await?;
        self.setup_oauth_storage(storage, device_name).await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let provider = storage.provider();
        let setup = self
            .database
            .setup_oauth_storage(storage, device_name)
            .await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected { provider });
        Ok(setup)
    }

    pub(crate) async fn use_cloudkit(
        &self,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let setup = self.database.setup_cloudkit_storage(device_name).await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::CloudKit,
            });
        Ok(setup)
    }

    pub(crate) async fn disconnect_cloud_provider(&self) -> Result<(), LibraryError> {
        let provider = self
            .database
            .storage_config()
            .await?
            .map(|storage| storage.provider());
        self.database.disconnect_storage().await?;
        if let Some(provider) = provider {
            self.diagnostics
                .event(TelemetryEvent::CloudProviderDisconnected { provider });
        }
        Ok(())
    }

    pub(crate) async fn attach_and_start_sync(&self) -> Result<(), LibraryError> {
        self.database.start_sync().await?;
        Ok(())
    }

    pub(crate) async fn unlock_store_key(&self) -> Result<(), LibraryError> {
        self.database.unlock_store_key().await?;
        Ok(())
    }
}
