//! The Storage Manager inspector's live read on [`AppServices`].

use super::*;

impl AppServices {
    /// What the inspector shows of the release the subscription is set to,
    /// as it changes; see
    /// [`bae_core::library::live_reads::subscribe_storage_inspector`].
    pub fn subscribe_storage_inspector(
        &self,
    ) -> bae_core::library::LiveDetail<bae_core::library::storage_inspector::StorageInspector> {
        bae_core::library::live_reads::subscribe_storage_inspector(
            &self.inner.manager,
            &self.inner.views,
        )
    }
}
