//! The restore codes saved for the person, and which of them a device offers
//! to restore.

/// The restore codes saved for the person (iCloud Keychain on Apple, this
/// phone's Block Store on Android) for libraries not already in `coven`, this
/// device's installation: a library this device has is opened, not restored.
/// An entry coven can't restore from is listed as unreadable, with its
/// library's id and why, so the person can delete it. A phone without Google
/// Play services has no Block Store and keeps no saved codes, so it lists
/// none.
pub async fn saved_restore_codes(
    coven: &coven::Coven,
) -> Result<Vec<coven::SavedRestoreEntry>, SavedRestoreCodesError> {
    let codes = match coven.saved_restore_codes().await {
        Err(error) if keeps_no_saved_codes(&error) => {
            tracing::info!("this device keeps no saved restore codes: it has no Block Store");
            Vec::new()
        }
        listed => listed?,
    };
    restorable_codes(codes, coven).await
}

#[derive(Debug, thiserror::Error)]
pub enum SavedRestoreCodesError {
    #[error(transparent)]
    Keychain(#[from] coven::BootstrapError),
    #[error(transparent)]
    Stores(#[from] coven::StoreListError),
}

impl SavedRestoreCodesError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Keychain(error) => bae_diagnostics::bootstrap_category(error),
            Self::Stores(_) => bae_diagnostics::UiErrorCategory::Config,
        }
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
    coven: &coven::Coven,
) -> Result<Vec<coven::SavedRestoreEntry>, SavedRestoreCodesError> {
    let here: std::collections::HashSet<_> =
        coven.stores().await?.into_iter().map(|store| store.id()).collect();
    Ok(codes.into_iter().filter(|code| !here.contains(&code.store_id())).collect())
}

#[cfg(test)]
#[path = "saved_restore_codes_tests.rs"]
mod tests;
