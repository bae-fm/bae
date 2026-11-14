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
    /// The libraries as the list shows them, in the order core lists them,
    /// with the active library's neighbours among the ones that open.
    pub(crate) fn list_from_core(
        infos: Vec<bae_app_dir::LibraryInfo>,
    ) -> Result<Vec<Self>, BridgeError> {
        let mut libraries =
            infos.into_iter().map(Self::from_core_info).collect::<Result<Vec<_>, _>>()?;
        let several = libraries.len() > 1;
        for library in &mut libraries {
            library.has_other_libraries = several;
        }
        let openable: Vec<usize> = libraries
            .iter()
            .enumerate()
            .filter(|(_, library)| {
                matches!(library.state, crate::types::BridgeLibraryState::Openable { .. })
            })
            .map(|(index, _)| index)
            .collect();
        let active = openable.iter().position(|&index| libraries[index].is_active);
        if let (Some(active), true) = (active, openable.len() > 1) {
            let next = libraries[openable[(active + 1) % openable.len()]].id.clone();
            let previous =
                libraries[openable[(active + openable.len() - 1) % openable.len()]].id.clone();
            let library = &mut libraries[openable[active]];
            library.next_openable_id = Some(next);
            library.previous_openable_id = Some(previous);
        }
        Ok(libraries)
    }

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
            has_other_libraries: false,
            next_openable_id: None,
            previous_openable_id: None,
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
                bae_app_dir::LibraryState::NameUnreadable => {
                    crate::types::BridgeLibraryState::NameUnreadable
                }
                bae_app_dir::LibraryState::ReplacementUnfinished(by) => {
                    let by = crate::types::BridgeReplacedBy::from_core(by);
                    crate::types::BridgeLibraryState::ReplacementUnfinished {
                        line_key: crate::types::bridge_error_category_key(
                            crate::types::BridgeErrorCategory::ReplacementUnfinished { by },
                        ),
                        by,
                    }
                }
                bae_app_dir::LibraryState::DeletionUnfinished => {
                    crate::types::BridgeLibraryState::DeletionUnfinished {
                        line_key: crate::types::bridge_error_category_key(
                            crate::types::BridgeErrorCategory::DeletionUnfinished,
                        ),
                    }
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
    Restore {
        metadata: BridgeCodeMetadata,
    },
    Invite {
        invite: BridgeInviteInfo,
    },
    /// The code another of the person's own devices shows to add this one.
    Pairing {
        metadata: BridgeCodeMetadata,
    },
}
/// What an invite tells before connecting: the library's name as the invite
/// carried it, and the name of the admin who made it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInviteInfo {
    pub metadata: BridgeCodeMetadata,
    pub library_name: String,
    /// The inviting admin's name, for the screens that say whom to tell the
    /// code and whom the join waits for.
    pub inviter: String,
}
impl BridgeInviteInfo {
    pub(crate) fn from_core(value: coven::InviteInfo) -> Self {
        Self {
            metadata: BridgeCodeMetadata::from_core(value.metadata),
            library_name: value.store_name,
            inviter: value.inviter,
        }
    }
}
#[uniffi::export]
pub fn decode_code_info(code: String) -> Result<BridgeCodeInfo, BridgeError> {
    Ok(match bae_library::sync::decode_code_info(&code).map_err(BridgeError::config)? {
        coven::CodeInfo::Pairing(pairing) => {
            BridgeCodeInfo::Pairing { metadata: BridgeCodeMetadata::from_core(pairing.metadata) }
        }
        coven::CodeInfo::Restore(metadata) => {
            BridgeCodeInfo::Restore { metadata: BridgeCodeMetadata::from_core(metadata) }
        }
        coven::CodeInfo::Invite(invite) => {
            BridgeCodeInfo::Invite { invite: BridgeInviteInfo::from_core(invite) }
        }
    })
}

/// A restore code or invite as a code field shows it: in groups of four with
/// a space between, whether it was typed in groups or run together.
#[uniffi::export]
pub fn code_in_groups(text: String) -> String {
    bae_library::sync::restore_code::grouped(&text)
}

/// A restore code or invite as a code field holds it, to pass on: the spaces
/// and line breaks of its groups dropped.
#[uniffi::export]
pub fn code_as_entered(text: String) -> String {
    bae_library::sync::restore_code::entered(&text)
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
