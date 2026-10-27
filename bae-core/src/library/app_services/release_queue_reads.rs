//! The transfer queues' reads on [`AppServices`]: each queue's entries beside
//! the releases they name, read from the queue's table with them, so a title
//! edited while the entry waits shows on its row. How far the active entry
//! has got travels apart, keyed by release.

use super::*;
use crate::library::release_queue::ReleaseQueueSnapshot;
use crate::library::LibraryError;

type QueueValues<Extra, Progress> = tokio::sync::mpsc::UnboundedReceiver<
    Result<ReleaseQueueSnapshot<Extra, Progress>, LibraryError>,
>;

impl AppServices {
    /// The download queue as the Downloads pane shows it now.
    pub async fn download_snapshot(
        &self,
    ) -> Result<crate::library::DownloadSnapshot, LibraryError> {
        self.inner.manager.download_snapshot().await
    }

    /// The download queue as the Downloads pane shows it, and each change to
    /// it: an entry coming, going, starting or failing, the queue pausing, or
    /// a library change to a release it names. The bytes the active entry
    /// moves are [`watch_download_transfers`](Self::watch_download_transfers).
    pub fn subscribe_download_values(&self) -> QueueValues<(), ()> {
        self.forward_release_queue(
            "download queue values",
            self.inner.manager.subscribe_download_snapshot(),
        )
    }

    /// The bytes each downloading release has moved, sent as they move.
    pub fn watch_download_transfers(&self) -> crate::library::DownloadTransfersWatch {
        crate::library::DownloadTransfersWatch::of(
            self.inner.manager.subscribe_download_transfers(),
        )
    }

    /// The export queue as it stands now, each entry beside its release, the
    /// active one with the percent it has reached.
    #[cfg(desktop)]
    pub async fn output_snapshot(&self) -> Result<crate::library::OutputSnapshot, LibraryError> {
        self.inner.manager.output_snapshot().await
    }

    /// The export queue, each entry beside its release, and each change to
    /// it: an entry coming, going, starting or failing, the queue pausing,
    /// or a library change to a release it names. The storage inspector
    /// reads it. The percent each export has reached is
    /// [`watch_output_transfers`](Self::watch_output_transfers).
    #[cfg(desktop)]
    pub fn subscribe_output_values(
        &self,
    ) -> QueueValues<crate::library::output_snapshot::OutputRequest, ()> {
        self.forward_release_queue(
            "output queue values",
            self.inner.manager.subscribe_output_snapshot(),
        )
    }

    /// The percent each exporting release has reached, sent as it moves.
    #[cfg(desktop)]
    pub fn watch_output_transfers(&self) -> crate::library::OutputTransfersWatch {
        crate::library::OutputTransfersWatch::of(self.inner.manager.subscribe_output_transfers())
    }

    /// Send each value the queue's live read delivers, until it ends or the
    /// reader goes.
    fn forward_release_queue<Extra: Send + 'static>(
        &self,
        task_name: &'static str,
        mut query: coven::LiveQuery<ReleaseQueueSnapshot<Extra, ()>>,
    ) -> QueueValues<Extra, ()>
    where
        ReleaseQueueSnapshot<Extra, ()>: Clone + PartialEq,
    {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.inner
            .manager
            .spawn_subscription_task(task_name, async move {
                while let Some(value) = query.next().await {
                    if tx.send(value.map_err(LibraryError::from)).is_err() {
                        return;
                    }
                }
            });
        rx
    }
}
