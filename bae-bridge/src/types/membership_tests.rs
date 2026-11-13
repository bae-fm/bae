use super::*;

/// An S3 invite names its key, so the QR screen can tell the person which
/// key to delete if nobody uses the invite.
#[test]
fn an_invite_carries_its_key_id_as_its_label() {
    let invite = BridgeInvite::from_core(bae_views::OpenInvite {
        invite: coven::Invite {
            id: coven::InviteId("00000000-0000-4000-8000-000000000001".parse().unwrap()),
            member: coven::MemberId("00000000-0000-4000-8000-000000000002".parse().unwrap()),
            admits: coven::InviteAdmits::NewPerson,
            access: coven::MemberAccess::S3AccessKey { access_key_id: "dan-key".into() },
            expires_at: std::time::UNIX_EPOCH,
            operation: coven::OperationId(1),
            stage: coven::InviteStage::Waiting { code: "code".into(), request: None },
        },
        held: None,
        requester: None,
    });
    assert_eq!(invite.access, BridgeMemberAccess::S3AccessKey { access_key_id: "dan-key".into() });
    assert_eq!(invite.label, "dan-key");
    assert!(matches!(
        &invite.shown,
        BridgeInviteShown::Code { code, progress: BridgeInviteProgress::Waiting } if code == "code"
    ));
    assert!(invite.held.is_none());
}

/// An invite a sharing refusal ended crosses with its operation, the address
/// and the refusal, and each refusal has its own core string.
#[test]
fn a_refused_invite_crosses_with_its_address_and_a_string_for_its_refusal() {
    let entry =
        BridgeInviteEntry::from_core(bae_views::InviteEntry::Refused(bae_views::RefusedInvite {
            operation: coven::OperationId(7),
            address: "dan@example.com".into(),
            refusal: coven::SharingRefusal::EmailUnverified,
        }));
    let BridgeInviteEntry::Refused { refused } = entry else {
        panic!("a refused invite crosses as one");
    };
    assert_eq!(
        refused,
        BridgeRefusedInvite {
            operation: 7,
            address: "dan@example.com".into(),
            refusal: BridgeSharingRefusal::EmailUnverified,
            reason_key: BridgeSharingRefusal::EmailUnverified.loc_key().to_string(),
        }
    );
    let keys: std::collections::BTreeSet<_> = [
        BridgeSharingRefusal::InvalidAddress,
        BridgeSharingRefusal::EmailUnverified,
        BridgeSharingRefusal::TeamSharingOff,
        BridgeSharingRefusal::PlanWithoutSharing,
        BridgeSharingRefusal::TooManyMembers,
    ]
    .map(|refusal| refusal.loc_key())
    .into();
    assert_eq!(keys.len(), 5);
}

/// The invite's access crosses as coven's: an account, a key typed for the
/// invited person, or the key this device holds, whose secret the app never
/// handles.
#[test]
fn an_invite_access_crosses_as_covens() {
    assert!(matches!(
        BridgeInviteAccess::HeldS3AccessKey.into_core(),
        coven::InviteAccess::HeldS3AccessKey
    ));
    assert!(matches!(
        BridgeInviteAccess::ProviderAccount { email: "dan@example.com".into() }.into_core(),
        coven::InviteAccess::ProviderAccount { email } if email == "dan@example.com"
    ));
}

/// The invite's lifetime and role cross as coven's.
#[test]
fn an_invite_lifetime_crosses_as_covens() {
    assert_eq!(BridgeInviteLifetime::SevenDays.into_core(), coven::InviteLifetime::SevenDays);
    assert_eq!(BridgeInviteLifetime::ThirtyDays.into_core(), coven::InviteLifetime::ThirtyDays);
    assert_eq!(BridgeMemberRole::Admin.into_core(), coven::MemberRole::Admin);
}
