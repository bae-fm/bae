//! The Storage Manager's live read on [`AppServices`].

use super::*;

impl AppServices {
    /// The Storage Manager list as it changes, read in the view the
    /// subscription is asked for; see
    /// [`bae_library::library::live_reads::subscribe_storage_browse`].
    pub fn subscribe_storage_browse(
        &self,
        initial: bae_library::library::StorageBrowseView,
    ) -> bae_library::library::LiveStorageBrowse {
        bae_library::library::live_reads::subscribe_storage_browse(
            &self.inner.manager,
            &self.inner.views,
            initial,
        )
    }
}
