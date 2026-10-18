//! People and their devices, with permissions derived from coven's membership.

pub use coven::{MemberInfo, MemberRole};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipMember {
    pub member: MemberInfo,
    pub can_remove: bool,
    pub can_remove_devices: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Membership {
    pub members: Vec<MembershipMember>,
    pub self_is_admin: bool,
}

impl Membership {
    pub fn from_members(members: Vec<MemberInfo>) -> Self {
        let self_is_admin = members
            .iter()
            .any(|member| member.is_self && member.role == MemberRole::Admin);
        Self {
            members: members
                .into_iter()
                .map(|member| MembershipMember {
                    can_remove: self_is_admin && !member.is_self,
                    can_remove_devices: self_is_admin || member.is_self,
                    member,
                })
                .collect(),
            self_is_admin,
        }
    }
}
