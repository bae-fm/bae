use super::*;
impl AppServices {
    pub async fn remove_library(
        &self,
        unsent: coven::IfUnsent,
    ) -> Result<(), crate::library::LibraryRemovalError> {
        self.stop_workers().await;
        self.inner.manager.remove_library(unsent).await
    }
}
