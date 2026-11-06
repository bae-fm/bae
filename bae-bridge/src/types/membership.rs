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
mirror_enum! { BridgeFormerDeviceState = bae_store::FormerDeviceState,
    from_core: pub(crate) fn, variants: { Removed, Replaced },
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
    pub(crate) fn from_core(device: bae_people::membership::ActiveDevice) -> Self {
        let bae_people::membership::ActiveDevice { id, name, is_self, can_remove } = device;
        Self { id: id.0, name, is_self, can_remove }
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
    fn from_core(device: bae_people::membership::FormerDevice) -> Self {
        let bae_people::membership::FormerDevice { id, name, state } = device;
        Self { id: id.0, name, state: BridgeFormerDeviceState::from_core(state) }
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
    fn from_core(value: bae_people::membership::MembershipMember) -> Self {
        let bae_people::membership::MembershipMember {
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
            active_devices: active_devices.into_iter().map(BridgeActiveDevice::from_core).collect(),
            former_devices: former_devices.into_iter().map(BridgeFormerDevice::from_core).collect(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMembership {
    pub members: Vec<BridgeMember>,
    pub self_is_admin: bool,
}
impl BridgeMembership {
    pub(crate) fn from_core(value: bae_people::membership::Members) -> Self {
        let bae_people::membership::Members { members, self_is_admin } = value;
        Self { members: members.into_iter().map(BridgeMember::from_core).collect(), self_is_admin }
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
    pub(crate) fn from_core(value: bae_people::people::Person) -> Self {
        let bae_people::people::Person { profile, membership } = value;
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
            coven::ProviderSignOut::RemoveAppAccess { provider } => {
                Self::RemoveAppAccess { provider: BridgeCloudProvider::from_core(&provider) }
            }
            coven::ProviderSignOut::RemoveFromAppleAccount => Self::RemoveFromAppleAccount,
        }
    }
}
#[derive(Clone, uniffi::Enum)]
pub enum BridgeInviteAccess {
    ProviderAccount { email: String },
    S3AccessKey { access_key_id: String, secret_access_key: String },
}
impl BridgeInviteAccess {
    pub(crate) fn into_core(self) -> coven::InviteAccess {
        match self {
            Self::ProviderAccount { email } => coven::InviteAccess::ProviderAccount { email },
            Self::S3AccessKey { access_key_id, secret_access_key } => {
                coven::InviteAccess::S3AccessKey {
                    access_key_id,
                    secret_access_key: coven::SecretText::new(secret_access_key),
                }
            }
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
            Self::Sent { lifetime } => {
                coven::InviteDelivery::Sent { lifetime: lifetime.into_core() }
            }
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
    pub expires_at: std::time::SystemTime,
    pub shown: BridgeInviteShown,
    /// Why coven holds the invite's work, when it does: a refused check or a
    /// failed grant, which cancelling the invite ends.
    pub held: Option<crate::BridgePendingWait>,
}
/// Where an open invite stands.
#[derive(Clone, uniffi::Enum)]
pub enum BridgeInviteShown {
    /// Sync is granting the invite's access; its code comes once it has.
    Granting,
    /// Its code, to show again, and how far its person has come.
    Code { code: String, progress: BridgeInviteProgress },
}
/// How far an invite whose code can be shown has come.
#[derive(Clone, uniffi::Enum)]
pub enum BridgeInviteProgress {
    /// Waiting for its person's request.
    Waiting,
    /// A sent invite's checked request, waiting for the admin to approve or
    /// decline; an in-person invite's needs nobody.
    Deciding { request: Arc<crate::BridgeJoinRequest> },
    /// Its person is being let in; it can no longer be cancelled.
    Joining,
}
impl BridgeInvite {
    pub(crate) fn from_core(value: bae_views::OpenInvite) -> Self {
        use bae_library::people::{InviteProgress, InviteShown};
        let bae_views::OpenInvite { invite, held } = value;
        let shown = match bae_library::people::invite_shown(&invite) {
            InviteShown::Granting => BridgeInviteShown::Granting,
            InviteShown::Code { code, progress } => BridgeInviteShown::Code {
                code,
                progress: match progress {
                    InviteProgress::Waiting => BridgeInviteProgress::Waiting,
                    InviteProgress::Deciding(request) => BridgeInviteProgress::Deciding {
                        request: Arc::new(crate::BridgeJoinRequest::new(request)),
                    },
                    InviteProgress::Joining => BridgeInviteProgress::Joining,
                },
            },
        };
        Self {
            id: invite.id.0.to_string(),
            member: invite.member.0.to_string(),
            invitee: invite.invitee,
            delivery: BridgeInviteDeliveryKind::from_core(invite.delivery),
            access: BridgeMemberAccess::from_core(invite.access),
            expires_at: invite.expires_at,
            shown,
            held: held.map(crate::BridgePendingWait::from_core),
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
#[path = "membership_tests.rs"]
mod tests;
