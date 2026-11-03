//! The transfer queues' reads on [`AppServices`]: the windows a pane draws of
//! a queue, each entry beside its release read from the queue's table with
//! it, so a title edited while the entry waits shows on its row; how many
//! entries are in each phase; and how far the active entry has got, which
//! travels apart, keyed by release.

use super::*;
use bae_core::library::LibraryError;

impl AppServices {
    /// The download queue as the Downloads pane draws it: how many entries it
    /// holds, and the windows the pane asks for, none until the first
    /// [`set_windows`](bae_core::library::LiveLibraryBrowse::set_windows).
    /// One query serves every window, and reads them again as an entry
    /// comes, goes, starts or fails, or a release a window shows changes.
    pub fn subscribe_download_browse(&self) -> bae_core::library::LiveDownloadBrowse {
        let manager = &self.inner.manager;
        let query = manager
            .until_closed(manager.subscribe_download_windows(bae_db::LibraryPageWindows::new()));
        bae_core::library::LiveLibraryBrowse::new(
            query,
            |read: bae_core::library::ReleaseQueueWindows<()>, request_revision, cause| {
                bae_core::library::LibraryBrowseSnapshot {
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
    ) -> Result<bae_core::library::ReleaseQueueCounts, LibraryError> {
        self.inner.manager.download_counts().await
    }

    /// How many downloads are in each phase, and whether the queue is paused:
    /// what the Downloads pane's controls show, the first value at once, then
    /// one after each write that changes them.
    pub fn subscribe_download_counts(
        &self,
    ) -> bae_db::live_query::LiveValues<bae_core::library::ReleaseQueueCounts> {
        let manager = &self.inner.manager;
        manager.values_until_closed(manager.subscribe_download_counts())
    }

    /// The bytes each downloading release has moved, sent as they move.
    pub fn watch_download_transfers(&self) -> bae_core::library::DownloadTransfersWatch {
        bae_core::library::DownloadTransfersWatch::of(
            self.inner.manager.subscribe_download_transfers(),
        )
    }

    /// The bytes each release this process is making local has downloaded,
    /// sent as they move.
    pub fn watch_make_local_transfers(&self) -> bae_core::library::MakeLocalTransfersWatch {
        bae_core::library::MakeLocalTransfersWatch::of(
            self.inner.manager.subscribe_make_local_transfers(),
        )
    }

    /// The export queue as it stands now, each entry beside its release, the
    /// active one with the percent it has reached: the automation surface's
    /// export status.
    #[cfg(desktop)]
    pub async fn output_snapshot(&self) -> Result<bae_core::library::OutputSnapshot, LibraryError> {
        self.inner.manager.output_snapshot().await
    }

    /// The percent each exporting release has reached, sent as it moves.
    #[cfg(desktop)]
    pub fn watch_output_transfers(&self) -> bae_core::library::OutputTransfersWatch {
        bae_core::library::OutputTransfersWatch::of(self.inner.manager.subscribe_output_transfers())
    }
}
