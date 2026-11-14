//! The transfer queues' reads on [`AppServices`]: the windows a pane draws of
//! a queue, each entry beside its release read from the queue's table with
//! it, so a title edited while the entry waits shows on its row; how many
//! entries are in each phase; and how far the active entry has got, which
//! travels apart, keyed by release.

use super::*;
use bae_library::LibraryError;

impl AppServices {
    /// One window of the download queue as the Downloads pane draws it: a
    /// range of the order queued, each entry beside its release. Coven reruns
    /// it as an entry in the range comes, goes, starts or fails, or a release
    /// it shows changes.
    pub fn subscribe_download_window(
        &self,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        super::list_reads::list_window(
            &self.inner.manager,
            self.inner.manager.subscribe_download_window(window),
            |read| bae_views::ListWindowValue::of(read, bae_views::ListRows::Downloads),
        )
    }

    /// How many entries the download queue holds.
    pub fn subscribe_download_count(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        super::list_reads::list_rows(
            &self.inner.manager,
            self.inner.manager.subscribe_download_row_count(),
        )
    }

    /// How many downloads come before the row at `key`.
    pub fn subscribe_download_position(
        &self,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        super::list_reads::list_position(
            &self.inner.manager,
            self.inner.manager.subscribe_download_position(key),
        )
    }

    delegate_async!(manager, download_key_at => download_key_at(position: u64) -> Result<Option<bae_store::list_windows::OrderKey>, LibraryError>);

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
