//! Where this process keeps coven's keys and the restore codes saved for the
//! person, and which of those codes a device offers to restore.

use bae_app_dir::AppDir;

/// Where this process keeps coven's keys, credentials and saved restore
/// codes: the OS keychain, or for UI and bridge tests an in-memory keychain of
/// their own, so a test never reads or writes the person's login keychain.
#[derive(Clone)]
pub enum Keychain {
    System,
    #[cfg(any(test, feature = "test-utils"))]
    InMemory(std::sync::Arc<coven::TestCoven>),
}

impl Keychain {
    /// An empty in-memory keychain, kept for this process only.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn in_memory() -> Self {
        Self::InMemory(std::sync::Arc::new(coven::TestCoven::new()))
    }

    /// A coven builder keeping keys in this keychain, for the functions here
    /// that create, open, restore or join a library: each configures it as
    /// bae's store (`configure_store`).
    pub fn store_builder(&self, app_dir: &AppDir) -> coven::CovenBuilder {
        let layout = app_dir.store_layout();
        match self {
            Self::System => coven::Coven::builder(layout),
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.builder(layout),
        }
    }

    /// Delete a closed store without opening it: this keychain's entries for
    /// it, then its directory.
    pub(crate) async fn discard_store(
        &self,
        directory: &coven::StoreDir,
    ) -> Result<(), coven::StoreDeletionError> {
        match self {
            Self::System => coven::Coven::discard_store(directory).await,
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.discard_store(directory).await,
        }
    }

    /// The restore codes saved for the person (iCloud Keychain on Apple,
    /// this phone's Block Store on Android) for libraries not already on this device: a library this device has
    /// is opened, not restored. An entry coven can't restore from is listed
    /// as unreadable, with its library's id and why, so the person can
    /// delete it. A phone without Google Play services has no Block Store
    /// and keeps no saved codes, so it lists none.
    pub async fn saved_restore_codes(
        &self,
        app_dir: &AppDir,
    ) -> Result<Vec<coven::SavedRestoreEntry>, SavedRestoreCodesError> {
        let codes = match self {
            Self::System => match coven::saved_restore_codes().await {
                Err(error) if keeps_no_saved_codes(&error) => {
                    tracing::info!(
                        "this device keeps no saved restore codes: it has no Block Store"
                    );
                    Vec::new()
                }
                listed => listed?,
            },
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.saved_restore_codes().await?,
        };
        restorable_codes(codes, &app_dir.store_layout()).await
    }

    /// Delete a library's saved restore code, and the S3 key saved beside it,
    /// from where it is saved: on Apple, from iCloud Keychain, and so from
    /// every one of the person's Apple devices; on Android, from this phone's
    /// Block Store. It changes no library, storage or device's copy, and
    /// needs no open library. An unreadable entry is deleted the same way. An
    /// entry already gone succeeds, and calling again finishes one that
    /// failed partway.
    pub async fn forget_saved_restore_code(
        &self,
        store: coven::StoreId,
    ) -> Result<(), ForgetSavedRestoreCodeError> {
        match self {
            Self::System => coven::forget_saved_restore_code(store).await?,
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.forget_saved_restore_code(store).await?,
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SavedRestoreCodesError {
    #[error(transparent)]
    Keychain(#[from] coven::BootstrapError),
    #[error(transparent)]
    Stores(#[from] coven::StoreLayoutError),
}

impl SavedRestoreCodesError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Keychain(error) => bae_diagnostics::bootstrap_category(error),
            Self::Stores(_) => bae_diagnostics::UiErrorCategory::Config,
        }
    }
}

/// Why a saved restore code couldn't be deleted from the keychain.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ForgetSavedRestoreCodeError(#[from] coven::BootstrapError);

impl ForgetSavedRestoreCodeError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        bae_diagnostics::bootstrap_category(&self.0)
    }
}

/// Whether listing the saved codes failed only because this device has
/// nowhere to keep them: coven answers `Unsupported` there, which says the
/// device keeps no saved codes, not that reading them failed.
fn keeps_no_saved_codes(error: &coven::BootstrapError) -> bool {
    matches!(error, coven::BootstrapError::SecureStorage(coven::KeyError::Unsupported))
}

async fn restorable_codes(
    codes: Vec<coven::SavedRestoreEntry>,
    layout: &coven::StoreLayout,
) -> Result<Vec<coven::SavedRestoreEntry>, SavedRestoreCodesError> {
    let here: std::collections::HashSet<_> =
        layout.stores().await?.into_iter().map(|store| store.id).collect();
    Ok(codes.into_iter().filter(|code| !here.contains(&code.store_id())).collect())
}

#[cfg(test)]
#[path = "keychain_tests.rs"]
mod tests;
