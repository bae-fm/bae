//! The transfer queues' reads on [`AppServices`]: each queue's entries beside
//! the releases they name, as the library holds them when the queue is read.
//! A queue entry carries its release's id only, so a title edited while the
//! entry waits shows on its row.

use super::*;
use crate::library::release_queue::{ReleaseQueueContents, ReleaseQueueSnapshot};
use crate::library::LibraryError;

type QueueValues<Extra, Progress> = tokio::sync::mpsc::UnboundedReceiver<
    Result<ReleaseQueueSnapshot<Extra, Progress>, LibraryError>,
>;

impl AppServices {
    /// The download queue as the Downloads pane shows it now.
    pub async fn download_snapshot(
        &self,
    ) -> Result<crate::library::DownloadSnapshot, LibraryError> {
        self.resolve_release_queue(self.inner.manager.download_queue().phases())
            .await
    }

    /// The download queue as the Downloads pane shows it, and each change to
    /// it: an entry coming, going, starting or failing, the queue pausing, or
    /// a library change to a release it names. The bytes the active entry
    /// moves are [`watch_download_transfers`](Self::watch_download_transfers).
    pub fn subscribe_download_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> QueueValues<(), ()> {
        self.subscribe_release_queue(
            runtime_handle,
            self.inner.manager.subscribe_download_queue(),
            |contents| contents.phases(),
        )
    }

    /// The bytes each downloading release has moved, sent as they move.
    pub fn watch_download_transfers(&self) -> crate::library::DownloadTransfersWatch {
        crate::library::DownloadTransfersWatch::of(self.inner.manager.subscribe_download_queue())
    }

    /// The export queue as it stands now, each entry beside its release.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn output_snapshot(&self) -> Result<crate::library::OutputSnapshot, LibraryError> {
        self.resolve_release_queue(self.inner.manager.output_queue())
            .await
    }

    /// The export queue, each entry beside its release, and each change to
    /// it: an entry coming, going, starting or failing, the queue pausing,
    /// or a library change to a release it names. The storage inspector
    /// reads it. The percent each export has reached is
    /// [`watch_output_transfers`](Self::watch_output_transfers).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn subscribe_output_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> QueueValues<crate::library::output_snapshot::OutputRequest, ()> {
        self.subscribe_release_queue(
            runtime_handle,
            self.inner.manager.subscribe_output_queue(),
            |contents| contents.phases(),
        )
    }

    /// The percent each exporting release has reached, sent as it moves.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn watch_output_transfers(&self) -> crate::library::OutputTransfersWatch {
        crate::library::OutputTransfersWatch::of(self.inner.manager.subscribe_output_queue())
    }

    async fn resolve_release_queue<Extra: Clone, Progress: Clone>(
        &self,
        queue: ReleaseQueueContents<Extra, Progress>,
    ) -> Result<ReleaseQueueSnapshot<Extra, Progress>, LibraryError> {
        let releases = self
            .inner
            .manager
            .queued_releases(queue.release_ids())
            .await?;
        Ok(queue.resolve(&releases))
    }

    /// One live query reads the releases the queue names; `shown` is what of
    /// the queue the value carries. A queue change that names the same
    /// releases — progress, a state change, pause — resolves the read it
    /// already has; one that adds or drops a release points the query at the
    /// new set, and the read for that set resolves the queue as it stands
    /// then. A value equal to the one sent last is not sent again: a change
    /// `shown` leaves out moves nothing a reader holds.
    fn subscribe_release_queue<Extra, Progress, Shown>(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        mut contents: tokio::sync::watch::Receiver<ReleaseQueueContents<Extra, Progress>>,
        shown: impl Fn(&ReleaseQueueContents<Extra, Progress>) -> ReleaseQueueContents<Extra, Shown>
            + Send
            + 'static,
    ) -> QueueValues<Extra, Shown>
    where
        Extra: Clone + PartialEq + Send + Sync + 'static,
        Progress: Clone + Send + Sync + 'static,
        Shown: Clone + PartialEq + Send + Sync + 'static,
    {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let query_runtime = runtime_handle.clone();
        runtime_handle.spawn(async move {
            let mut queue = shown(&contents.borrow_and_update());
            let mut request = queue.release_ids();
            // The value last sent, which an equal one is not sent over.
            let mut sent: Option<ReleaseQueueSnapshot<Extra, Shown>> = None;
            let mut releases = reconfigurable_live_query_events(
                &query_runtime,
                manager.subscribe_queued_releases(request.clone()),
            );
            // The releases last read for `request`.
            let mut current = None;
            loop {
                tokio::select! {
                    event = releases.recv() => {
                        let Some(result) = event else { return };
                        let value = match result {
                            Ok(read) => {
                                let value = queue.resolve(&read);
                                current = Some(read);
                                if sent.as_ref() == Some(&value) { continue; }
                                sent = Some(value.clone());
                                Ok(value)
                            }
                            Err(error) => {
                                // The next value after a failure is what says
                                // the read recovered, even when it equals the
                                // one before.
                                sent = None;
                                Err(error)
                            }
                        };
                        if tx.send(value).is_err() { return; }
                    }
                    changed = contents.changed() => {
                        if changed.is_err() { return; }
                        queue = shown(&contents.borrow_and_update());
                        let next = queue.release_ids();
                        if next != request {
                            request = next;
                            current = None;
                            releases.set(request.clone());
                        } else if let Some(read) = current.as_ref() {
                            let value = queue.resolve(read);
                            if sent.as_ref() == Some(&value) { continue; }
                            sent = Some(value.clone());
                            if tx.send(Ok(value)).is_err() { return; }
                        }
                    }
                }
            }
        });
        rx
    }
}
