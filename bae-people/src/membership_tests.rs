use super::*;

#[test]
fn an_admin_removes_any_member_but_themself() {
    assert!(MembershipMember::removable(true, false));
    assert!(!MembershipMember::removable(true, true));
    assert!(!MembershipMember::removable(false, false));
}

#[test]
fn a_member_removes_only_their_own_other_devices() {
    // A member: their own other device, nobody else's, never this one.
    assert!(ActiveDevice::removable(false, true, false));
    assert!(!ActiveDevice::removable(false, false, false));
    assert!(!ActiveDevice::removable(false, true, true));
    // An admin: anyone's device, still never this one.
    assert!(ActiveDevice::removable(true, false, false));
    assert!(!ActiveDevice::removable(true, true, true));
}

/// How a `coven_members` row reaches storage: an account, an S3 key id, or,
/// for the library's creator until Set up storage records theirs, neither.
/// Both at once is no row coven writes.
#[test]
fn a_members_access_is_an_account_a_key_or_not_yet_recorded() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    // The compressed base point: a signing key the row's conversion accepts.
    let key = format!("x'58{}'", "66".repeat(31));
    let read = |account: Option<&str>, access_key_id: Option<&str>| {
        connection.query_row(
            &format!(
                "SELECT '00000000-0000-4000-8000-000000000001', 'admin', 1, {key}, {key}, \
                 ?1, ?2, '[]', '[]'"
            ),
            [account, access_key_id],
            |row| super::members::member_row(row, 0, true),
        )
    };
    assert_eq!(
        read(Some("ana@example.com"), None).unwrap().access,
        Some(coven::MemberAccess::ProviderAccount("ana@example.com".into()))
    );
    assert_eq!(
        read(None, Some("AKIA1")).unwrap().access,
        Some(coven::MemberAccess::S3AccessKey { access_key_id: "AKIA1".into() })
    );
    assert_eq!(read(None, None).unwrap().access, None);
    assert!(read(Some("ana@example.com"), Some("AKIA1")).is_err());
}
