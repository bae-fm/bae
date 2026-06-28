use std::sync::Arc;

use crate::types::CloudKitError;

// =========================================================================
// CloudKit driver: UniFFI callback interface bridging Swift -> Rust
// =========================================================================

/// Synchronous CloudKit operations, implemented in Swift via UniFFI callback.
#[uniffi::export(callback_interface)]
pub trait CloudKitDriver: Send + Sync {
    fn write_record(&self, key: String, data: Vec<u8>) -> Result<(), CloudKitError>;
    fn read_record(&self, key: String) -> Result<Vec<u8>, CloudKitError>;
    fn list_records(&self, prefix: String) -> Result<Vec<String>, CloudKitError>;
    fn delete_record(&self, key: String) -> Result<(), CloudKitError>;
    fn record_exists(&self, key: String) -> Result<bool, CloudKitError>;
}

/// Adapts a UniFFI `CloudKitDriver` callback to `CloudKitOps` (bae-core).
struct CloudKitDriverAdapter {
    driver: Arc<dyn CloudKitDriver>,
}

impl bae_core::storage::cloud::CloudKitOps for CloudKitDriverAdapter {
    fn write_record(
        &self,
        key: &str,
        data: Vec<u8>,
    ) -> Result<(), bae_core::storage::cloud::CloudHomeError> {
        self.driver
            .write_record(key.to_string(), data)
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn read_record(&self, key: &str) -> Result<Vec<u8>, bae_core::storage::cloud::CloudHomeError> {
        self.driver
            .read_record(key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn list_records(
        &self,
        prefix: &str,
    ) -> Result<Vec<String>, bae_core::storage::cloud::CloudHomeError> {
        self.driver
            .list_records(prefix.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn delete_record(&self, key: &str) -> Result<(), bae_core::storage::cloud::CloudHomeError> {
        self.driver
            .delete_record(key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn record_exists(&self, key: &str) -> Result<bool, bae_core::storage::cloud::CloudHomeError> {
        self.driver
            .record_exists(key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    // coven's `CloudKitOps` trait declares the CKShare grant/revoke/accept
    // methods. bae uses a personal library on its own iCloud account, so all
    // three are unavailable; the Swift driver implements only the
    // private-database storage methods.
    fn grant_access(
        &self,
        _email: &str,
    ) -> Result<String, bae_core::storage::cloud::CloudHomeError> {
        Err(sharing_unsupported())
    }

    fn revoke_access(
        &self,
        _user_record_id: &str,
    ) -> Result<(), bae_core::storage::cloud::CloudHomeError> {
        Err(sharing_unsupported())
    }

    fn accept_share(
        &self,
        _share_url: &str,
    ) -> Result<(), bae_core::storage::cloud::CloudHomeError> {
        Err(sharing_unsupported())
    }
}

/// The error coven's `CloudKitOps` sharing methods return on a personal library:
/// bae has no library sharing, so grant/revoke/accept are unavailable.
fn sharing_unsupported() -> bae_core::storage::cloud::CloudHomeError {
    bae_core::storage::cloud::CloudHomeError::Storage(
        "CloudKit library sharing is not supported".to_string(),
    )
}

fn cloudkit_err_to_cloud_home_err(e: CloudKitError) -> bae_core::storage::cloud::CloudHomeError {
    match e {
        CloudKitError::NotFound { msg } => bae_core::storage::cloud::CloudHomeError::NotFound(msg),
        CloudKitError::Storage { msg } => bae_core::storage::cloud::CloudHomeError::Storage(msg),
    }
}

static CLOUDKIT_DRIVER: std::sync::Mutex<Option<Arc<dyn CloudKitDriver>>> =
    std::sync::Mutex::new(None);

/// Register the CloudKit driver. Call before `initApp()` on CloudKit-backed libraries.
#[uniffi::export]
pub fn set_cloudkit_driver(driver: Box<dyn CloudKitDriver>) {
    *CLOUDKIT_DRIVER
        .lock()
        .expect("CloudKit driver mutex poisoned") = Some(Arc::from(driver));
}

/// Get a CloudKit ops adapter, if a driver has been registered.
pub(crate) fn get_cloudkit_ops() -> Option<Arc<dyn bae_core::storage::cloud::CloudKitOps>>
{
    let driver = CLOUDKIT_DRIVER
        .lock()
        .expect("CloudKit driver mutex poisoned")
        .clone()?;
    Some(Arc::new(CloudKitDriverAdapter { driver }))
}
