//! Upload subscriptions and provider operations over coven's store handle.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::{debug, warn};

use crate::config::CloudProvider;
use crate::db::{Database, OutboxDisplayContext, OutboxDisplayRequest};
use crate::diagnostics::{Diagnostics, TelemetryEvent};
use crate::library::{LibraryError, OutboxQueue, OutboxState, OutboxTransfers};
use crate::sync::S3ConfigData;

/// Owns the outbox projection and the cloud-connection lifecycle. Holds clones
/// of the handles the sync paths need (config, database, and diagnostics) plus
/// the live upload state. Cloned alongside the manager — every field is itself
/// a clone-shared handle or `Arc`.
#[derive(Clone)]
pub(crate) struct SyncController {
    outbox_values: tokio::sync::watch::Sender<Option<Result<OutboxState, String>>>,
    /// Serializes every outbox publication and numbers the queue changes in
    /// the order they reach subscribers, and holds the durable queue each one
    /// is built from.
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
    /// Counts the changes to the published queue; a new transfer on the same
    /// queue keeps it.
    revision: u64,
    /// The queue last published, which the next one is compared against;
    /// `None` before the first and after a failure.
    published: Option<OutboxQueue>,
    throughput: crate::library::upload_throughput::UploadThroughput,
    /// The newest durable queue coven delivered or was read for, with the
    /// display rows it needs labelled.
    durable: Option<(coven::UploadQueue, OutboxDisplayRequest)>,
    /// The display query's latest answer and the request it answered.
    names: Option<(OutboxDisplayRequest, OutboxDisplayContext)>,
    /// The records waiting to upload as the pending query last delivered
    /// them, which each queue is published with; `None` before it first
    /// delivers and after it fails.
    pending: Option<Vec<coven::PendingRecord>>,
    /// Points the display query at the rows the held durable queue needs;
    /// present once the outbox subscription runs, and reading nothing more
    /// once that subscription stops or its query ends with its store.
    display_requests: Option<coven::LiveQueryRequests<OutboxDisplayRequest>>,
}

impl OutboxProjection {
    /// The revision `queue` is published at: the last one when it is the
    /// queue last published, the next one otherwise.
    fn revision_for(&mut self, queue: &OutboxQueue) -> u64 {
        if self.published.as_ref() != Some(queue) {
            self.revision = next_revision(self.revision);
            self.published = Some(queue.clone());
        }
        self.revision
    }

    /// The revision a failure is published at, after which any queue is a
    /// change.
    fn revision_for_failure(&mut self) -> u64 {
        self.revision = next_revision(self.revision);
        self.published = None;
        self.revision
    }

    fn hold_durable(&mut self, snapshot: coven::UploadQueue, request: OutboxDisplayRequest) {
        if let Some(requests) = &self.display_requests {
            if let Err(coven::LiveQueryClosed) = requests.set(request.clone()) {
                warn!("the outbox display query has ended; {request:?} is not read");
            }
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
        Ok(self.database.uploads_paused().await?)
    }

    /// The stream the outbox pane and upload standing read; each value is the
    /// latest queue with its transfers, or the failure that kept one from
    /// being built. It wakes its readers only when one of them changed.
    pub(crate) fn subscribe_outbox_values(
        &self,
    ) -> tokio::sync::watch::Receiver<Option<Result<OutboxState, String>>> {
        self.outbox_values.subscribe()
    }

    /// Read coven's durable queue now and publish the queue built from it.
    /// A command that changed the queue calls this so the value it hands back
    /// a revision for already shows its change; the live query's delivery of
    /// the same change follows. The revision is the queue's as it now stands,
    /// published already when the command changed nothing.
    pub(crate) async fn emit_outbox_changed(&self) -> u64 {
        let mut projection = self.outbox.lock().await;
        self.read_and_publish(&mut projection).await
    }

    /// The upload queue and its transfers as the durable queue holds them
    /// now, at the revision last published.
    pub(crate) async fn outbox_state(&self) -> Result<OutboxState, LibraryError> {
        let mut projection = self.outbox.lock().await;
        let durable = self.database.outbox_queue().await?;
        let (queue, transfers) =
            OutboxQueue::from_queue(durable, &projection.throughput.rates(self.clock.elapsed()));
        Ok(OutboxState {
            revision: projection.revision,
            queue: queue.with_pending(&self.database.pending().await?),
            transfers,
        })
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
        // Hold the records before the first queue is built, so every queue
        // this subscription publishes carries the records it delivered.
        let records = pending
            .next()
            .await
            .expect("a new query answers before it ends");
        self.pending_delivered(records).await;
        // Keep this marker until the durable query delivers: cancellation of
        // next() retains its run, which may have begun before a direct read.
        let mut reads_before = self.outbox_reads.load(Ordering::Acquire);
        // A query that has ended, with its store, reads nothing more; once
        // all three have, the subscription is over.
        loop {
            tokio::select! {
                Some(records) = pending.next() => self.pending_delivered(records).await,
                Some(delivered) = subscription.next() => {
                    self.durable_delivered(reads_before, delivered).await;
                    reads_before = self.outbox_reads.load(Ordering::Acquire);
                }
                Some(event) = display.next() => {
                    self.names_answered(event.request, event.result).await;
                }
                else => return,
            }
        }
    }

    /// Hold the records the pending query delivered, and publish the held
    /// queue with them.
    async fn pending_delivered(&self, records: coven::CovenResult<Vec<coven::PendingRecord>>) {
        let mut projection = self.outbox.lock().await;
        match records {
            Ok(records) => {
                projection.pending = Some(records);
                self.publish_if_labelled(&mut projection).await;
            }
            Err(error) => {
                projection.pending = None;
                self.publish_failure(
                    &mut projection,
                    "read the records waiting to upload",
                    cause_text(&error),
                )
                .await;
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
                self.publish_failure(
                    &mut projection,
                    "read the durable cloud outbox",
                    cause_text(&error),
                )
                .await;
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
                self.publish_failure(
                    &mut projection,
                    "identify the durable outbox's display rows",
                    cause_text(&error),
                )
                .await;
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
                self.publish_failure(
                    &mut projection,
                    "read the durable outbox's display rows",
                    cause_text(&error),
                )
                .await;
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
                let built = OutboxQueue::from_queue(
                    queue,
                    &projection.throughput.rates(self.clock.elapsed()),
                );
                self.publish(projection, Ok(built)).await;
            }
            Err(error) => {
                self.publish_failure(
                    projection,
                    "label the durable cloud outbox",
                    cause_text(&error),
                )
                .await;
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
                        let built = OutboxQueue::from_queue(
                            queue,
                            &projection.throughput.rates(self.clock.elapsed()),
                        );
                        self.publish(projection, Ok(built)).await
                    }
                    Err(error) => {
                        self.publish_failure(
                            projection,
                            "label the durable cloud outbox",
                            cause_text(&error),
                        )
                        .await
                    }
                }
            }
            Err(error) => {
                self.publish_failure(
                    projection,
                    "read the durable cloud outbox",
                    cause_text(&error),
                )
                .await
            }
        }
    }

    /// Report a failed read of the upload queue as the bug it is — the
    /// queue's own tables, which nothing else writes, failed to read — and
    /// publish the failure, which every reader of the queue shows.
    async fn publish_failure(
        &self,
        projection: &mut OutboxProjection,
        operation: &str,
        detail: String,
    ) -> u64 {
        self.diagnostics.report_bug(operation, &detail);
        self.publish(projection, Err(detail)).await
    }

    /// Send one outbox value, its files beside the records the pending query
    /// last delivered, numbering it when its queue differs from the one last
    /// published, and only when it differs from the value readers hold.
    /// Before the pending query first delivers, the records are read for it.
    /// Returns the queue's revision.
    async fn publish(
        &self,
        projection: &mut OutboxProjection,
        value: Result<(OutboxQueue, OutboxTransfers), String>,
    ) -> u64 {
        let value = match (value, &projection.pending) {
            (Ok((queue, transfers)), Some(records)) => Ok((queue.with_pending(records), transfers)),
            (Ok((queue, transfers)), None) => self
                .database
                .pending()
                .await
                .map(|records| (queue.with_pending(&records), transfers))
                .map_err(|error| {
                    let detail = cause_text(&error);
                    self.diagnostics
                        .report_bug("read the records waiting to upload", &detail);
                    detail
                }),
            (Err(error), _) => Err(error),
        };
        let value = match value {
            Ok((queue, transfers)) => Ok(OutboxState {
                revision: projection.revision_for(&queue),
                queue,
                transfers,
            }),
            Err(error) => {
                projection.revision_for_failure();
                Err(error)
            }
        };
        self.outbox_values.send_if_modified(|current| {
            if current.as_ref() == Some(&value) {
                return false;
            }
            *current = Some(value);
            true
        });
        projection.revision
    }

    pub(crate) async fn save_s3_config(
        &self,
        data: S3ConfigData,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let (storage, credentials) = data
            .into_location()
            .map_err(|error| LibraryError::Validation(format!("invalid S3 endpoint: {error}")))?;
        let setup = self
            .database
            .setup_s3_storage(
                storage,
                device_name,
                credentials.access_key_id,
                credentials.secret_access_key,
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

fn next_revision(revision: u64) -> u64 {
    revision
        .checked_add(1)
        .expect("outbox projection revision overflow")
}

/// An error with its whole chain of causes, as one line of text.
fn cause_text(error: &(dyn std::error::Error + 'static)) -> String {
    bae_util::error_text::CauseChain(error).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{OutboxPauseState, UploadProgress};

    fn queue(queued: u32) -> OutboxQueue {
        OutboxQueue {
            upload_groups: Vec::new(),
            other_files: Vec::new(),
            total: UploadProgress {
                queued,
                uploading: 0,
            },
            pause_state: OutboxPauseState::Running,
        }
    }

    /// The same queue published again keeps its revision, so a reader that
    /// follows the revision hears of a queue only when it changed; a failure
    /// in between makes the same queue a change again.
    #[test]
    fn a_queue_takes_a_new_revision_only_when_it_changes() {
        let mut projection = OutboxProjection::default();
        let first = projection.revision_for(&queue(1));
        assert_eq!(projection.revision_for(&queue(1)), first);
        let second = projection.revision_for(&queue(2));
        assert!(second > first);
        let failure = projection.revision_for_failure();
        assert!(failure > second);
        assert!(projection.revision_for(&queue(2)) > failure);
    }

    /// A queue read after the outbox subscription stopped, while its store
    /// still reads, is published, though the display query it would point at
    /// the queue's rows has ended with the subscription.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queue_read_after_the_subscription_stopped_is_published() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("library.db");
        let database = Database::new_test(path.to_str().unwrap(), Arc::new(coven::SystemClock))
            .await
            .unwrap();
        let sync = SyncController::new(
            database.clone(),
            Diagnostics::noop(),
            Arc::new(coven::SystemClock),
        );
        let mut values = sync.subscribe_outbox_values();
        let subscription = tokio::spawn({
            let sync = sync.clone();
            let outbox = database.subscribe_cloud_outbox();
            async move { sync.run_cloud_outbox_subscription(outbox).await }
        });
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            values.wait_for(Option::is_some),
        )
        .await
        .expect("the subscription publishes a queue")
        .expect("the outbox stream stays open");
        subscription.abort();
        assert!(subscription.await.unwrap_err().is_cancelled());

        let revision = sync.emit_outbox_changed().await;

        let published = values.borrow().clone().expect("a queue is published");
        assert_eq!(
            published.expect("the queue is read").revision,
            revision,
            "the queue read now is the one published"
        );
    }
}
