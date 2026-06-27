//! The membership / join / restore DTOs and re-exports bae's sync surface uses.
//!
//! The sync manager itself is coven's — bae uses it directly (re-exported here so
//! `crate::sync::sync_manager::SyncManager` resolves). It is built lazily by
//! [`CovenHandle::connect_sync`](coven::CovenHandle::connect_sync) when a provider
//! is connected, wired with bae's config provider (which reads the live
//! `ConfigHandle`, so connect/disconnect are picked up without rebuilding) and the
//! blob-transition observer the handle holds.

// coven owns the sync manager; bae uses it directly.
pub use coven::sync::membership::MemberRole;
pub use coven::sync::sync_manager::{ConfigProvider, MemberInfo, SyncManager};

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
    /// Enrich coven's raw member list into bae's membership view, computing
    /// `self_is_owner` once and each member's fingerprint and `can_remove`.
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

/// This device's join-request code plus the fingerprint it encodes, so the
/// joining device can show its own identity without decoding the code it just
/// generated.
pub struct JoinRequest {
    pub code: String,
    pub fingerprint: String,
}

/// Generate this device's join-request code and the fingerprint of the public
/// key it carries. Creates this device's keypair if one doesn't exist yet.
pub fn generate_join_request() -> Result<JoinRequest, crate::keys::KeyError> {
    let code = coven::join_code::generate_join_request(None)?;
    let pubkey = coven::join_code::decode_join_request(&code)
        .expect("a code this device just encoded decodes")
        .public_key;
    let fingerprint = pubkey_fingerprint(&pubkey);
    Ok(JoinRequest { code, fingerprint })
}

/// A decoded join-request code: the joining device's public key, its
/// fingerprint, and an optional contact email — shown to an existing member for
/// approval before inviting the device.
pub struct JoinRequestInfo {
    pub pubkey: String,
    pub fingerprint: String,
    pub email: Option<String>,
}

/// Decode a join-request code to preview the joining device before approving it.
pub fn decode_join_request(code: &str) -> Result<JoinRequestInfo, coven::join_code::JoinCodeError> {
    let req = coven::join_code::decode_join_request(code)?;
    Ok(JoinRequestInfo {
        fingerprint: pubkey_fingerprint(&req.public_key),
        pubkey: req.public_key,
        email: req.email,
    })
}

/// UI-ready info from a decoded invite code, with the owner's fingerprint
/// precomputed for the join preview.
pub struct InviteCodeInfo {
    pub library_id: String,
    pub library_name: String,
    pub owner_pubkey: String,
    pub owner_fingerprint: String,
    pub cloud_provider: crate::config::CloudProvider,
    pub needs_oauth: bool,
}

/// Decode an invite code and return UI-ready info for the join preview.
pub fn decode_invite_code_info(
    code: &str,
) -> Result<InviteCodeInfo, coven::join_code::JoinCodeError> {
    let info = coven::join_code::decode_invite_code_info(code)?;
    Ok(InviteCodeInfo {
        owner_fingerprint: pubkey_fingerprint(&info.owner_pubkey),
        library_id: info.library_id,
        library_name: info.library_name,
        owner_pubkey: info.owner_pubkey,
        cloud_provider: info.cloud_provider,
        needs_oauth: info.needs_oauth,
    })
}

/// S3 configuration data for save_s3_config.
pub struct S3ConfigData {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub key_prefix: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    /// Opaque (encrypted, obfuscated) or browsable (plaintext, readable) home.
    pub storage: crate::config::HomeStorage,
}
