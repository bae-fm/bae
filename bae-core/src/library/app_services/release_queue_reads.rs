//! The transfer queues' reads on [`AppServices`]: the windows a pane draws of
//! a queue, each entry beside its release read from the queue's table with
//! it, so a title edited while the entry waits shows on its row; how many
//! entries are in each phase; and how far the active entry has got, which
//! travels apart, keyed by release.

use super::*;
use crate::library::LibraryError;

impl AppServices {
    /// The download queue as the Downloads pane draws it: how many entries it
    /// holds, and the windows the pane asks for, none until the first
    /// [`set_windows`](crate::library::LibraryBrowseSubscription::set_windows).
    /// One query serves every window, and reads them again as an entry
    /// comes, goes, starts or fails, or a release a window shows changes.
    pub fn subscribe_download_browse(&self) -> crate::library::DownloadBrowseSubscription {
        let manager = &self.inner.manager;
        let query = manager.until_closed(
            manager.subscribe_download_windows(crate::library::LibraryPageWindows::new()),
        );
        crate::library::LibraryBrowseSubscription::new(
            query,
            |read: crate::library::ReleaseQueueWindows<()>, request_revision, cause| {
                crate::library::LibraryBrowseSnapshot {
                    windows: read.windows,
                    sections: Vec::new(),
                    total_count: read.total_count,
                    row_count: read.total_count,
                    request_revision,
                    cause,
                }
            },
        )
    }

    /// How many downloads are in each phase now, and whether the queue is
    /// paused.
    pub async fn download_counts(
        &self,
    ) -> Result<crate::library::ReleaseQueueCounts, LibraryError> {
        self.inner.manager.download_counts().await
    }

    /// How many downloads are in each phase, and whether the queue is paused,
    /// and each change to them: what the Downloads pane's controls show.
    pub fn subscribe_download_counts(
        &self,
    ) -> tokio::sync::mpsc::UnboundedReceiver<
        Result<crate::library::ReleaseQueueCounts, LibraryError>,
    > {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut query = self.inner.manager.subscribe_download_counts();
        self.inner
            .manager
            .spawn_subscription_task("download counts", async move {
                while let Some(value) = query.next().await {
                    if tx.send(value.map_err(LibraryError::from)).is_err() {
                        return;
                    }
                }
            });
        rx
    }

    /// The bytes each downloading release has moved, sent as they move.
    pub fn watch_download_transfers(&self) -> crate::library::DownloadTransfersWatch {
        crate::library::DownloadTransfersWatch::of(
            self.inner.manager.subscribe_download_transfers(),
        )
    }

    /// The export queue as it stands now, each entry beside its release, the
    /// active one with the percent it has reached: the automation surface's
    /// export status.
    #[cfg(desktop)]
    pub async fn output_snapshot(&self) -> Result<crate::library::OutputSnapshot, LibraryError> {
        self.inner.manager.output_snapshot().await
    }

    /// The percent each exporting release has reached, sent as it moves.
    #[cfg(desktop)]
    pub fn watch_output_transfers(&self) -> crate::library::OutputTransfersWatch {
        crate::library::OutputTransfersWatch::of(self.inner.manager.subscribe_output_transfers())
    }
}
