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
    /// The CloudKit namespace and principal facts for this scope: which
    /// container and environment the app is bound to, which zone it is reading,
    /// and which user record the signed-in account is.
    fn provider_identity(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
    ) -> Result<BridgeCloudKitProviderIdentity, CloudKitError>;
    /// The accepted CKShare for a shared scope: the participant facts the host
    /// verified, plus the share record's exact canonical bytes. Those bytes are
    /// hashed into cross-device evidence, so they must be byte-stable across
    /// devices and repeated calls.
    fn accepted_read_write_share(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
    ) -> Result<BridgeCloudKitAcceptedShare, CloudKitError>;
    /// Read a record and return its opaque `recordChangeTag` alongside the bytes.
    fn read_versioned_record(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        key: String,
    ) -> Result<BridgeCloudVersionedObject, CloudKitError>;
    /// Open a host-local staging batch, returning its id. Staging creates no
    /// CloudKit records — the host holds the payloads until commit.
    fn begin_atomic_create(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
    ) -> Result<String, CloudKitError>;
    /// Stage one record payload in an open batch.
    fn stage_atomic_create_record(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        batch: String,
        record: BridgeCloudKitRecordCreate,
    ) -> Result<(), CloudKitError>;
    /// Create every staged record as one atomic zone modification, create-only.
    /// A known pre-commit failure must leave no record behind. If the commit
    /// response is lost, keep whatever landed — the caller reads the keys back
    /// to settle the outcome. Versions come back in staging order.
    fn commit_atomic_create(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        batch: String,
    ) -> Result<Vec<BridgeCloudKitRecordVersion>, CloudKitError>;
    /// Drop host-local staging. Never deletes records the batch may already have
    /// committed, and is idempotent.
    fn discard_atomic_create(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        batch: String,
    ) -> Result<(), CloudKitError>;
    /// Delete exactly these record versions as one atomic zone modification. A
    /// record that changed or vanished fails the whole deletion.
    fn delete_record_versions(
        &self,
        owner_name: Option<String>,
        zone_name: Option<String>,
        records: Vec<BridgeCloudKitRecordVersion>,
    ) -> Result<(), CloudKitError>;
    /// The share a member already holds, or `None` if none was granted.
    fn share_for_member(
        &self,
        member_pubkey: String,
    ) -> Result<Option<BridgeCloudKitShare>, CloudKitError>;
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

/// Which CloudKit deployment the container is bound to.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeCloudKitEnvironment {
    Development,
    Production,
}

/// The stable CloudKit facts behind a scope.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCloudKitProviderIdentity {
    pub container_id: String,
    pub environment: BridgeCloudKitEnvironment,
    pub owner_name: String,
    pub zone_name: String,
    pub current_user_record_name: String,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeCloudKitSharePermission {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeCloudKitShareAcceptance {
    Pending,
    Accepted,
}

/// An accepted CKShare as the host read it back, with the exact canonical
/// record bytes coven hashes into its cross-principal evidence.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCloudKitAcceptedShare {
    pub share_record_name: String,
    pub owner_name: String,
    pub zone_name: String,
    pub participant_record_name: String,
    pub permission: BridgeCloudKitSharePermission,
    pub acceptance: BridgeCloudKitShareAcceptance,
    pub canonical_record: Vec<u8>,
}

/// A record's bytes with the opaque provider revision they were read at.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCloudVersionedObject {
    pub bytes: Vec<u8>,
    /// CloudKit's `recordChangeTag`, opaque to coven.
    pub version: String,
}

/// One record named by the revision it was read at.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCloudKitRecordVersion {
    pub key: String,
    pub version: String,
}

/// One record payload staged for an atomic create.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCloudKitRecordCreate {
    pub key: String,
    pub data: Vec<u8>,
}

/// Adapts a UniFFI `CloudKitDriver` callback to `CloudKitOps` (bae-core).
struct CloudKitDriverAdapter {
    driver: Arc<dyn CloudKitDriver>,
}

impl coven::CloudKitOps for CloudKitDriverAdapter {
    fn provider_identity(
        &self,
        scope: &coven::CloudKitScope,
    ) -> Result<coven::CloudKitProviderIdentity, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .provider_identity(owner_name, zone_name)
            .map(BridgeCloudKitProviderIdentity::into_core)
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn accepted_read_write_share(
        &self,
        scope: &coven::CloudKitScope,
    ) -> Result<coven::CloudKitAcceptedShareRecord, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .accepted_read_write_share(owner_name, zone_name)
            .map(BridgeCloudKitAcceptedShare::into_core)
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn read_versioned_record(
        &self,
        scope: &coven::CloudKitScope,
        key: &str,
    ) -> Result<coven::CloudVersionedObject, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        let read = self
            .driver
            .read_versioned_record(owner_name, zone_name, key.to_string())
            .map_err(cloudkit_err_to_cloud_home_err)?;
        Ok(coven::CloudVersionedObject {
            bytes: read.bytes,
            version: coven::CloudObjectVersion::from_provider(read.version)?,
        })
    }

    fn begin_atomic_create(
        &self,
        scope: &coven::CloudKitScope,
    ) -> Result<coven::CloudKitAtomicCreateBatch, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        let batch = self
            .driver
            .begin_atomic_create(owner_name, zone_name)
            .map_err(cloudkit_err_to_cloud_home_err)?;
        coven::CloudKitAtomicCreateBatch::from_provider(batch)
    }

    fn stage_atomic_create_record(
        &self,
        scope: &coven::CloudKitScope,
        batch: &coven::CloudKitAtomicCreateBatch,
        record: coven::CloudKitRecordCreate,
    ) -> Result<(), coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .stage_atomic_create_record(
                owner_name,
                zone_name,
                batch.as_provider().to_string(),
                BridgeCloudKitRecordCreate {
                    key: record.key,
                    data: record.data,
                },
            )
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn commit_atomic_create(
        &self,
        scope: &coven::CloudKitScope,
        batch: &coven::CloudKitAtomicCreateBatch,
    ) -> Result<Vec<coven::CloudKitRecordVersion>, coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .commit_atomic_create(owner_name, zone_name, batch.as_provider().to_string())
            .map_err(cloudkit_err_to_cloud_home_err)?
            .into_iter()
            .map(BridgeCloudKitRecordVersion::into_core)
            .collect()
    }

    fn discard_atomic_create(
        &self,
        scope: &coven::CloudKitScope,
        batch: &coven::CloudKitAtomicCreateBatch,
    ) -> Result<(), coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        self.driver
            .discard_atomic_create(owner_name, zone_name, batch.as_provider().to_string())
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn delete_record_versions(
        &self,
        scope: &coven::CloudKitScope,
        records: &[coven::CloudKitRecordVersion],
    ) -> Result<(), coven::CloudHomeError> {
        let (owner_name, zone_name) = scope_fields(scope);
        let records = records
            .iter()
            .map(|record| BridgeCloudKitRecordVersion {
                key: record.key.clone(),
                version: record.version.as_provider().to_string(),
            })
            .collect();
        self.driver
            .delete_record_versions(owner_name, zone_name, records)
            .map_err(cloudkit_err_to_cloud_home_err)
    }

    fn share_for_member(
        &self,
        member_pubkey: &str,
    ) -> Result<Option<coven::CloudKitShare>, coven::CloudHomeError> {
        self.driver
            .share_for_member(member_pubkey.to_string())
            .map(|share| share.map(BridgeCloudKitShare::into_core))
            .map_err(cloudkit_err_to_cloud_home_err)
    }

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
            .map(BridgeCloudKitShare::into_core)
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
            .map(BridgeCloudKitShare::into_core)
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

impl BridgeCloudKitShare {
    fn into_core(self) -> coven::CloudKitShare {
        let BridgeCloudKitShare {
            share_url,
            owner_name,
            zone_name,
        } = self;
        coven::CloudKitShare {
            share_url,
            owner_name,
            zone_name,
        }
    }
}

impl BridgeCloudKitProviderIdentity {
    fn into_core(self) -> coven::CloudKitProviderIdentity {
        let BridgeCloudKitProviderIdentity {
            container_id,
            environment,
            owner_name,
            zone_name,
            current_user_record_name,
        } = self;
        coven::CloudKitProviderIdentity {
            container_id,
            environment: match environment {
                BridgeCloudKitEnvironment::Development => coven::CloudKitEnvironment::Development,
                BridgeCloudKitEnvironment::Production => coven::CloudKitEnvironment::Production,
            },
            owner_name,
            zone_name,
            current_user_record_name,
        }
    }
}

impl BridgeCloudKitAcceptedShare {
    fn into_core(self) -> coven::CloudKitAcceptedShareRecord {
        let BridgeCloudKitAcceptedShare {
            share_record_name,
            owner_name,
            zone_name,
            participant_record_name,
            permission,
            acceptance,
            canonical_record,
        } = self;
        coven::CloudKitAcceptedShareRecord {
            share_record_name,
            owner_name,
            zone_name,
            participant_record_name,
            permission: match permission {
                BridgeCloudKitSharePermission::ReadOnly => coven::CloudKitSharePermission::ReadOnly,
                BridgeCloudKitSharePermission::ReadWrite => {
                    coven::CloudKitSharePermission::ReadWrite
                }
            },
            acceptance: match acceptance {
                BridgeCloudKitShareAcceptance::Pending => coven::CloudKitShareAcceptance::Pending,
                BridgeCloudKitShareAcceptance::Accepted => coven::CloudKitShareAcceptance::Accepted,
            },
            canonical_record,
        }
    }
}

impl BridgeCloudKitRecordVersion {
    fn into_core(self) -> Result<coven::CloudKitRecordVersion, coven::CloudHomeError> {
        Ok(coven::CloudKitRecordVersion {
            key: self.key,
            version: coven::CloudObjectVersion::from_provider(self.version)?,
        })
    }
}

fn cloudkit_err_to_cloud_home_err(e: CloudKitError) -> coven::CloudHomeError {
    match e {
        CloudKitError::NotFound { msg } => coven::CloudHomeError::NotFound(msg),
        CloudKitError::Storage { msg } => coven::CloudHomeError::Transport(msg),
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
