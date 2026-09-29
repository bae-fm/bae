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
        self.resolve_release_queue(self.inner.manager.download_queue())
            .await
    }

    /// The download queue as the Downloads pane shows it, and each change to
    /// it: a change to the queue, or a library change to a release it names.
    pub fn subscribe_download_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> QueueValues<(), crate::library::DownloadTransferProgress> {
        self.subscribe_release_queue(
            runtime_handle,
            self.inner.manager.subscribe_download_queue(),
        )
    }

    /// The export queue as the Exporting pane shows it now.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn output_snapshot(&self) -> Result<crate::library::OutputSnapshot, LibraryError> {
        self.resolve_release_queue(self.inner.manager.output_queue())
            .await
    }

    /// The export queue as the Exporting pane shows it, and each change to
    /// it: a change to the queue, or a library change to a release it names.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn subscribe_output_values(
        &self,
        runtime_handle: &tokio::runtime::Handle,
    ) -> QueueValues<crate::library::output_snapshot::OutputRequest, u8> {
        self.subscribe_release_queue(runtime_handle, self.inner.manager.subscribe_output_queue())
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

    /// One live query reads the releases the queue names. A queue change that
    /// names the same releases — progress, a state change, pause — resolves
    /// the read it already has; one that adds or drops a release points the
    /// query at the new set, and the read for that set resolves the queue as
    /// it stands then.
    fn subscribe_release_queue<Extra, Progress>(
        &self,
        runtime_handle: &tokio::runtime::Handle,
        mut contents: tokio::sync::watch::Receiver<ReleaseQueueContents<Extra, Progress>>,
    ) -> QueueValues<Extra, Progress>
    where
        Extra: Clone + Send + Sync + 'static,
        Progress: Clone + Send + Sync + 'static,
    {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let manager = self.inner.manager.clone();
        let query_runtime = runtime_handle.clone();
        runtime_handle.spawn(async move {
            let mut queue = contents.borrow_and_update().clone();
            let mut request = queue.release_ids();
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
                        let value = result.map(|read| {
                            let value = queue.resolve(&read);
                            current = Some(read);
                            value
                        });
                        if tx.send(value).is_err() { return; }
                    }
                    changed = contents.changed() => {
                        if changed.is_err() { return; }
                        queue = contents.borrow_and_update().clone();
                        let next = queue.release_ids();
                        if next != request {
                            request = next;
                            current = None;
                            releases.set(request.clone());
                        } else if let Some(read) = current.as_ref() {
                            if tx.send(Ok(queue.resolve(read))).is_err() { return; }
                        }
                    }
                }
            }
        });
        rx
    }
}
