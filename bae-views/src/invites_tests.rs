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

/// Once Dan's invite shows its code, coven's pass looks for his request and
/// holds a record naming the request it hasn't found: that is the invite
/// waiting for him, not a hold to show, so the invite shows nothing.
#[test]
fn a_waiting_invite_shows_no_record_for_the_request_it_waits_on() {
    let waiting = coven::Invite {
        stage: coven::InviteStage::Waiting { code: "code".into(), request: None },
        ..invite("00000000-0000-4000-8000-000000000001", 7)
    };
    let record = coven::PendingRecord {
        subject: coven::PendingSubject::Operation {
            id: waiting.operation,
            kind: coven::OperationKind::Invite,
        },
        reason: coven::PendingReason::Missing {
            path: coven::ObjectPath::join_request(
                coven::StoreId("00000000-0000-4000-8000-0000000000a1".parse().unwrap()),
                coven::DeviceId(1),
                waiting.id,
            ),
        },
        cause: None,
    };
    let open = OpenInvite::held_by(vec![waiting], std::slice::from_ref(&record));
    assert_eq!(open[0].held, None);
}

/// A waiting invite's own waits are not holds, but a request that cannot be
/// read is: coven's record of a storage failure or of no storage shows,
/// whatever stage the invite is at.
#[test]
fn a_waiting_invite_shows_the_record_of_a_failure_to_read_its_request() {
    let waiting = coven::Invite {
        stage: coven::InviteStage::Waiting { code: "code".into(), request: None },
        ..invite("00000000-0000-4000-8000-000000000001", 7)
    };
    let record = coven::PendingRecord {
        subject: coven::PendingSubject::Operation {
            id: waiting.operation,
            kind: coven::OperationKind::Invite,
        },
        reason: coven::PendingReason::NoStorage,
        cause: Some("offline".into()),
    };
    let open = OpenInvite::held_by(vec![waiting], std::slice::from_ref(&record));
    let held = open[0].held.as_ref().expect("a failure to read the request is a hold");
    assert_eq!(held.reason, coven::PendingReason::NoStorage);
}

/// A pause is the wait of an invite whose request awaits Ana's decision, not
/// of one still being granted, which shows it.
#[test]
fn a_pause_before_the_code_shows_is_a_hold() {
    let granting = invite("00000000-0000-4000-8000-000000000002", 8);
    let paused = coven::PendingRecord {
        subject: coven::PendingSubject::Operation {
            id: granting.operation,
            kind: coven::OperationKind::Invite,
        },
        reason: coven::PendingReason::Paused,
        cause: None,
    };
    let open = OpenInvite::held_by(vec![granting], &[paused]);
    assert!(open[0].held.is_some());
}
