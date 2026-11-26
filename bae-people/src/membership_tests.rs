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

/// The values of a member row's columns that the tests vary.
struct MemberValues<'a> {
    account: Option<&'a str>,
    access_key_id: Option<&'a str>,
    active: &'a str,
    former: &'a str,
    self_is_admin: bool,
}

const NO_DEVICES: MemberValues<'static> = MemberValues {
    account: None,
    access_key_id: None,
    active: "[]",
    former: "[]",
    self_is_admin: false,
};

/// The member that a row of the member fragment's columns, in their order,
/// holds with `values`.
fn member_of(values: MemberValues<'_>) -> Result<MembershipMember, bae_store::DbError> {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .query_row(
            "SELECT '00000000-0000-4000-8000-000000000001', 0, 'member', 0, \
             ?1, ?2, ?3, ?4, 0, 0, ?5",
            rusqlite::params![
                values.account,
                values.access_key_id,
                values.active,
                values.former,
                values.self_is_admin
            ],
            |row| super::fragments::Member::read(row, 0),
        )
        .unwrap()
        .into_membership()
}

/// How a `coven_members` row reaches storage: an account, an S3 key id, or,
/// for the library's creator until Set up storage records theirs, neither.
/// Both at once is no row coven writes.
#[test]
fn a_members_access_is_an_account_a_key_or_not_yet_recorded() {
    let read =
        |account, access_key_id| member_of(MemberValues { account, access_key_id, ..NO_DEVICES });
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

/// A member row carries whether this device's person is an admin, and the
/// member's and their devices' removal flags follow from it.
#[test]
fn a_members_removal_flags_follow_the_admin_column_of_their_row() {
    let read = |self_is_admin| {
        member_of(MemberValues {
            active: r#"[{"id":"0000000000000001","name":"Phone","is_self":false}]"#,
            self_is_admin,
            ..NO_DEVICES
        })
        .unwrap()
    };
    let by_admin = read(true);
    assert!(by_admin.can_remove);
    assert!(by_admin.active_devices[0].can_remove);
    assert_eq!(by_admin.active_devices[0].id, coven::DeviceId(1));
    let by_member = read(false);
    assert!(!by_member.can_remove);
    assert!(!by_member.active_devices[0].can_remove);
}

/// A member row reads each former device's state by coven's name of it, and
/// fails on text that names no state of coven's.
#[test]
fn a_former_devices_state_reads_by_coven_s_name() {
    let read = |former| member_of(MemberValues { former, ..NO_DEVICES });
    let member = read(
        r#"[{"id":"0000000000000002","name":"Old Phone","state":"replaced"},
            {"id":"0000000000000003","name":"Tablet","state":"removed"}]"#,
    )
    .unwrap();
    assert_eq!(
        member.former_devices,
        [
            FormerDevice {
                id: coven::DeviceId(2),
                name: "Old Phone".into(),
                state: bae_store::FormerDeviceState::Replaced
            },
            FormerDevice {
                id: coven::DeviceId(3),
                name: "Tablet".into(),
                state: bae_store::FormerDeviceState::Removed
            },
        ]
    );
    assert!(read(r#"[{"id":"0000000000000002","name":"Old Phone","state":"lost"}]"#).is_err());
}

/// Each place's statement returns the changes of its own kinds, and every
/// place's statement returns a change whose operation coven lists with a kind
/// no place shows, so that reading that change fails wherever the person
/// looks. Whether a place has a change comes from the same rows.
#[test]
fn every_place_returns_a_change_of_a_kind_no_place_shows() {
    use crate::admin_change::AdminChangePlace;
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE admin_operations (operation INTEGER, member TEXT, device TEXT, \
                 role TEXT, name TEXT); \
             CREATE TABLE coven_operations (operation INTEGER, kind TEXT, outcome TEXT); \
             INSERT INTO admin_operations (operation, member) VALUES \
                 (1, NULL), (2, 'ana'), (3, NULL), (4, NULL); \
             INSERT INTO coven_operations (operation, kind) VALUES \
                 (1, 'RenameStore'), (2, 'SetRole'), (3, 'RemoveDevice'), (4, 'CreateStore');",
        )
        .unwrap();
    let ana: coven::MemberId = "00000000-0000-4000-8000-000000000001".parse().unwrap();
    connection
        .execute("UPDATE admin_operations SET member = ?1 WHERE operation = 2", [ana])
        .unwrap();
    let read = |place: AdminChangePlace| -> Vec<i64> {
        let member = match &place {
            AdminChangePlace::Person(member) => Some(*member),
            AdminChangePlace::Library | AdminChangePlace::People => None,
        };
        let sql = format!(
            "SELECT a.operation {} WHERE {} ORDER BY a.operation",
            super::admin_operations::ADMIN_CHANGES_FROM,
            super::admin_operations::shown_in(&place)
        );
        let mut statement = connection.prepare(&sql).unwrap();
        statement
            .query_map(rusqlite::params_from_iter(member.iter()), |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(read(AdminChangePlace::Library), [1, 4]);
    assert_eq!(read(AdminChangePlace::Person(ana)), [2, 4]);
    assert_eq!(read(AdminChangePlace::People), [3, 4]);
    let any = |place: AdminChangePlace, member: Option<coven::MemberId>| -> bool {
        let sql = super::admin_operations::any_admin_change_sql(&place);
        connection
            .query_row(&sql, rusqlite::params_from_iter(member.iter()), |row| row.get(0))
            .unwrap()
    };
    assert!(any(AdminChangePlace::Library, None));
    connection.execute_batch("DELETE FROM admin_operations WHERE operation <> 2").unwrap();
    assert!(!any(AdminChangePlace::Library, None), "no change shows with the library's name");
    assert!(!any(AdminChangePlace::People, None), "no change shows on People and devices");
    assert!(any(AdminChangePlace::Person(ana), Some(ana)));
}
