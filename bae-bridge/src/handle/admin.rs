use super::*;
use crate::types::*;
forward! {async this=>{
    fn storage_location()->Option<BridgeStorageLocation> {Ok(this.services.storage_location().await?.map(BridgeStorageLocation::from_core))}
} }
