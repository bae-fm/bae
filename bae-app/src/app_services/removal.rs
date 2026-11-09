use super::*;
impl AppServices {
    /// Stop the library's services, leave its store and delete its files,
    /// telling `steps` as each step starts. `coven` is the installation the
    /// library was opened from.
    pub async fn remove_library(
        &self,
        coven: &coven::Coven,
        steps: &(dyn Fn(bae_library::ClosingStep) + Send + Sync),
    ) -> Result<(), bae_library::LibraryRemovalError> {
        self.stop_workers(steps).await.map_err(bae_library::LibraryRemovalError::Stop)?;
        self.inner.manager.remove_library(coven, steps).await
    }
    delegate_async!(manager, unsent_before_removal => unsent_before_removal() -> Result<coven::UnsentWork, bae_library::LibraryError>);
    delegate_async!(manager, standing => standing() -> Result<bae_library::LibraryStanding, bae_library::LibraryError>);
}
