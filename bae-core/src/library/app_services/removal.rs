use super::*;
impl AppServices {
    /// Stop the library's services, leave its store and delete its files,
    /// telling `steps` as each step starts.
    pub async fn remove_library(
        &self,
        steps: &(dyn Fn(crate::library::ClosingStep) + Send + Sync),
    ) -> Result<(), crate::library::LibraryRemovalError> {
        self.stop_workers(steps)
            .await
            .map_err(crate::library::LibraryRemovalError::Stop)?;
        self.inner.manager.remove_library(steps).await
    }
    delegate_async!(manager, unsent_before_removal => unsent_before_removal() -> Result<coven::UnsentWork, crate::library::LibraryError>);
}
