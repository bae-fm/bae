use std::sync::Arc;

use crate::types::CloudKitError;

// =========================================================================
// CloudKit driver: UniFFI callback interface bridging Swift -> Rust
// =========================================================================

/// Synchronous CloudKit operations, implemented in Swift via UniFFI callback.
#[uniffi::export(callback_interface)]
pub trait CloudKitDriver: Send + Sync {
    fn write_record(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        key: String,
        data: Vec<u8>,
    ) -> Result<(), CloudKitError>;
    fn read_record(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        key: String,
    ) -> Result<Vec<u8>, CloudKitError>;
    fn list_records(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        prefix: String,
    ) -> Result<Vec<String>, CloudKitError>;
    fn delete_record(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        key: String,
    ) -> Result<(), CloudKitError>;
    fn record_exists(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        key: String,
    ) -> Result<bool, CloudKitError>;
    fn grant_share(&self, member_pubkey: String) -> Result<BridgeCloudKitShare, CloudKitError>;
    fn revoke_share(&self, member_pubkey: String) -> Result<(), CloudKitError>;
    fn accept_share(&self, share_url: String) -> Result<BridgeCloudKitShare, CloudKitError>;
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCloudKitShare {
    pub share_url: String,
    pub owner_name: String,
    pub zone_name: String,
}

/// Adapts a UniFFI `CloudKitDriver` callback to `CloudKitOps` (bae-core).
struct CloudKitDriverAdapter {
    driver: Arc<dyn CloudKitDriver>,
}

impl coven::CloudKitOps for CloudKitDriverAdapter {
    fn write_record(
        &self,
        scope: &coven::CloudKitScope,
        key: &str,
        data: Vec<u8>,
    ) -> Result<(), coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .write_record(owner_name, zone_name, key.to_string(), data)
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn read_record(
        &self,
        scope: &coven::CloudKitScope,
        key: &str,
    ) -> Result<Vec<u8>, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .read_record(owner_name, zone_name, key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn list_records(
        &self,
        scope: &coven::CloudKitScope,
        prefix: &str,
    ) -> Result<Vec<String>, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .list_records(owner_name, zone_name, prefix.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn delete_record(
        &self,
        scope: &coven::CloudKitScope,
        key: &str,
    ) -> Result<(), coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .delete_record(owner_name, zone_name, key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn record_exists(
        &self,
        scope: &coven::CloudKitScope,
        key: &str,
    ) -> Result<bool, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .record_exists(owner_name, zone_name, key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn grant_share(
        &self,
        member_pubkey: &str,
    ) -> Result<coven::CloudKitShare, coven::CloudHomeError> {
        self.driver
            .grant_share(member_pubkey.to_string())
            .map(bridge_share_to_coven)
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn revoke_share(&self, member_pubkey: &str) -> Result<(), coven::CloudHomeError> {
        self.driver
            .revoke_share(member_pubkey.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn accept_share(&self, share_url: &str) -> Result<coven::CloudKitShare, coven::CloudHomeError> {
        self.driver
            .accept_share(share_url.to_string())
            .map(bridge_share_to_coven)
            .map_err(cloudkit_err_to_cloud_home_err)
    }
}

fn scope_fields(scope: &coven::CloudKitScope) -> (Option<String>, Option<String>) {
    match scope {
        coven::CloudKitScope::Private => (None, None),
        coven::CloudKitScope::Shared {
            owner_name,
            zone_name,
        } => (Some(owner_name.clone()), Some(zone_name.clone())),
    }
}

fn bridge_share_to_coven(share: BridgeCloudKitShare) -> coven::CloudKitShare {
    coven::CloudKitShare {
        share_url: share.share_url,
        owner_name: share.owner_name,
        zone_name: share.zone_name,
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

fn cloudkit_driver_slot() -> std::sync::MutexGuard<'static, Option<Arc<dyn CloudKitDriver>>> {
    CLOUDKIT_DRIVER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Register the CloudKit driver. Call before `initApp()` on CloudKit-backed libraries.
#[uniffi::export]
pub fn set_cloudkit_driver(driver: Box<dyn CloudKitDriver>) {
    *cloudkit_driver_slot() = Some(Arc::from(driver));
}

/// Get a CloudKit ops adapter, if a driver has been registered.
pub(crate) fn get_cloudkit_ops() -> Option<Arc<dyn coven::CloudKitOps>> {
    let driver = cloudkit_driver_slot().clone()?;
    Some(Arc::new(CloudKitDriverAdapter { driver }))
}
