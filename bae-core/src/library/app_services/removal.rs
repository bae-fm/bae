use super::*;
impl AppServices {
    pub async fn remove_library(&self) -> Result<(), crate::library::LibraryRemovalError> {
        self.stop_workers().await;
        self.inner.manager.remove_library().await
    }
    delegate_async!(manager, unsent_before_removal => unsent_before_removal() -> Result<coven::UnsentWork, crate::library::LibraryError>);
}
