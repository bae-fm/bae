//! Provider operations and the upload pause over coven's store handle.

use crate::config::CloudProvider;
use crate::db::Database;
use crate::diagnostics::{Diagnostics, TelemetryEvent};
use crate::library::LibraryError;
use crate::sync::S3ConfigData;

/// Owns the cloud-connection lifecycle. Holds clones of the handles the sync
/// paths need (database and diagnostics). Cloned alongside the manager —
/// every field is itself a clone-shared handle.
#[derive(Clone)]
pub(crate) struct SyncController {
    database: Database,
    /// Typed telemetry sink, shared with the owning manager. The
    /// provider-connect/disconnect completions emit through it.
    diagnostics: Diagnostics,
}

impl SyncController {
    pub(crate) fn new(database: Database, diagnostics: Diagnostics) -> Self {
        Self {
            database,
            diagnostics,
        }
    }

    pub(crate) async fn set_sync_paused(&self, paused: bool) -> Result<(), LibraryError> {
        self.database.set_uploads_paused(paused).await?;
        Ok(())
    }

    pub(crate) async fn is_sync_paused(&self) -> Result<bool, LibraryError> {
        Ok(self.database.uploads_paused().await?)
    }

    pub(crate) async fn save_s3_config(
        &self,
        data: S3ConfigData,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let (storage, credentials) = data
            .into_location()
            .map_err(|error| LibraryError::Validation(format!("invalid S3 endpoint: {error}")))?;
        let setup = self
            .database
            .setup_s3_storage(
                storage,
                device_name,
                credentials.access_key_id,
                credentials.secret_access_key,
            )
            .await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::S3,
            });
        Ok(setup)
    }

    /// Sign in to an account provider before choosing a new location on it.
    /// bae registers a Full Dropbox app, since a household's library lives in
    /// a folder its members share.
    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn sign_in(&self, provider: CloudProvider) -> Result<(), LibraryError> {
        let access = match provider {
            CloudProvider::Dropbox => {
                coven::OAuthAccess::Dropbox(coven::DropboxAccess::FullDropbox)
            }
            CloudProvider::OneDrive => coven::OAuthAccess::OneDrive,
            CloudProvider::S3 | CloudProvider::CloudKit => {
                return Err(LibraryError::Internal(format!(
                    "{provider:?} has no account sign-in"
                )))
            }
        };
        self.database.authenticate(access).await?;
        Ok(())
    }

    /// Sign in again to the provider this library's location is on, with the
    /// access that location needs, and reconnect the location with the new
    /// sign-in.
    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn sign_in_again(
        &self,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let storage = self.database.storage_config().await?.ok_or_else(|| {
            LibraryError::Internal("signing in again needs a library with storage".into())
        })?;
        let access = storage.oauth_access().ok_or_else(|| {
            LibraryError::Internal(format!(
                "{:?} storage has no account sign-in",
                storage.provider()
            ))
        })?;
        self.database.authenticate(access).await?;
        self.setup_oauth_storage(storage, device_name).await
    }

    #[cfg(feature = "oauth-providers")]
    pub(crate) async fn setup_oauth_storage(
        &self,
        storage: coven::StorageConfig,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let provider = storage.provider();
        let setup = self
            .database
            .setup_oauth_storage(storage, device_name)
            .await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected { provider });
        Ok(setup)
    }

    pub(crate) async fn use_cloudkit(
        &self,
        device_name: &str,
    ) -> Result<coven::StorageSetup, LibraryError> {
        let setup = self.database.setup_cloudkit_storage(device_name).await?;
        self.diagnostics
            .event(TelemetryEvent::CloudProviderConnected {
                provider: CloudProvider::CloudKit,
            });
        Ok(setup)
    }

    pub(crate) async fn disconnect_cloud_provider(&self) -> Result<(), LibraryError> {
        let provider = self
            .database
            .storage_config()
            .await?
            .map(|storage| storage.provider());
        self.database.disconnect_storage().await?;
        if let Some(provider) = provider {
            self.diagnostics
                .event(TelemetryEvent::CloudProviderDisconnected { provider });
        }
        Ok(())
    }

    pub(crate) async fn attach_and_start_sync(&self) -> Result<(), LibraryError> {
        self.database.start_sync().await?;
        Ok(())
    }

    pub(crate) async fn unlock_store_key(&self) -> Result<(), LibraryError> {
        self.database.unlock_store_key().await?;
        Ok(())
    }
}
