//! The Storage Manager's live read on [`AppServices`].

use super::*;

impl AppServices {
    /// The Storage Manager list as it changes, read in the view the
    /// subscription is asked for; see
    /// [`crate::live_reads::subscribe_storage_browse`].
    pub fn subscribe_storage_browse(
        &self,
        initial: bae_views::StorageBrowseRequest,
    ) -> crate::live_reads::LiveStorageBrowse {
        crate::live_reads::subscribe_storage_browse(&self.inner.manager, &self.inner.views, initial)
    }
}
