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

impl coven::CloudKitOps for CloudKitDriverAdapter {
    fn write_record(&self, key: &str, data: Vec<u8>) -> Result<(), coven::CloudHomeError> {
        self.driver
            .write_record(key.to_string(), data)
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn read_record(&self, key: &str) -> Result<Vec<u8>, coven::CloudHomeError> {
        self.driver
            .read_record(key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn list_records(&self, prefix: &str) -> Result<Vec<String>, coven::CloudHomeError> {
        self.driver
            .list_records(prefix.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn delete_record(&self, key: &str) -> Result<(), coven::CloudHomeError> {
        self.driver
            .delete_record(key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn record_exists(&self, key: &str) -> Result<bool, coven::CloudHomeError> {
        self.driver
            .record_exists(key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }
}

fn cloudkit_err_to_cloud_home_err(e: CloudKitError) -> coven::CloudHomeError {
    match e {
        CloudKitError::NotFound { msg } => coven::CloudHomeError::NotFound(msg),
        CloudKitError::Storage { msg } => coven::CloudHomeError::Storage(msg),
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
pub(crate) fn get_cloudkit_ops() -> Option<Arc<dyn coven::CloudKitOps>> {
    let driver = CLOUDKIT_DRIVER
        .lock()
        .expect("CloudKit driver mutex poisoned")
        .clone()?;
    Some(Arc::new(CloudKitDriverAdapter { driver }))
}
