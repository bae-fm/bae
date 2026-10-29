//! People and their devices, with permissions derived from coven's membership.

/// A device that can still sync, and whether this device's person may remove
/// it. This device leaves by removing the library from it instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveDevice {
    pub id: coven::DeviceId,
    pub name: String,
    pub is_self: bool,
    pub can_remove: bool,
}

/// How a device lost access to the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormerDeviceState {
    Removed,
    Replaced,
}

/// A device that lost access. Coven keeps its record for good, so it stays
/// listed under its person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormerDevice {
    pub id: coven::DeviceId,
    pub name: String,
    pub state: FormerDeviceState,
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

/// Watches the membership for a device of this device's person that wasn't
/// there at the watch's first look. Add Device's sheet shows its code until
/// one joins.
#[derive(Default)]
pub struct AddedDeviceWatch {
    /// The person's active devices at the first look.
    known: std::sync::OnceLock<std::collections::BTreeSet<coven::DeviceId>>,
}

impl AddedDeviceWatch {
    /// The first of the person's active devices, in device-id order, that
    /// wasn't there at the first look; none until one joins. The first call
    /// is the first look.
    pub fn added(&self, membership: &Membership) -> Option<ActiveDevice> {
        let own = membership.members.iter().find(|member| member.is_self)?;
        let known = self
            .known
            .get_or_init(|| own.active_devices.iter().map(|device| device.id).collect());
        own.active_devices
            .iter()
            .find(|device| !known.contains(&device.id))
            .cloned()
    }
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
mod tests {
    use super::*;

    #[test]
    fn an_admin_removes_any_member_but_themself() {
        assert!(MembershipMember::removable(true, false));
        assert!(!MembershipMember::removable(true, true));
        assert!(!MembershipMember::removable(false, false));
    }

    #[test]
    fn a_member_removes_only_their_own_other_devices() {
        // A member: their own other device, nobody else's, never this one.
        assert!(ActiveDevice::removable(false, true, false));
        assert!(!ActiveDevice::removable(false, false, false));
        assert!(!ActiveDevice::removable(false, true, true));
        // An admin: anyone's device, still never this one.
        assert!(ActiveDevice::removable(true, false, false));
        assert!(!ActiveDevice::removable(true, true, true));
    }

    fn device(id: u64) -> ActiveDevice {
        ActiveDevice {
            id: coven::DeviceId(id),
            name: format!("Device {id}"),
            is_self: id == 1,
            can_remove: id != 1,
        }
    }

    fn member(id: u128, is_self: bool, devices: &[u64]) -> MembershipMember {
        MembershipMember {
            id: coven::MemberId(uuid::Uuid::from_u128(id)),
            keys: coven::MemberKeys::generate().unwrap().public_keys(),
            role: coven::MemberRole::Member,
            access: coven::MemberAccess::S3AccessKey {
                access_key_id: format!("key-{id}"),
            },
            is_self,
            can_remove: false,
            active_devices: devices.iter().copied().map(device).collect(),
            former_devices: Vec::new(),
        }
    }

    /// Add Device's sheet watches for the person's device that joins while
    /// it shows the code: what the person had at the first look is no news,
    /// nor is another person's new device.
    #[test]
    fn a_watch_finds_the_persons_device_that_joins_after_its_first_look() {
        let membership = |own: &[u64], others: &[u64]| Membership {
            members: vec![member(1, true, own), member(2, false, others)],
            self_is_admin: false,
        };
        let watch = AddedDeviceWatch::default();

        assert_eq!(watch.added(&membership(&[1, 2], &[3])), None);
        assert_eq!(watch.added(&membership(&[1, 2], &[3, 4])), None);
        assert_eq!(
            watch.added(&membership(&[1, 2, 5], &[3, 4])),
            Some(device(5))
        );
    }
}
