//! Import domain operations for [`LibraryManager`].

impl super::LibraryManager {
    /// Store `release` with no row naming it, which only a test's setup
    /// does.
    #[cfg(test)]
    pub(crate) async fn save_source_release(
        &self,
        release: &crate::catalog_model::source_release::SourceRelease,
    ) -> Result<(), super::LibraryError> {
        Ok(self.database.save_source_release(release).await?)
    }
}
