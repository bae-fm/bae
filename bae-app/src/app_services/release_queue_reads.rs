//! The transfer queues' reads on [`AppServices`]: the windows a pane draws of
//! a queue, each entry beside its release read from the queue's table with
//! it, so a title edited while the entry waits shows on its row; how many
//! entries are in each phase; and how far the active entry has got, which
//! travels apart, keyed by release.

use super::*;
use bae_library::LibraryError;

impl AppServices {
    /// The download queue as the Downloads pane draws it: how many entries it
    /// holds, and the windows the pane asks for, none until the first
    /// [`set`](bae_store::live_query::LiveRead::set).
    /// One query serves every window, and reads them again as an entry
    /// comes, goes, starts or fails, or a release a window shows changes.
    pub fn subscribe_download_browse(&self) -> bae_views::LiveDownloadBrowse {
        let manager = &self.inner.manager;
        let query = manager
            .until_closed(manager.subscribe_download_windows(bae_store::LibraryPageWindows::new()));
        bae_views::LiveDownloadBrowse::new(
            query,
            |read, queue: bae_transfers::ReleaseQueueWindows<bae_transfers::DownloadRequest>| {
                Some(bae_views::LibraryBrowseSnapshot {
                    windows: queue.windows,
                    total_count: queue.total_count,
                    request_revision: read.revision,
                    cause: read.cause,
                })
            },
        )
    }

    /// How many downloads are in each phase now, and whether the queue is
    /// paused.
    pub async fn download_counts(&self) -> Result<bae_transfers::ReleaseQueueCounts, LibraryError> {
        self.inner.manager.download_counts().await
    }

    /// How many downloads are in each phase, and whether the queue is paused:
    /// what the Downloads pane's controls show, the first value at once, then
    /// one after each write that changes them.
    pub fn subscribe_download_counts(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_transfers::ReleaseQueueCounts> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_download_counts())
    }

    /// How far each downloading release has come (a pin's bytes, or a
    /// move-to-this-device's), sent as they move.
    pub fn watch_download_transfers(
        &self,
    ) -> bae_transfers::ReleaseTransfersWatch<bae_transfers::DownloadTransfer> {
        self.inner.manager.watch_download_transfers()
    }

    /// The export queue as it stands now, each entry beside its release, the
    /// active one with the percent it has reached: the automation surface's
    /// export status.
    #[cfg(desktop)]
    pub async fn output_snapshot(&self) -> Result<bae_transfers::OutputSnapshot, LibraryError> {
        self.inner.manager.output_snapshot().await
    }

    /// The percent each exporting release has reached, sent as it moves.
    #[cfg(desktop)]
    pub fn watch_output_transfers(&self) -> bae_transfers::ReleaseTransfersWatch<u8> {
        self.inner.manager.watch_output_transfers()
    }
}
