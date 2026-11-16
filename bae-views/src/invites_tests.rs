use super::*;

/// Open an in-memory library and create tables with the columns of the coven
/// views `INVITE_ROWS` reads, beside the library's own `people`.
fn views() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    crate::TEST_SCHEMA.open_bare_library(&conn);
    conn.execute_batch(
        "CREATE TABLE coven_invites(invite, operation, stage, request_member, joiner_name, invite_id);
         CREATE TABLE coven_pending(record, cause, operation, subject_kind, reason_kind);
         CREATE TABLE coven_operations(operation, kind, outcome, outcome_kind, account);",
    )
    .unwrap();
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

fn invite(conn: &rusqlite::Connection, operation: i64, stage: &str) {
    conn.execute(
        "INSERT INTO coven_invites VALUES (x'00', ?1, ?2, NULL, NULL, 'invite-' || ?1)",
        rusqlite::params![operation, stage],
    )
    .unwrap();
}

fn record(conn: &rusqlite::Connection, operation: i64, reason: &str) {
    conn.execute(
        "INSERT INTO coven_pending VALUES (x'01', NULL, ?1, 'operation', ?2)",
        rusqlite::params![operation, reason],
    )
    .unwrap();
}

/// Coven holds Dan's invite while it can't grant his access: the invite
/// whose operation the record names shows why, and the invite beside it
/// shows nothing.
#[test]
fn an_invite_shows_the_record_its_operation_holds() {
    let conn = views();
    invite(&conn, 6, "granting");
    invite(&conn, 7, "granting");
    record(&conn, 7, "no_storage");
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
    invite(&conn, 7, "waiting");
    record(&conn, 7, "missing");
    invite(&conn, 8, "waiting");
    record(&conn, 8, "no_storage");
    let rows = rows(&conn);
    assert_eq!(rows[0].4, None);
    assert!(rows[1].4.is_some());
}

/// A pause is the wait of an invite whose request awaits Ana's decision, not
/// of one still being granted, which shows it.
#[test]
fn a_pause_before_the_code_shows_is_a_hold() {
    let conn = views();
    conn.execute(
        "INSERT INTO coven_invites VALUES (x'00', 8, 'waiting', x'0a', 'Dan', 'invite-8')",
        [],
    )
    .unwrap();
    record(&conn, 8, "paused");
    invite(&conn, 9, "granting");
    record(&conn, 9, "paused");
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
    invite(&conn, 1, "waiting");
    conn.execute(
        "INSERT INTO coven_invites VALUES (x'00', 2, 'waiting', x'0a', 'Dan', 'invite-2')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO coven_invites VALUES (x'00', 3, 'approving', x'0c', NULL, 'invite-3')",
        [],
    )
    .unwrap();
    let shown: Vec<(Option<bool>, Option<String>)> =
        rows(&conn).into_iter().map(|row| (row.2, row.3)).collect();
    assert_eq!(
        shown,
        [(Some(false), None), (Some(true), Some("Dan".to_string())), (Some(true), None),]
    );
}

/// Open invites and the ones a sharing refusal ended are one list.
/// `INVITE_ROWS` lists them in the order coven started their operations and
/// leaves out operations with any other outcome.
#[test]
fn open_and_refused_invites_are_listed_in_the_order_they_were_made() {
    let conn = views();
    invite(&conn, 5, "granting");
    for (operation, kind, outcome_kind) in [
        (2, "Invite", "sharing_refused"),
        (4, "Invite", "applied"),
        (6, "Invite", "sharing_refused"),
        (3, "Other", "sharing_refused"),
    ] {
        conn.execute(
            "INSERT INTO coven_operations VALUES (?1, ?2, x'02', ?3, 'dan@example.com')",
            rusqlite::params![operation, kind, outcome_kind],
        )
        .unwrap();
    }
    let listed: Vec<(i64, String)> = rows(&conn).into_iter().map(|row| (row.0, row.1)).collect();
    assert_eq!(
        listed,
        [(2, "refused".to_string()), (5, "open".to_string()), (6, "refused".to_string())]
    );
}

/// An invite whose request has arrived: Dan's, at `stage`.
fn requested(conn: &rusqlite::Connection, operation: i64, stage: &str) {
    conn.execute(
        "INSERT INTO coven_invites VALUES (x'00', ?1, ?2, x'0a', 'Dan', 'invite-' || ?1)",
        rusqlite::params![operation, stage],
    )
    .unwrap();
}

/// The operation of the invite `WAITING_REQUEST` returns, leaving out the
/// invites whose ids `closed` holds.
fn waiting_request(conn: &rusqlite::Connection, closed: &[&str]) -> Option<i64> {
    let closed = serde_json::Value::from(closed.to_vec()).to_string();
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
/// request to decide, and a request Ana closed gives its place to the next.
#[test]
fn the_waiting_request_is_the_first_one_the_person_did_not_close() {
    let conn = views();
    invite(&conn, 1, "waiting");
    requested(&conn, 2, "approving");
    requested(&conn, 3, "waiting");
    requested(&conn, 4, "waiting");
    assert_eq!(waiting_request(&conn, &[]), Some(3));
    assert_eq!(waiting_request(&conn, &["invite-3"]), Some(4));
    assert_eq!(waiting_request(&conn, &["invite-3", "invite-4"]), None);
}
