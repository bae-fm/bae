//! The membership / join / restore types bae's sync surface uses.
//!
//! The sync manager itself is coven's; bae drives it through `CovenHandle` and
//! keeps only what the membership, join, and invite screens render.

pub use coven::{MemberInfo, MemberRole};

/// A device's short identity for display: the first 8 characters of its
/// hex-encoded Ed25519 public key. The single source for the value every
/// membership screen shows, so the UI never truncates a pubkey itself.
pub fn pubkey_fingerprint(pubkey: &str) -> String {
    pubkey.chars().take(8).collect()
}

/// One device in the library's membership chain, with everything a membership
/// screen renders or gates on precomputed: its short [`fingerprint`] and whether
/// the running device may remove it.
///
/// [`fingerprint`]: MembershipMember::fingerprint
pub struct MembershipMember {
    /// Hex-encoded Ed25519 public key — the device's stable identity.
    pub pubkey: String,
    pub role: MemberRole,
    /// True for the device this app is running on.
    pub is_self: bool,
    /// Short display identity — see [`pubkey_fingerprint`].
    pub fingerprint: String,
    /// Whether the running device may remove this one: only an owner may remove,
    /// and never itself.
    pub can_remove: bool,
}

/// The library's membership: its devices and whether the running device is an
/// owner (the gate for inviting and removing).
pub struct Membership {
    pub members: Vec<MembershipMember>,
    pub self_is_owner: bool,
}

impl Membership {
    /// Build bae's membership view from coven's raw member list, resolving
    /// `self_is_owner` once and every member's fingerprint and `can_remove` from
    /// it.
    pub fn from_members(members: Vec<MemberInfo>) -> Self {
        let self_is_owner = members
            .iter()
            .any(|m| m.is_self && m.role == MemberRole::Owner);
        let members = members
            .into_iter()
            .map(|m| MembershipMember {
                fingerprint: pubkey_fingerprint(&m.pubkey),
                can_remove: self_is_owner && !m.is_self,
                pubkey: m.pubkey,
                role: m.role,
                is_self: m.is_self,
            })
            .collect();
        Self {
            members,
            self_is_owner,
        }
    }
}
