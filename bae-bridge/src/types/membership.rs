use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMemberRole {
    Admin,
    Member,
}
mirror_enum! { BridgeMemberRole = coven::MemberRole,
    from_core: pub(crate) fn, into_core: pub(crate) fn, variants: { Admin, Member },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFormerDeviceState {
    Removed,
    Replaced,
}
mirror_enum! { BridgeFormerDeviceState = bae_core::sync::membership::FormerDeviceState,
    from_core: fn, variants: { Removed, Replaced },
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMemberPublicKeys {
    pub signing: Vec<u8>,
    pub sealing: Vec<u8>,
}
impl BridgeMemberPublicKeys {
    pub(crate) fn from_core(keys: coven::MemberPublicKeys) -> Self {
        Self {
            signing: keys.signing.as_bytes().to_vec(),
            sealing: keys.sealing.as_bytes().to_vec(),
        }
    }
}
/// A device that can still sync.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeActiveDevice {
    pub id: u64,
    pub name: String,
    pub is_self: bool,
    pub can_remove: bool,
}
impl BridgeActiveDevice {
    fn from_core(device: bae_core::sync::membership::ActiveDevice) -> Self {
        let bae_core::sync::membership::ActiveDevice {
            id,
            name,
            is_self,
            can_remove,
        } = device;
        Self {
            id: id.0,
            name,
            is_self,
            can_remove,
        }
    }
}
/// A removed or replaced device, which no longer has access.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFormerDevice {
    pub id: u64,
    pub name: String,
    pub state: BridgeFormerDeviceState,
}
impl BridgeFormerDevice {
    fn from_core(device: bae_core::sync::membership::FormerDevice) -> Self {
        let bae_core::sync::membership::FormerDevice { id, name, state } = device;
        Self {
            id: id.0,
            name,
            state: BridgeFormerDeviceState::from_core(state),
        }
    }
}
/// How a member, or an invite's person, reaches storage. On S3 only the key's
/// public id crosses, never its secret.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMemberAccess {
    ProviderAccount { email: String },
    S3AccessKey { access_key_id: String },
}
impl BridgeMemberAccess {
    pub(crate) fn from_core(value: coven::MemberAccess) -> Self {
        match value {
            coven::MemberAccess::ProviderAccount(email) => Self::ProviderAccount { email },
            coven::MemberAccess::S3AccessKey { access_key_id } => {
                Self::S3AccessKey { access_key_id }
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMember {
    pub id: String,
    pub keys: BridgeMemberPublicKeys,
    pub role: BridgeMemberRole,
    pub access: BridgeMemberAccess,
    pub is_self: bool,
    pub can_remove: bool,
    pub active_devices: Vec<BridgeActiveDevice>,
    pub former_devices: Vec<BridgeFormerDevice>,
}
impl BridgeMember {
    fn from_core(value: bae_core::sync::membership::MembershipMember) -> Self {
        let bae_core::sync::membership::MembershipMember {
            id,
            keys,
            role,
            access,
            is_self,
            can_remove,
            active_devices,
            former_devices,
        } = value;
        Self {
            id: id.0.to_string(),
            keys: BridgeMemberPublicKeys::from_core(keys),
            role: BridgeMemberRole::from_core(role),
            access: BridgeMemberAccess::from_core(access),
            is_self,
            can_remove,
            active_devices: active_devices
                .into_iter()
                .map(BridgeActiveDevice::from_core)
                .collect(),
            former_devices: former_devices
                .into_iter()
                .map(BridgeFormerDevice::from_core)
                .collect(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMembership {
    pub members: Vec<BridgeMember>,
    pub self_is_admin: bool,
}
impl BridgeMembership {
    pub(crate) fn from_core(value: bae_core::sync::membership::Membership) -> Self {
        let bae_core::sync::membership::Membership {
            members,
            self_is_admin,
        } = value;
        Self {
            members: members.into_iter().map(BridgeMember::from_core).collect(),
            self_is_admin,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePerson {
    pub id: String,
    pub profile: Option<BridgePersonProfile>,
    pub membership: Option<BridgeMember>,
}
impl BridgePerson {
    pub(crate) fn from_core(value: bae_core::people::Person) -> Self {
        let bae_core::people::Person {
            id,
            profile,
            membership,
        } = value;
        Self {
            id: id.0.to_string(),
            profile: profile.map(BridgePersonProfile::from_core),
            membership: membership.map(BridgeMember::from_core),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeProviderSignOut {
    RemoveAppAccess { provider: BridgeCloudProvider },
    RemoveFromAppleAccount,
    ReplaceAccessKey,
}
impl BridgeProviderSignOut {
    pub(crate) fn from_core(value: coven::ProviderSignOut) -> Self {
        match value {
            coven::ProviderSignOut::RemoveAppAccess { provider } => Self::RemoveAppAccess {
                provider: BridgeCloudProvider::from_core(&provider),
            },
            coven::ProviderSignOut::RemoveFromAppleAccount => Self::RemoveFromAppleAccount,
            coven::ProviderSignOut::ReplaceAccessKey => Self::ReplaceAccessKey,
        }
    }
}
#[derive(Clone, uniffi::Enum)]
pub enum BridgeInviteAccess {
    ProviderAccount {
        email: String,
    },
    S3AccessKey {
        access_key_id: String,
        secret_access_key: String,
    },
}
impl BridgeInviteAccess {
    pub(crate) fn into_core(self) -> coven::InviteAccess {
        match self {
            Self::ProviderAccount { email } => coven::InviteAccess::ProviderAccount { email },
            Self::S3AccessKey {
                access_key_id,
                secret_access_key,
            } => coven::InviteAccess::S3AccessKey {
                access_key_id,
                secret_access_key: coven::SecretText::new(secret_access_key),
            },
        }
    }
}
#[derive(Clone, uniffi::Record)]
pub struct BridgeInvite {
    pub id: String,
    pub member: String,
    pub code: String,
    pub expires_at: std::time::SystemTime,
}
impl BridgeInvite {
    pub(crate) fn from_core(value: coven::Invite) -> Self {
        Self {
            id: value.id.0.to_string(),
            member: value.member.0.to_string(),
            code: value.code,
            expires_at: value.expires_at,
        }
    }
}

/// Instructions returned after removing a device. The provider is available
/// separately for its brand name; the instruction itself is translated.
#[uniffi::export]
pub fn bridge_provider_sign_out_key(instruction: BridgeProviderSignOut) -> String {
    match instruction {
        BridgeProviderSignOut::RemoveAppAccess { .. } => "core.device.sign_out.remove_app_access",
        BridgeProviderSignOut::RemoveFromAppleAccount => "core.device.sign_out.apple",
        BridgeProviderSignOut::ReplaceAccessKey => "core.device.sign_out.access_key",
    }
    .to_string()
}
