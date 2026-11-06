use super::*;

fn invite(id: &str, operation: i64) -> coven::Invite {
    coven::Invite {
        id: coven::InviteId(id.parse().unwrap()),
        member: coven::MemberId("00000000-0000-4000-8000-000000000009".parse().unwrap()),
        invitee: Some("Dan".into()),
        access: coven::MemberAccess::ProviderAccount("dan@example.com".into()),
        expires_at: std::time::UNIX_EPOCH,
        operation: coven::OperationId(operation),
        stage: coven::InviteStage::Granting,
    }
}

/// Coven granting Dan's access is all an invite shows until its code can
/// be shown.
#[test]
fn a_granting_invite_shows_granting() {
    assert_eq!(
        invite_shown(&invite("00000000-0000-4000-8000-000000000002", 7)),
        InviteShown::Granting
    );
}
