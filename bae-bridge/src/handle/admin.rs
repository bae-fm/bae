use super::*;
use crate::types::*;
forward! {async this=>{
    fn detect_s3_settings(location:BridgeStorageLocation,credentials:BridgeS3Credentials)->std::sync::Arc<BridgeS3Detection> {
        Ok(BridgeS3Detection::from_core(this.services.detect_s3_settings(location.into_core()?,credentials.into_core()).await?))
    }
    fn storage_location()->Option<BridgeStorageLocation> {Ok(this.services.storage_location().await?.map(BridgeStorageLocation::from_core))}
    fn retry_with_admin_access(operation:i64,access:BridgeAdminAccess)->() {Ok(this.services.retry_with_admin_access(coven::OperationId(operation),access.into_core()?).await?)}
    fn delete_access_key(operation:i64,access:BridgeAdminAccess)->() {Ok(this.services.delete_access_key(coven::OperationId(operation),access.into_core()?).await?)}
    fn confirm_access_key_deleted(access_key_id:String)->() {Ok(this.services.confirm_access_key_deleted(&access_key_id).await?)}
} }
