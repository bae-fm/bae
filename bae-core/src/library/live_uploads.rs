//! The cloud-upload pipeline's live state, as one owner.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::db::{DbOutboxQueue, DbOutboxUpload};
use crate::library::outbox_snapshot::{build_outbox_snapshot, TransientUploadState, UploadBlobKey};
use crate::library::{OutboxSnapshot, UploadThroughput};

/// coven's durable outbox says what is queued and survives a restart. These
/// three facts do not: how far the current preparation or provider transfer of
/// each blob has moved, the rolling-window rate those same bytes produce, and
/// whether the person has paused the pipeline. They are one concern because
/// every write touches more than one of them — a callback that advances a
/// blob's bytes also feeds its rate, completion clears both, and the outbox
/// snapshot is derived from all three at once. Held apart, the observer that
/// writes them and the sync controller that reads them each have to keep three
/// handles in step; held here, one place says how a callback changes them and
/// both sides carry a single clone.
#[derive(Clone)]
pub(crate) struct LiveUploads {
    /// Exact blob-bearing rows with preparation or provider work in flight,
    /// mapped to buffer-cadence progress.
    transient: Arc<Mutex<HashMap<UploadBlobKey, TransientUploadState>>>,
    /// Rolling-window throughput over the same bytes `transient` counts, with
    /// each blob's measurement reset at the preparation/provider boundary.
    throughput: Arc<UploadThroughput>,
    /// User-driven absolute pause state. coven's active preparation and
    /// provider futures wait on this through the observer, so suspending them
    /// touches neither the durable queue nor their open upload sessions.
    paused: tokio::sync::watch::Sender<bool>,
}

impl LiveUploads {
    pub(crate) fn new() -> Self {
        let (paused, _) = tokio::sync::watch::channel(false);
        Self {
            transient: Arc::new(Mutex::new(HashMap::new())),
            throughput: Arc::new(UploadThroughput::new()),
            paused,
        }
    }

    /// coven began consuming this blob's plaintext into its durable spool.
    pub(crate) fn preparation_started(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        {
            let mut transient = self.transient.lock().unwrap();
            match transient.entry(blob_key.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(TransientUploadState::Preparing {
                        bytes_done: 0,
                        bytes_total: upload.plaintext_size(),
                    });
                }
                Entry::Occupied(entry) => panic!(
                    "preparation started while the same blob already had transient state for {}:{}; state: {:?}",
                    upload.table(),
                    upload.row_id(),
                    entry.get()
                ),
            }
        }
        self.throughput.begin_preparation(blob_key);
    }

    /// Advance the blob's preparation bytes and feed the tracker only what is
    /// new since the last report. The counts are cumulative within an attempt;
    /// anything else is a coven contract violation and fails loudly.
    pub(crate) fn preparation_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        let blob_key = UploadBlobKey::from_row(upload);
        let delta = {
            let mut transient = self.transient.lock().unwrap();
            match transient.get_mut(&blob_key) {
                Some(TransientUploadState::Preparing {
                    bytes_done: previous,
                    bytes_total: exact_total,
                }) => {
                    if bytes_total != *exact_total || bytes_total != upload.plaintext_size() {
                        panic!(
                            "preparation progress changed its exact plaintext total for {}:{}: \
                             expected {}, received {bytes_total}",
                            upload.table(),
                            upload.row_id(),
                            *exact_total
                        );
                    }
                    if bytes_done < *previous || bytes_done > bytes_total {
                        panic!(
                            "preparation progress regressed or exceeded its exact total for {}:{}: \
                             previous {}, received {bytes_done} of {bytes_total}",
                            upload.table(),
                            upload.row_id(),
                            *previous
                        );
                    }
                    let delta = bytes_done - *previous;
                    *previous = bytes_done;
                    delta
                }
                state => panic!(
                    "preparation progress arrived without a preparation-start state for {}:{}; \
                     transient state: {state:?}",
                    upload.table(),
                    upload.row_id()
                ),
            }
        };
        if delta > 0 {
            self.throughput.record_preparation(&blob_key, delta);
        }
    }

    /// coven began sending this blob's prepared payload to the provider. A
    /// restart can resume directly from coven's durable Prepared state, so no
    /// preparation callback is required in this process first.
    pub(crate) fn upload_started(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        {
            let mut transient = self.transient.lock().unwrap();
            match transient.entry(blob_key.clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(TransientUploadState::UploadStarted);
                }
                Entry::Occupied(mut entry) => match entry.get() {
                    TransientUploadState::Preparing {
                        bytes_done,
                        bytes_total,
                    } if bytes_done == bytes_total => {
                        entry.insert(TransientUploadState::UploadStarted);
                    }
                    state => panic!(
                        "upload started while the same blob already had transient state for {}:{}; state: {state:?}",
                        upload.table(),
                        upload.row_id()
                    ),
                },
            }
        }
        self.throughput.begin_upload(blob_key);
    }

    /// Advance the blob's provider bytes and feed the tracker only what is new
    /// since the last report. coven coalesces these calls to a tick, so each is
    /// already throttled.
    pub(crate) fn upload_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        if bytes_total == 0 || bytes_done > bytes_total {
            panic!(
                "provider progress regressed or changed its exact total for {}:{}: \
                 received {bytes_done} of {bytes_total}",
                upload.table(),
                upload.row_id()
            );
        }
        let blob_key = UploadBlobKey::from_row(upload);
        let delta = {
            let mut transient = self.transient.lock().unwrap();
            match transient.get_mut(&blob_key) {
                Some(state @ TransientUploadState::UploadStarted) => {
                    *state = TransientUploadState::Uploading {
                        bytes_done,
                        bytes_total,
                    };
                    bytes_done
                }
                Some(TransientUploadState::Uploading {
                    bytes_done: previous,
                    bytes_total: previous_total,
                }) => {
                    if bytes_done < *previous || bytes_total != *previous_total {
                        panic!(
                            "provider progress regressed or changed its exact total for {}:{}: \
                             previous {} of {}, received {bytes_done} of {bytes_total}",
                            upload.table(),
                            upload.row_id(),
                            *previous,
                            *previous_total
                        );
                    }
                    let delta = bytes_done - *previous;
                    *previous = bytes_done;
                    delta
                }
                state => panic!(
                    "upload progress arrived without an upload-start state for {}:{}; \
                     transient state: {state:?}",
                    upload.table(),
                    upload.row_id()
                ),
            }
        };
        if delta > 0 {
            self.throughput.record_upload(&blob_key, delta);
        }
    }

    /// coven committed this row journal as Created before reporting completion,
    /// so the durable outbox now owns the blob's Uploaded state: keep no
    /// transient terminal copy that could survive or disagree with that commit.
    ///
    /// The final provider-progress report with the exact encrypted total is
    /// required. Substituting the plaintext source size here would make both the
    /// file row and the throughput false.
    pub(crate) fn upload_finished(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        match self.transient.lock().unwrap().remove(&blob_key) {
            Some(TransientUploadState::Uploading {
                bytes_done,
                bytes_total,
            }) if bytes_total > 0 && bytes_done == bytes_total => {}
            state => panic!(
                "upload completion arrived without exact provider byte progress for {}:{}; \
                 transient state: {state:?}",
                upload.table(),
                upload.row_id()
            ),
        }
        self.throughput.end(&blob_key);
    }

    /// The attempt failed. coven's drain records the attempt count and the error
    /// on its own queue entry, so nothing about the failure is kept here — only
    /// this attempt's live bytes and rate are dropped.
    pub(crate) fn upload_failed(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        let removed = self.transient.lock().unwrap().remove(&blob_key);
        if removed.is_some() {
            self.throughput.end(&blob_key);
        }
    }

    /// Whether the person has paused the pipeline. The snapshot reports it and
    /// coven's drain skips uploads while it holds.
    pub(crate) fn is_paused(&self) -> bool {
        *self.paused.borrow()
    }

    pub(crate) fn set_paused(&self, paused: bool) {
        self.paused.send_replace(paused);
    }

    pub(crate) async fn wait_until_paused(&self) {
        self.wait_for_pause_state(true).await;
    }

    pub(crate) async fn wait_until_resumed(&self) {
        self.wait_for_pause_state(false).await;
    }

    async fn wait_for_pause_state(&self, target: bool) {
        let mut pause_state = self.paused.subscribe();
        loop {
            if *pause_state.borrow_and_update() == target {
                return;
            }
            pause_state
                .changed()
                .await
                .expect("live uploads own the pause sender");
        }
    }

    /// Derive the outbox snapshot from coven's durable queue and this live
    /// state, after dropping the callback facts that queue proves superseded.
    /// Both steps read the same queue, so the snapshot can never report a
    /// transient state the durable rows have already moved past.
    pub(crate) fn outbox_snapshot(&self, queue: DbOutboxQueue) -> OutboxSnapshot {
        self.retain_current(&queue.uploads);
        let transient = { self.transient.lock().unwrap().clone() };
        let paused = self.is_paused();
        build_outbox_snapshot(queue, &transient, &self.throughput, paused)
    }

    /// Drop callback facts when the durable outbox proves their phase has been
    /// superseded. `Created` deliberately retains provider progress: coven
    /// commits that handoff before calling `on_blob_uploaded`, and that callback
    /// owns validation and removal of the exact final byte report.
    fn retain_current(&self, uploads: &[DbOutboxUpload]) {
        let durable: HashMap<_, _> = uploads
            .iter()
            .map(|upload| (UploadBlobKey::from_row(&upload.blob), upload.phase))
            .collect();
        self.transient
            .lock()
            .unwrap()
            .retain(|key, transient| match durable.get(key) {
                None => false,
                Some(coven::QueuedUploadPhase::Created) => true,
                Some(coven::QueuedUploadPhase::Prepared) => {
                    !matches!(transient, TransientUploadState::Preparing { .. })
                }
                Some(coven::QueuedUploadPhase::Pending) => true,
            });
    }

    #[cfg(test)]
    pub(crate) fn clear_release_file_observation_for_test(&self, file_id: &str) {
        let key = UploadBlobKey::new(crate::sync::RELEASE_FILES_NAMESPACE, file_id);
        assert!(
            self.transient.lock().unwrap().remove(&key).is_some(),
            "the test upload observation must exist before it is cleared"
        );
        self.throughput.end(&key);
    }

    #[cfg(test)]
    pub(crate) fn transient_state_for_test(
        &self,
        upload: &coven::RowBlobRef,
    ) -> Option<TransientUploadState> {
        self.transient
            .lock()
            .unwrap()
            .get(&UploadBlobKey::from_row(upload))
            .copied()
    }

    #[cfg(test)]
    pub(crate) fn rates_for_test(&self) -> crate::library::upload_throughput::UploadRates {
        self.throughput.rates()
    }
}
