//! The membership / join / restore DTOs and re-exports bae's sync surface uses.
//!
//! The sync manager itself is coven's private implementation. bae drives it
//! through `CovenHandle` and keeps only the membership types needed by the UI.

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
///
/// `email` is the OAuth account address the joiner authenticated as, baked into
/// the code so the approver can share the OAuth folder to it; `None` for S3,
/// which shares no folder to an email.
pub fn generate_join_request(email: Option<String>) -> Result<JoinRequest, crate::keys::KeyError> {
    let code = coven::generate_join_request(email)?;
    let pubkey = coven::decode_join_request(&code)
        .expect("a code this device just encoded decodes")
        .public_key;
    let fingerprint = pubkey_fingerprint(&pubkey);
    Ok(JoinRequest { code, fingerprint })
}

/// Fetch the email of the OAuth account `oauth_tokens` authenticated, so the
/// joiner can bake it into its join-request. A thin pass-through to coven, which
/// dispatches per provider and rejects non-OAuth providers.
#[cfg(feature = "oauth-providers")]
pub async fn fetch_account_email(
    provider: crate::config::CloudProvider,
    oauth_tokens: &coven::OAuthTokens,
) -> Result<String, coven::OAuthError> {
    coven::fetch_account_email(provider, oauth_tokens).await
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
pub fn decode_join_request(code: &str) -> Result<JoinRequestInfo, coven::JoinCodeError> {
    let req = coven::decode_join_request(code)?;
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
pub fn decode_invite_code_info(code: &str) -> Result<InviteCodeInfo, coven::JoinCodeError> {
    let info = coven::decode_invite_code_info(code)?;
    Ok(InviteCodeInfo {
        owner_fingerprint: pubkey_fingerprint(&info.owner_pubkey),
        library_id: info.store_id,
        library_name: info.store_name,
        owner_pubkey: info.owner_pubkey,
        cloud_provider: info.cloud_provider,
        needs_oauth: info.needs_oauth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The email the joiner authenticated as is baked into the join-request code
    /// and comes back out when the approver decodes it — the wire that carries an
    /// OAuth account address across device pairing.
    #[test]
    fn join_request_round_trips_the_email() {
        crate::config::install_test_keyring();

        let with_email = generate_join_request(Some("joiner@example.com".to_string())).unwrap();
        let decoded = decode_join_request(&with_email.code).unwrap();
        assert_eq!(decoded.email.as_deref(), Some("joiner@example.com"));
        assert_eq!(decoded.fingerprint, with_email.fingerprint);

        // S3 carries no email; the code decodes with `None`, not an empty string.
        let without_email = generate_join_request(None).unwrap();
        assert_eq!(
            decode_join_request(&without_email.code).unwrap().email,
            None
        );
    }
}
