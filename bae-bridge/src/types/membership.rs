use super::*;
use std::sync::Arc;

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
    pub(crate) fn from_core(device: bae_core::sync::membership::ActiveDevice) -> Self {
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
/// Someone in the library: their membership, and their profile once they
/// made one. Mirrors `Person`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePerson {
    pub profile: Option<BridgePersonProfile>,
    pub membership: BridgeMember,
}
impl BridgePerson {
    pub(crate) fn from_core(value: bae_core::people::Person) -> Self {
        let bae_core::people::Person {
            profile,
            membership,
        } = value;
        Self {
            profile: profile.map(BridgePersonProfile::from_core),
            membership: BridgeMember::from_core(membership),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeProviderSignOut {
    RemoveAppAccess { provider: BridgeCloudProvider },
    RemoveFromAppleAccount,
}
impl BridgeProviderSignOut {
    pub(crate) fn from_core(value: coven::ProviderSignOut) -> Self {
        match value {
            coven::ProviderSignOut::RemoveAppAccess { provider } => Self::RemoveAppAccess {
                provider: BridgeCloudProvider::from_core(&provider),
            },
            coven::ProviderSignOut::RemoveFromAppleAccount => Self::RemoveFromAppleAccount,
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
/// How an invite reaches the person, which fixes how it is approved: in
/// person, the inviting device lets them in itself; sent, the admin approves
/// after comparing the confirmation number (§5.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInviteDelivery {
    InPerson,
    Sent { lifetime: BridgeInviteLifetime },
}
impl BridgeInviteDelivery {
    pub(crate) fn into_core(self) -> coven::InviteDelivery {
        match self {
            Self::InPerson => coven::InviteDelivery::InPerson,
            Self::Sent { lifetime } => coven::InviteDelivery::Sent {
                lifetime: lifetime.into_core(),
            },
        }
    }
}
/// How long a sent invite stays open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInviteLifetime {
    Hour,
    Day,
    Week,
}
mirror_enum! { BridgeInviteLifetime = coven::InviteLifetime,
    into_core: pub(crate) fn, variants: { Hour, Day, Week },
}
/// In person or sent, as an invite carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInviteDeliveryKind {
    InPerson,
    Sent,
}
mirror_enum! { BridgeInviteDeliveryKind = coven::InviteDeliveryKind,
    from_core: pub(crate) fn, variants: { InPerson, Sent },
}
/// One of this device's open invites.
#[derive(Clone, uniffi::Record)]
pub struct BridgeInvite {
    pub id: String,
    pub member: String,
    /// The person's name; `None` for a replacement.
    pub invitee: Option<String>,
    pub delivery: BridgeInviteDeliveryKind,
    /// The access the invite grants; on S3 the id of the key it carries.
    pub access: BridgeMemberAccess,
    /// The code as first shown, to show again.
    pub code: String,
    pub expires_at: std::time::SystemTime,
    /// A sent invite's checked request, waiting for the admin to approve or
    /// decline; an in-person invite's needs nobody.
    pub request_to_decide: Option<Arc<crate::BridgeJoinRequest>>,
}
impl BridgeInvite {
    pub(crate) fn from_core(value: coven::Invite) -> Self {
        let request_to_decide = bae_core::people::request_to_decide(&value)
            .cloned()
            .map(|request| Arc::new(crate::BridgeJoinRequest::new(request)));
        Self {
            id: value.id.0.to_string(),
            member: value.member.0.to_string(),
            invitee: value.invitee,
            delivery: BridgeInviteDeliveryKind::from_core(value.delivery),
            access: BridgeMemberAccess::from_core(value.access),
            code: value.code,
            expires_at: value.expires_at,
            request_to_decide,
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
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An S3 invite names its key, so the QR screen can tell the person which
    /// key to delete if nobody uses the invite.
    #[test]
    fn an_invite_carries_its_key_id_and_person() {
        let invite = BridgeInvite::from_core(coven::Invite {
            id: coven::InviteId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
            member: coven::MemberId("00000000-0000-4000-8000-000000000002".parse().unwrap()),
            invitee: Some("Dan".into()),
            delivery: coven::InviteDeliveryKind::Sent,
            access: coven::MemberAccess::S3AccessKey {
                access_key_id: "dan-key".into(),
            },
            code: "code".into(),
            expires_at: std::time::UNIX_EPOCH,
            request: None,
        });
        assert_eq!(
            invite.access,
            BridgeMemberAccess::S3AccessKey {
                access_key_id: "dan-key".into()
            }
        );
        assert_eq!(invite.invitee.as_deref(), Some("Dan"));
        assert_eq!(invite.delivery, BridgeInviteDeliveryKind::Sent);
        assert!(invite.request_to_decide.is_none());
    }
}
