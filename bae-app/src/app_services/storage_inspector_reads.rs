//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;

impl AppServices {
    /// What the inspector shows of the release the subscription is set to,
    /// as it changes; see
    /// [`bae_library::library::live_reads::subscribe_storage_inspector`].
    pub fn subscribe_storage_inspector(
        &self,
    ) -> bae_library::library::LiveDetail<bae_library::library::storage_inspector::StorageInspector>
    {
        bae_library::library::live_reads::subscribe_storage_inspector(
            &self.inner.manager,
            &self.inner.views,
        )
    }
}
