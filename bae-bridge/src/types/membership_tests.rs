use super::*;

/// An S3 invite names its key, so the QR screen can tell the person which
/// key to delete if nobody uses the invite.
#[test]
fn an_invite_carries_its_key_id_and_person() {
    let invite = BridgeInvite::from_core(bae_views::OpenInvite {
        invite: coven::Invite {
            id: coven::InviteId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
            member: coven::MemberId("00000000-0000-4000-8000-000000000002".parse().unwrap()),
            invitee: Some("Dan".into()),
            access: coven::MemberAccess::S3AccessKey { access_key_id: "dan-key".into() },
            expires_at: std::time::UNIX_EPOCH,
            operation: coven::OperationId(1),
            stage: coven::InviteStage::Waiting { code: "code".into(), request: None },
        },
        held: None,
    });
    assert_eq!(invite.access, BridgeMemberAccess::S3AccessKey { access_key_id: "dan-key".into() });
    assert_eq!(invite.invitee.as_deref(), Some("Dan"));
    assert!(matches!(
        &invite.shown,
        BridgeInviteShown::Code { code, progress: BridgeInviteProgress::Waiting } if code == "code"
    ));
    assert!(invite.held.is_none());
}
