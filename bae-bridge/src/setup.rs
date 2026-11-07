//! Onboarding records and opaque ownership of a store returned by coven.
use crate::{
    BridgeCloudProvider, BridgeError, BridgeHost, BridgeLibrary, BridgeOAuthAccess,
    BridgeStorageLocation,
};
use std::sync::Arc;

#[uniffi::export]
pub fn available_cloud_providers() -> Vec<BridgeCloudProvider> {
    vec![
        #[cfg(feature = "cloudkit")]
        BridgeCloudProvider::CloudKit,
        #[cfg(feature = "oauth-providers")]
        BridgeCloudProvider::Dropbox,
        #[cfg(feature = "oauth-providers")]
        BridgeCloudProvider::OneDrive,
        BridgeCloudProvider::S3,
    ]
}

impl BridgeLibrary {
    pub(crate) fn from_core_info(info: bae_app_dir::LibraryInfo) -> Result<Self, BridgeError> {
        let bae_app_dir::LibraryInfo { id, name, path, is_active, state } = info;
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
            state: match state {
                bae_app_dir::LibraryState::Openable(storage) => {
                    crate::types::BridgeLibraryState::Openable {
                        storage: storage.map(BridgeStorageLocation::from_core),
                    }
                }
                bae_app_dir::LibraryState::Unreadable(error) => {
                    crate::types::BridgeLibraryState::Unreadable {
                        error: BridgeError::from_core(error),
                    }
                }
                bae_app_dir::LibraryState::ReplacementUnfinished(by) => {
                    crate::types::BridgeLibraryState::ReplacementUnfinished {
                        by: crate::types::BridgeReplacedBy::from_core(by),
                    }
                }
                bae_app_dir::LibraryState::DeletionUnfinished => {
                    crate::types::BridgeLibraryState::DeletionUnfinished
                }
            },
        })
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCodeMetadata {
    pub library_id: String,
    pub member_id: String,
    pub provider: BridgeCloudProvider,
    /// Where the code's library is stored, with the account it was made for.
    pub location: BridgeStorageLocation,
    pub oauth_access: Option<BridgeOAuthAccess>,
    /// Whether the code holds an S3 key, so the device asks for none: every
    /// S3 invite does, and the restore code the person's own device shows.
    pub carries_s3_key: bool,
}
impl BridgeCodeMetadata {
    pub(crate) fn from_core(value: coven::CodeMetadata) -> Self {
        Self {
            library_id: value.store_id.to_string(),
            member_id: value.member.to_string(),
            provider: BridgeCloudProvider::from_core(&value.cloud_provider),
            location: BridgeStorageLocation::from_core(value.location),
            oauth_access: value.oauth_access.map(BridgeOAuthAccess::from_core),
            carries_s3_key: value.carries_s3_key,
        }
    }
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCodeInfo {
    Restore { metadata: BridgeCodeMetadata },
    Invite { invite: BridgeInviteInfo },
}
/// What an invite tells before connecting: the library's name as the invite
/// carried it, the person's name, and when it expires.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInviteInfo {
    pub metadata: BridgeCodeMetadata,
    pub library_name: String,
    /// `None` for a replacement: the person already has a profile.
    pub invitee: Option<String>,
    /// When the invite ends, by the inviting device's clock: a join waits
    /// for approval until then.
    pub expires_at: std::time::SystemTime,
}
impl BridgeInviteInfo {
    pub(crate) fn from_core(value: coven::InviteInfo) -> Self {
        Self {
            metadata: BridgeCodeMetadata::from_core(value.metadata),
            library_name: value.store_name,
            invitee: value.invitee,
            expires_at: value.expires_at,
        }
    }
}
#[uniffi::export]
pub fn decode_code_info(code: String) -> Result<BridgeCodeInfo, BridgeError> {
    Ok(match bae_library::sync::decode_code_info(&code).map_err(BridgeError::config)? {
        coven::CodeInfo::Restore(metadata) => {
            BridgeCodeInfo::Restore { metadata: BridgeCodeMetadata::from_core(metadata) }
        }
        coven::CodeInfo::Invite(invite) => {
            BridgeCodeInfo::Invite { invite: BridgeInviteInfo::from_core(invite) }
        }
    })
}

/// A store a create, restore, join or device reset left open, held until an
/// open hands it to the library it opens. Released before that, its store is
/// closed on a thread the host waits for before opening or removing the store
/// again, as a released [`crate::AppHandle`]'s is.
#[derive(uniffi::Object)]
pub struct OpenedLibrary {
    store: coven::StoreId,
    /// The open store, until an open takes it; a failed open leaves it here.
    handle: std::sync::Mutex<Option<coven::CovenHandle>>,
    host: Arc<BridgeHost>,
}
impl OpenedLibrary {
    pub(crate) fn new(handle: coven::CovenHandle, host: Arc<BridgeHost>) -> Self {
        Self { store: handle.store_id(), handle: std::sync::Mutex::new(Some(handle)), host }
    }
}
#[uniffi::export]
impl OpenedLibrary {
    pub fn library_id(&self) -> String {
        self.store.to_string()
    }

    /// Open the library on the store, which the library then owns. Once one
    /// open has, the store is no longer this object's to open again.
    pub fn open(
        &self,
        position_update_interval_ms: u32,
        restore_playback: bool,
    ) -> Result<Arc<crate::AppHandle>, BridgeError> {
        let mut held = self.handle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let handle = held.as_ref().ok_or_else(|| {
            BridgeError::from(bae_library::LibraryError::Internal(
                "this library was already opened".to_string(),
            ))
        })?;
        let opened = self.host.open_prepared(
            handle.clone(),
            position_update_interval_ms,
            restore_playback,
        )?;
        *held = None;
        Ok(opened)
    }
}

impl Drop for OpenedLibrary {
    fn drop(&mut self) {
        let held = self.handle.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        if let Some(handle) = held {
            self.host.close_released(handle);
        }
    }
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
