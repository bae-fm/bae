//! People and their devices, with permissions derived from coven's
//! membership, as db reads them from coven's views.

/// A device that can still sync, and whether this device's person may remove
/// it. This device leaves by removing the library from it instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveDevice {
    pub id: coven::DeviceId,
    pub name: String,
    pub is_self: bool,
    pub can_remove: bool,
}

/// A device that lost access. Coven keeps its record for good, so it stays
/// listed under its person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormerDevice {
    pub id: coven::DeviceId,
    pub name: String,
    pub state: bae_store::FormerDeviceState,
}

/// A member with their devices split into those that can sync and those that
/// lost access, each list in device-id order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipMember {
    pub id: coven::MemberId,
    pub keys: coven::MemberPublicKeys,
    pub role: coven::MemberRole,
    /// How they reach storage: on S3 the id of their key, which the person
    /// deletes in the provider's console when they remove the member.
    pub access: coven::MemberAccess,
    pub is_self: bool,
    pub can_remove: bool,
    pub active_devices: Vec<ActiveDevice>,
    pub former_devices: Vec<FormerDevice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Membership {
    pub members: Vec<MembershipMember>,
    pub self_is_admin: bool,
}

impl MembershipMember {
    /// Whether this device's person may remove a member: an admin may remove
    /// anyone but themself.
    pub(crate) fn removable(self_is_admin: bool, is_self: bool) -> bool {
        self_is_admin && !is_self
    }
}

impl ActiveDevice {
    /// Whether this device's person may remove a device: an admin any
    /// member's, a member their own, and never this device, which leaves by
    /// removing the library from it instead.
    pub(crate) fn removable(
        self_is_admin: bool,
        member_is_self: bool,
        device_is_self: bool,
    ) -> bool {
        (self_is_admin || member_is_self) && !device_is_self
    }
}

#[cfg(test)]
#[path = "membership_tests.rs"]
mod tests;
