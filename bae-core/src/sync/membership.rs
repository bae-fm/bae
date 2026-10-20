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

impl Membership {
    pub fn from_members(members: Vec<coven::MemberInfo>) -> Self {
        let self_is_admin = members
            .iter()
            .any(|member| member.is_self && member.role == coven::MemberRole::Admin);
        Self {
            members: members
                .into_iter()
                .map(|member| MembershipMember::new(member, self_is_admin))
                .collect(),
            self_is_admin,
        }
    }
}

impl MembershipMember {
    fn new(member: coven::MemberInfo, self_is_admin: bool) -> Self {
        let coven::MemberInfo {
            id,
            keys,
            role,
            devices,
            is_self,
        } = member;
        let can_remove_devices = self_is_admin || is_self;
        let mut active_devices = Vec::new();
        let mut former_devices = Vec::new();
        for device in devices {
            let state = match device.state {
                coven::DeviceState::Active => {
                    active_devices.push(ActiveDevice {
                        id: device.id,
                        name: device.name,
                        is_self: device.is_self,
                        can_remove: can_remove_devices && !device.is_self,
                    });
                    continue;
                }
                coven::DeviceState::Removed => FormerDeviceState::Removed,
                coven::DeviceState::Replaced => FormerDeviceState::Replaced,
            };
            // A former device's `is_self` is dropped: this installation's
            // own lost registration reads as removed or replaced like any other.
            former_devices.push(FormerDevice {
                id: device.id,
                name: device.name,
                state,
            });
        }
        Self {
            id,
            keys,
            role,
            is_self,
            can_remove: self_is_admin && !is_self,
            active_devices,
            former_devices,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: u64, state: coven::DeviceState, is_self: bool) -> coven::DeviceInfo {
        coven::DeviceInfo {
            id: coven::DeviceId(id),
            name: format!("Device {id}"),
            state,
            is_self,
        }
    }

    fn member(
        id: u128,
        role: coven::MemberRole,
        is_self: bool,
        devices: Vec<coven::DeviceInfo>,
    ) -> coven::MemberInfo {
        coven::MemberInfo {
            id: coven::MemberId(uuid::Uuid::from_u128(id)),
            keys: coven::MemberKeys::generate().unwrap().public_keys(),
            role,
            devices,
            is_self,
        }
    }

    #[test]
    fn removed_and_replaced_devices_are_listed_apart_from_active_ones() {
        use coven::DeviceState::{Active, Removed, Replaced};
        let membership = Membership::from_members(vec![
            member(
                1,
                coven::MemberRole::Admin,
                true,
                vec![device(1, Active, true), device(2, Replaced, false)],
            ),
            member(
                2,
                coven::MemberRole::Member,
                false,
                vec![
                    device(3, Removed, false),
                    device(4, Active, false),
                    device(5, Replaced, false),
                ],
            ),
        ]);

        let me = &membership.members[0];
        assert_eq!(
            me.active_devices,
            vec![ActiveDevice {
                id: coven::DeviceId(1),
                name: "Device 1".into(),
                is_self: true,
                can_remove: false,
            }]
        );
        assert_eq!(
            me.former_devices,
            vec![FormerDevice {
                id: coven::DeviceId(2),
                name: "Device 2".into(),
                state: FormerDeviceState::Replaced,
            }]
        );
        let other = &membership.members[1];
        assert_eq!(
            other.active_devices,
            vec![ActiveDevice {
                id: coven::DeviceId(4),
                name: "Device 4".into(),
                is_self: false,
                can_remove: true,
            }]
        );
        assert_eq!(
            other.former_devices,
            vec![
                FormerDevice {
                    id: coven::DeviceId(3),
                    name: "Device 3".into(),
                    state: FormerDeviceState::Removed,
                },
                FormerDevice {
                    id: coven::DeviceId(5),
                    name: "Device 5".into(),
                    state: FormerDeviceState::Replaced,
                },
            ]
        );
    }

    #[test]
    fn a_member_removes_only_their_own_other_devices() {
        use coven::DeviceState::Active;
        let membership = Membership::from_members(vec![
            member(
                1,
                coven::MemberRole::Member,
                true,
                vec![device(1, Active, true), device(2, Active, false)],
            ),
            member(
                2,
                coven::MemberRole::Admin,
                false,
                vec![device(3, Active, false)],
            ),
        ]);

        let removable = |member: &MembershipMember| {
            member
                .active_devices
                .iter()
                .map(|device| device.can_remove)
                .collect::<Vec<_>>()
        };
        assert!(!membership.self_is_admin);
        assert_eq!(removable(&membership.members[0]), [false, true]);
        assert_eq!(removable(&membership.members[1]), [false]);
    }
}
