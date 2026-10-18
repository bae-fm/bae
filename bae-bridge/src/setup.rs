//! Onboarding records and opaque ownership of a store returned by coven.
use crate::{
    BridgeCloudProvider, BridgeError, BridgeHost, BridgeLibrary, BridgeOAuthAccess,
    BridgeStorageLocation,
};
use std::sync::Arc;

#[uniffi::export]
pub fn available_cloud_providers() -> Vec<BridgeCloudProvider> {
    vec![
        BridgeCloudProvider::S3,
        #[cfg(feature = "cloudkit")]
        BridgeCloudProvider::CloudKit,
        #[cfg(feature = "oauth-providers")]
        BridgeCloudProvider::Dropbox,
        #[cfg(feature = "oauth-providers")]
        BridgeCloudProvider::OneDrive,
    ]
}

impl BridgeLibrary {
    pub(crate) fn from_core_info(info: bae_core::config::LibraryInfo) -> Result<Self, BridgeError> {
        let bae_core::config::LibraryInfo {
            id,
            name,
            path,
            is_active,
            settings,
        } = info;
        let path = path
            .to_str()
            .ok_or_else(|| {
                BridgeError::config(format!("Library path is not UTF-8: {}", path.display()))
            })?
            .to_owned();
        Ok(Self {
            id,
            name,
            path,
            is_active,
            settings: match settings {
                bae_core::config::LibrarySettings::Readable(storage) => {
                    crate::types::BridgeLibrarySettings::Readable {
                        storage: storage.map(BridgeStorageLocation::from_core),
                    }
                }
                bae_core::config::LibrarySettings::Unreadable(error) => {
                    crate::types::BridgeLibrarySettings::Unreadable {
                        error: BridgeError::from_core(error),
                    }
                }
            },
        })
    }
}

#[uniffi::export]
pub fn init_keyring(diagnostics: Arc<crate::BridgeDiagnostics>) -> Result<(), BridgeError> {
    diagnostics.init_keyring()
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCodeMetadata {
    pub library_id: String,
    pub member_id: String,
    pub provider: BridgeCloudProvider,
    /// Where the code's library is stored, with the account it was made for.
    pub location: BridgeStorageLocation,
    pub oauth_access: Option<BridgeOAuthAccess>,
}
impl BridgeCodeMetadata {
    fn from_core(value: coven::CodeMetadata) -> Self {
        Self {
            library_id: value.store_id.to_string(),
            member_id: value.member.to_string(),
            provider: BridgeCloudProvider::from_core(&value.cloud_provider),
            location: BridgeStorageLocation::from_core(value.location),
            oauth_access: value.oauth_access.map(BridgeOAuthAccess::from_core),
        }
    }
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCodeInfo {
    Restore {
        metadata: BridgeCodeMetadata,
    },
    Invite {
        metadata: BridgeCodeMetadata,
        library_name: String,
    },
}
#[uniffi::export]
pub fn decode_code_info(code: String) -> Result<BridgeCodeInfo, BridgeError> {
    Ok(
        match bae_core::sync::decode_code_info(&code).map_err(BridgeError::config)? {
            coven::CodeInfo::Restore(metadata) => BridgeCodeInfo::Restore {
                metadata: BridgeCodeMetadata::from_core(metadata),
            },
            coven::CodeInfo::Invite {
                metadata,
                store_name,
            } => BridgeCodeInfo::Invite {
                metadata: BridgeCodeMetadata::from_core(metadata),
                library_name: store_name,
            },
        },
    )
}

/// The live handle and the runtime holding its session custody stay together.
#[derive(uniffi::Object)]
pub struct OpenedLibrary {
    handle: coven::CovenHandle,
    host: Arc<BridgeHost>,
}
impl OpenedLibrary {
    pub(crate) fn new(handle: coven::CovenHandle, host: Arc<BridgeHost>) -> Self {
        Self { handle, host }
    }
}
#[uniffi::export]
impl OpenedLibrary {
    pub fn library_id(&self) -> String {
        self.handle.store_id().to_string()
    }
    pub fn open(
        &self,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> Result<Arc<crate::AppHandle>, BridgeError> {
        self.host.open_prepared(
            self.handle.clone(),
            position_update_interval_ms,
            restore_playback,
        )
    }
}
