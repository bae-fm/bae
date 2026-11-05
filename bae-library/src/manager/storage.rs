//! Where a library's files are, for [`LibraryManager`]: the original a file
//! has on this device, and this device's id in the store.

use super::*;

impl LibraryManager {
    /// The path of the original coven records on this device for a file,
    /// whether or not it has been uploaded. `Ok(None)` means there is no
    /// original here or no such row; coven's cached copies expose no path.
    /// Database errors propagate.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn file_local_path(&self, file_id: &str) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self.records.external_file(file_id).await?.map(|ext| ext.path))
    }

    /// This device's id in the store.
    pub(crate) fn device_id(&self) -> coven::DeviceId {
        self.store.device_id()
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
