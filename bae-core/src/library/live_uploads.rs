//! The cloud-upload pipeline's live state, as one owner.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tracing::error;

use crate::db::DbOutboxQueue;
use crate::library::outbox_snapshot::{
    build_outbox_snapshot, ByteProgress, TransientUploadState, UploadBlobKey,
};
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
    /// Marked on every change a callback or the pause makes. The sync
    /// controller's outbox projection waits on it and republishes from the
    /// durable queue it already holds; a burst of changes before it runs is
    /// one republish.
    changed: tokio::sync::watch::Sender<()>,
}

impl LiveUploads {
    pub(crate) fn new() -> Self {
        let (paused, _) = tokio::sync::watch::channel(false);
        let (changed, _) = tokio::sync::watch::channel(());
        Self {
            transient: Arc::new(Mutex::new(HashMap::new())),
            throughput: Arc::new(UploadThroughput::new()),
            paused,
            changed,
        }
    }

    /// Wakes on each change to this live state, coalescing a burst into one
    /// wake.
    pub(crate) fn subscribe_changes(&self) -> tokio::sync::watch::Receiver<()> {
        self.changed.subscribe()
    }

    fn mark_changed(&self) {
        self.changed.send_replace(());
    }

    /// coven began consuming this blob's plaintext into its durable spool. A
    /// start begins a new attempt, whatever the blob's previous attempt left.
    pub(crate) fn preparation_started(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        {
            let mut transient = self.transient.lock().unwrap();
            let start =
                TransientUploadState::Preparing(ByteProgress::none_of(upload.plaintext_size()));
            if let Some(previous) = transient.insert(blob_key.clone(), start) {
                error!(
                    "preparation of {}:{} started while its previous attempt never reported \
                     its end; that attempt's state: {previous:?}",
                    upload.table(),
                    upload.row_id()
                );
                if previous.is_measured() {
                    self.throughput.end(&blob_key);
                }
            }
        }
        self.throughput.begin_preparation(blob_key);
        self.mark_changed();
    }

    /// Advance the blob's preparation bytes and feed the tracker only what is
    /// new since the last report. The counts are cumulative within an attempt
    /// against one exact plaintext total; a report that breaks that leaves the
    /// attempt untracked rather than showing bytes that cannot be trusted.
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
                Some(TransientUploadState::Preparing(progress)) => {
                    let delta = progress.advance(bytes_done, bytes_total);
                    if delta.is_none() {
                        let previous = *progress;
                        self.untrack(
                            &mut transient,
                            upload,
                            format_args!(
                                "preparation progress {bytes_done} of {bytes_total} does not \
                                 follow {previous:?}"
                            ),
                        );
                    }
                    delta
                }
                Some(TransientUploadState::Untracked) => None,
                state => {
                    let state = state.map(|state| *state);
                    self.untrack(
                        &mut transient,
                        upload,
                        format_args!(
                            "preparation progress arrived without a preparation start; state: \
                             {state:?}"
                        ),
                    );
                    None
                }
            }
        };
        if let Some(delta) = delta.filter(|delta| *delta > 0) {
            self.throughput.record_preparation(&blob_key, delta);
        }
        self.mark_changed();
    }

    /// coven began sending this blob's prepared payload to the provider. A
    /// restart can resume directly from coven's durable Prepared state, so no
    /// preparation callback is required in this process first.
    pub(crate) fn upload_started(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        {
            let mut transient = self.transient.lock().unwrap();
            if let Some(previous) =
                transient.insert(blob_key.clone(), TransientUploadState::UploadStarted)
            {
                match previous {
                    TransientUploadState::Preparing(progress) if progress.is_complete() => {}
                    TransientUploadState::Untracked => {}
                    previous => error!(
                        "upload of {}:{} started while its previous attempt never reported \
                         its end; that attempt's state: {previous:?}",
                        upload.table(),
                        upload.row_id()
                    ),
                }
                if previous.is_measured() {
                    self.throughput.end(&blob_key);
                }
            }
        }
        self.throughput.begin_upload(blob_key);
        self.mark_changed();
    }

    /// Advance the blob's provider bytes and feed the tracker only what is new
    /// since the last report. coven coalesces these calls to a tick, so each is
    /// already throttled. A report that goes backwards, overshoots, or changes
    /// the exact provider total leaves the attempt untracked.
    pub(crate) fn upload_progress(
        &self,
        upload: &coven::RowBlobRef,
        bytes_done: u64,
        bytes_total: u64,
    ) {
        let blob_key = UploadBlobKey::from_row(upload);
        let delta = {
            let mut transient = self.transient.lock().unwrap();
            match transient.get_mut(&blob_key) {
                Some(state @ TransientUploadState::UploadStarted) => {
                    match ByteProgress::new(bytes_done, bytes_total).filter(|_| bytes_total > 0) {
                        Some(progress) => {
                            *state = TransientUploadState::Uploading(progress);
                            Some(bytes_done)
                        }
                        None => {
                            self.untrack(
                                &mut transient,
                                upload,
                                format_args!(
                                    "provider progress {bytes_done} of {bytes_total} is not a \
                                     byte count within a provider total"
                                ),
                            );
                            None
                        }
                    }
                }
                Some(TransientUploadState::Uploading(progress)) => {
                    let delta = progress.advance(bytes_done, bytes_total);
                    if delta.is_none() {
                        let previous = *progress;
                        self.untrack(
                            &mut transient,
                            upload,
                            format_args!(
                                "provider progress {bytes_done} of {bytes_total} does not follow \
                                 {previous:?}"
                            ),
                        );
                    }
                    delta
                }
                Some(TransientUploadState::Untracked) => None,
                state => {
                    let state = state.map(|state| *state);
                    self.untrack(
                        &mut transient,
                        upload,
                        format_args!(
                            "provider progress arrived without an upload start; state: {state:?}"
                        ),
                    );
                    None
                }
            }
        };
        if let Some(delta) = delta.filter(|delta| *delta > 0) {
            self.throughput.record_upload(&blob_key, delta);
        }
        self.mark_changed();
    }

    /// coven committed this row journal as Created before reporting completion,
    /// so the durable outbox now owns the blob's Uploaded state: keep no
    /// transient terminal copy that could survive or disagree with that commit.
    /// The attempt should have reported its exact final provider bytes first;
    /// when it did not, the durable row still reads Uploaded, and the gap is
    /// logged.
    pub(crate) fn upload_finished(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        let removed = self.transient.lock().unwrap().remove(&blob_key);
        match removed {
            Some(TransientUploadState::Uploading(progress))
                if progress.total() > 0 && progress.is_complete() => {}
            state => error!(
                "upload of {}:{} finished without exact final provider progress; state: \
                 {state:?}",
                upload.table(),
                upload.row_id()
            ),
        }
        if removed.is_some_and(TransientUploadState::is_measured) {
            self.throughput.end(&blob_key);
        }
        self.mark_changed();
    }

    /// The attempt ended without finishing: it failed (coven's drain records
    /// the attempt count and the error on its own queue entry, so nothing about
    /// the failure is kept here), or the drain running it was dropped and a
    /// later drain resumes it. Either way only this attempt's live bytes and
    /// rate are dropped.
    pub(crate) fn upload_ended(&self, upload: &coven::RowBlobRef) {
        let blob_key = UploadBlobKey::from_row(upload);
        let removed = self.transient.lock().unwrap().remove(&blob_key);
        if removed.is_some_and(TransientUploadState::is_measured) {
            self.throughput.end(&blob_key);
        }
        self.mark_changed();
    }

    /// Stop tracking the blob's current attempt: its callbacks contradicted
    /// themselves, so its bytes and rate are dropped and its row renders from
    /// coven's durable phase until the next attempt starts.
    fn untrack(
        &self,
        transient: &mut HashMap<UploadBlobKey, TransientUploadState>,
        upload: &coven::RowBlobRef,
        why: std::fmt::Arguments<'_>,
    ) {
        error!(
            "no longer tracking the upload attempt of {}:{}: {why}",
            upload.table(),
            upload.row_id()
        );
        let blob_key = UploadBlobKey::from_row(upload);
        if transient
            .insert(blob_key.clone(), TransientUploadState::Untracked)
            .is_some_and(TransientUploadState::is_measured)
        {
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
        self.mark_changed();
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
    /// state. Every attempt coven starts reports how it ends — finished,
    /// failed, or abandoned with its drain — so the live state holds exactly
    /// the attempts in flight.
    pub(crate) fn outbox_snapshot(&self, queue: DbOutboxQueue) -> OutboxSnapshot {
        let transient = { self.transient.lock().unwrap().clone() };
        let paused = self.is_paused();
        build_outbox_snapshot(queue, &transient, &self.throughput, paused)
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
