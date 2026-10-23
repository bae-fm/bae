use super::*;
use crate::types::*;
forward! {async this=>{
    fn detect_s3_settings(location:BridgeStorageLocation,credentials:BridgeS3Credentials)->std::sync::Arc<BridgeS3Detection> {
        Ok(BridgeS3Detection::from_core(this.services.detect_s3_settings(location.into_core()?,credentials.into_core()).await?))
    }
    fn storage_location()->Option<BridgeStorageLocation> {Ok(this.services.storage_location().await?.map(BridgeStorageLocation::from_core))}
} }
