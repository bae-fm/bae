use super::*;
impl AppServices {
    pub async fn remove_library(
        &self,
        unsent: coven::IfUnsent,
    ) -> Result<(), crate::library::LibraryRemovalError> {
        self.stop_workers().await;
        self.inner.manager.remove_library(unsent).await
    }
    delegate_async!(manager, finish_removed_library => finish_removed_library(library:crate::library::RemovedLibrary) -> Result<(),crate::library::LibraryRemovalError>);
}
