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
            state,
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
            state: match state {
                bae_core::config::LibraryState::Openable(storage) => {
                    crate::types::BridgeLibraryState::Openable {
                        storage: storage.map(BridgeStorageLocation::from_core),
                    }
                }
                bae_core::config::LibraryState::Unreadable(error) => {
                    crate::types::BridgeLibraryState::Unreadable {
                        error: BridgeError::from_core(error),
                    }
                }
                bae_core::config::LibraryState::ReplacementUnfinished(by) => {
                    crate::types::BridgeLibraryState::ReplacementUnfinished {
                        by: crate::types::BridgeReplacedBy::from_core(by),
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
    Restore { metadata: BridgeCodeMetadata },
    Invite { invite: BridgeInviteInfo },
}
/// What an invite tells before connecting: the library's name as the invite
/// carried it, the person's name, and in person or sent.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInviteInfo {
    pub metadata: BridgeCodeMetadata,
    pub library_name: String,
    /// `None` for a replacement: the person already has a profile.
    pub invitee: Option<String>,
    pub delivery: crate::types::BridgeInviteDeliveryKind,
}
impl BridgeInviteInfo {
    pub(crate) fn from_core(value: coven::InviteInfo) -> Self {
        Self {
            metadata: BridgeCodeMetadata::from_core(value.metadata),
            library_name: value.store_name,
            invitee: value.invitee,
            delivery: crate::types::BridgeInviteDeliveryKind::from_core(value.delivery),
        }
    }
}
#[uniffi::export]
pub fn decode_code_info(code: String) -> Result<BridgeCodeInfo, BridgeError> {
    Ok(
        match bae_core::sync::decode_code_info(&code).map_err(BridgeError::config)? {
            coven::CodeInfo::Restore(metadata) => BridgeCodeInfo::Restore {
                metadata: BridgeCodeMetadata::from_core(metadata),
            },
            coven::CodeInfo::Invite(invite) => BridgeCodeInfo::Invite {
                invite: BridgeInviteInfo::from_core(invite),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A library whose replacement of this device's copy is unfinished
    /// crosses with the call that finishes it.
    #[test]
    fn a_library_waiting_on_a_replacement_crosses_with_the_call_that_finishes_it() {
        for (core, bridge) in [
            (coven::ReplacedBy::Restore, crate::BridgeReplacedBy::Restore),
            (coven::ReplacedBy::Reset, crate::BridgeReplacedBy::Reset),
        ] {
            let library = BridgeLibrary::from_core_info(bae_core::config::LibraryInfo {
                id: "library".into(),
                name: "Household".into(),
                path: "/libraries/library".into(),
                is_active: true,
                state: bae_core::config::LibraryState::ReplacementUnfinished(core),
            })
            .unwrap();
            assert!(
                matches!(
                    library.state,
                    crate::BridgeLibraryState::ReplacementUnfinished { by } if by == bridge
                ),
                "{:?}",
                library.state
            );
        }
    }
}
