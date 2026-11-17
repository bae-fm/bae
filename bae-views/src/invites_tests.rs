use super::*;
use coven::{RecordedInviteStage as Stage, RecordedJoiner as Joiner};

/// Open an in-memory library, where coven makes the views `INVITE_ROWS`
/// reads beside the library's own `people`.
fn views() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::TEST_SCHEMA.open_bare_library(&conn);
    conn
}

/// What a row of the statement says, without decoding the invite's or the
/// record's bytes: its operation, kind, whether a request arrived, the name
/// the request shows, and the record held for it.
type Shown = (i64, String, Option<bool>, Option<String>, Option<Vec<u8>>);

fn rows(conn: &rusqlite::Connection) -> Vec<Shown> {
    conn.prepare(INVITE_ROWS)
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get("operation")?,
                row.get("kind")?,
                row.get("has_request")?,
                row.get("requester")?,
                row.get("record")?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The id of the invite a test builds from `n`.
fn invite_id(n: u8) -> coven::InviteId {
    coven::InviteId(uuid::Uuid::from_bytes([n; 16]))
}

/// Journal the invite built from `n` at `stage`, as coven journals an invite
/// and its later steps, and return its operation.
fn invite(
    conn: &rusqlite::Connection,
    n: u8,
    stage: coven::RecordedInviteStage,
) -> coven::OperationId {
    let invite = coven::RecordedInvite {
        id: invite_id(n),
        member: coven::MemberId(uuid::Uuid::from_bytes([n + 100; 16])),
        stage,
    };
    coven::record_invite(conn, invite).unwrap()
}

/// Save the pending record coven saves when it holds the invite `operation`
/// for `reason`.
fn record(
    conn: &rusqlite::Connection,
    operation: coven::OperationId,
    reason: coven::PendingReason,
) {
    let subject =
        coven::PendingSubject::Operation { id: operation, kind: coven::OperationKind::Invite };
    coven::record_pending(conn, &coven::PendingRecord::new(subject, reason)).unwrap();
}

/// The reason coven holds a waiting invite while the join request at its
/// path hasn't arrived.
fn request_missing(n: u8) -> coven::PendingReason {
    coven::PendingReason::Missing {
        path: coven::ObjectPath::join_request(
            coven::StoreId(uuid::Uuid::from_bytes([200; 16])),
            coven::DeviceId(1),
            invite_id(n),
        ),
    }
}

/// The stage of an invite whose request from Dan, a new person, waits for
/// Ana's decision.
fn dan_waiting() -> Stage {
    Stage::Waiting { joiner: Some(Joiner::NewPerson("Dan".into())) }
}

/// Journal a rename of the store, a change that is not an invite.
fn rename(conn: &rusqlite::Connection) -> coven::OperationId {
    coven::record_change(conn, coven::StoreChange::RenameStore { name: "Household".into() })
        .unwrap()
}

/// The outcome of an operation the provider refused to share for.
fn refused_outcome() -> coven::OperationOutcome {
    coven::OperationOutcome::SharingRefused {
        account: "dan@example.com".into(),
        refusal: coven::SharingRefusal::TooManyMembers,
    }
}

/// End the invite `operation` as the provider's refusal to share with Dan
/// ends it.
fn refuse(conn: &rusqlite::Connection, operation: coven::OperationId) {
    coven::refuse_invite(conn, operation, "dan@example.com", coven::SharingRefusal::TooManyMembers)
        .unwrap();
}

/// Coven holds Dan's invite while it can't grant his access: the invite
/// whose operation the record names shows why, and the invite beside it
/// shows nothing.
#[test]
fn an_invite_shows_the_record_its_operation_holds() {
    let conn = views();
    invite(&conn, 6, Stage::Granting);
    let held = invite(&conn, 7, Stage::Granting);
    record(&conn, held, coven::PendingReason::NoStorage);
    let rows = rows(&conn);
    assert_eq!(rows[0].4, None);
    assert!(rows[1].4.is_some());
}

/// Once Dan's invite shows its code, coven's pass looks for his request and
/// holds a record naming the request it hasn't found: that is the invite
/// waiting for him, not a hold to show. A record of a failure to read the
/// request is a hold the invite shows, whatever stage the invite is at.
#[test]
fn a_waiting_invite_shows_no_record_for_the_request_it_waits_on() {
    let conn = views();
    let waiting = invite(&conn, 7, Stage::Waiting { joiner: None });
    record(&conn, waiting, request_missing(7));
    let held = invite(&conn, 8, Stage::Waiting { joiner: None });
    record(&conn, held, coven::PendingReason::NoStorage);
    let rows = rows(&conn);
    assert_eq!(rows[0].4, None);
    assert!(rows[1].4.is_some());
}

/// A pause is the wait of an invite whose request awaits Ana's decision, not
/// of one still being granted, which shows it.
#[test]
fn a_pause_before_the_code_shows_is_a_hold() {
    let conn = views();
    let deciding = invite(&conn, 8, dan_waiting());
    record(&conn, deciding, coven::PendingReason::Paused);
    let granting = invite(&conn, 9, Stage::Granting);
    record(&conn, granting, coven::PendingReason::Paused);
    let rows = rows(&conn);
    assert_eq!(rows[0].4, None);
    assert!(rows[1].4.is_some());
}

/// A request shows the name a new person typed; an invite with no request
/// shows none, and a recovering member's request, which carries no name,
/// shows none unless the member's profile gives one. `invite_entry` returns
/// an error for a request that arrived with no name.
#[test]
fn a_request_shows_the_name_a_new_person_typed() {
    let conn = views();
    invite(&conn, 1, Stage::Waiting { joiner: None });
    invite(&conn, 2, dan_waiting());
    let binding = rename(&conn);
    invite(&conn, 3, Stage::Approving { joiner: Joiner::Recovering, binding });
    let shown: Vec<(Option<bool>, Option<String>)> =
        rows(&conn).into_iter().map(|row| (row.2, row.3)).collect();
    assert_eq!(
        shown,
        [(Some(false), None), (Some(true), Some("Dan".to_string())), (Some(true), None),]
    );
}

/// Open invites and the ones a sharing refusal ended are one list.
/// `INVITE_ROWS` lists them in the order coven started their operations and
/// leaves out operations of any other kind or with any other outcome.
#[test]
fn open_and_refused_invites_are_listed_in_the_order_they_were_made() {
    let conn = views();
    let first = invite(&conn, 2, Stage::Granting);
    refuse(&conn, first);
    let other_kind = rename(&conn);
    coven::finish_operation(&conn, other_kind, &refused_outcome()).unwrap();
    let other_outcome = rename(&conn);
    coven::finish_operation(&conn, other_outcome, &coven::OperationOutcome::Satisfied).unwrap();
    let open = invite(&conn, 5, Stage::Granting);
    let last = invite(&conn, 6, Stage::Granting);
    refuse(&conn, last);
    let listed: Vec<(i64, String)> = rows(&conn).into_iter().map(|row| (row.0, row.1)).collect();
    assert_eq!(
        listed,
        [
            (first.0, "refused".to_string()),
            (open.0, "open".to_string()),
            (last.0, "refused".to_string())
        ]
    );
}

/// The operation of the invite `WAITING_REQUEST` returns, leaving out the
/// invites built from the numbers in `closed`.
fn waiting_request(conn: &rusqlite::Connection, closed: &[u8]) -> Option<i64> {
    let closed: Vec<String> = closed.iter().map(|&n| invite_id(n).0.to_string()).collect();
    let closed = serde_json::Value::from(closed).to_string();
    conn.prepare(WAITING_REQUEST)
        .unwrap()
        .query_map([closed], |row| row.get("operation"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .pop()
}

/// The join request dialog shows the first invite whose request waits for
/// Ana's decision. An invite with no request, or one being approved, is no
/// request to decide, and the query skips a request Ana closed and returns
/// the next waiting request.
#[test]
fn the_waiting_request_is_the_first_one_the_person_did_not_close() {
    let conn = views();
    invite(&conn, 1, Stage::Waiting { joiner: None });
    let binding = rename(&conn);
    invite(&conn, 2, Stage::Approving { joiner: Joiner::NewPerson("Dan".into()), binding });
    let third = invite(&conn, 3, dan_waiting());
    let fourth = invite(&conn, 4, dan_waiting());
    assert_eq!(waiting_request(&conn, &[]), Some(third.0));
    assert_eq!(waiting_request(&conn, &[3]), Some(fourth.0));
    assert_eq!(waiting_request(&conn, &[3, 4]), None);
}

/// The invite sheet reads its invite by id, and reads none once the invite
/// has ended and left coven's open invites.
#[test]
fn an_invite_is_read_by_its_id_while_it_is_open() {
    let conn = views();
    invite(&conn, 1, Stage::Waiting { joiner: None });
    let dan = invite(&conn, 2, dan_waiting());
    let by_id = |n: u8| -> Option<i64> {
        conn.prepare(INVITE_BY_ID)
            .unwrap()
            .query_map([invite_id(n).0.to_string()], |row| row.get("operation"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .pop()
    };
    assert_eq!(by_id(2), Some(dan.0));
    refuse(&conn, dan);
    assert_eq!(by_id(2), None);
}
