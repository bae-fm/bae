use super::*;

fn invite(id: &str, operation: i64) -> coven::Invite {
    coven::Invite {
        id: coven::InviteId(id.parse().unwrap()),
        member: coven::MemberId("00000000-0000-4000-8000-000000000009".parse().unwrap()),
        invitee: Some("Dan".into()),
        delivery: coven::InviteDeliveryKind::Sent,
        access: coven::MemberAccess::ProviderAccount("dan@example.com".into()),
        expires_at: std::time::UNIX_EPOCH,
        operation: coven::OperationId(operation),
        stage: coven::InviteStage::Granting,
    }
}

/// Coven holds Dan's invite while it can't grant his access: the invite
/// whose operation the record names shows why, and the invite beside it
/// shows nothing.
#[test]
fn an_invite_shows_the_record_its_operation_holds() {
    let record = coven::PendingRecord {
        subject: coven::PendingSubject::Operation {
            id: coven::OperationId(7),
            kind: coven::OperationKind::Invite,
        },
        reason: coven::PendingReason::NoStorage,
        cause: Some("offline".into()),
    };
    let open = OpenInvite::held_by(
        vec![
            invite("00000000-0000-4000-8000-000000000001", 6),
            invite("00000000-0000-4000-8000-000000000002", 7),
        ],
        std::slice::from_ref(&record),
    );
    assert_eq!(open[0].held, None);
    let held = open[1].held.as_ref().expect("the record names the second invite");
    assert_eq!(held.reason, coven::PendingReason::NoStorage);
    assert_eq!(held.cause.as_deref(), Some("offline"));
}
