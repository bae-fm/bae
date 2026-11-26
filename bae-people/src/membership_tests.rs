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
/// looks. A place with no change lists none and says so.
#[test]
fn every_place_returns_a_change_of_a_kind_no_place_shows() {
    use super::admin_operations::{PlaceAdminChanges, PlaceAdminChangesParams};
    use crate::admin_change::AdminChangePlace;
    use sqlive_compose::{Returning as _, Statement as _};
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::test_fixtures::TEST_SCHEMA.open_bare_library(&connection);
    connection
        .execute_batch(
            "INSERT INTO admin_operations (operation, member) VALUES \
                 (1, NULL), (2, 'ana'), (3, NULL), (4, NULL); \
             INSERT INTO _coven_app_operations (operation, kind) VALUES \
                 (1, 'RenameStore'), (2, 'SetRole'), (3, 'RemoveDevice'), (4, 'CreateStore');",
        )
        .unwrap();
    let ana: coven::MemberId = "00000000-0000-4000-8000-000000000001".parse().unwrap();
    connection
        .execute("UPDATE admin_operations SET member = ?1 WHERE operation = 2", [ana])
        .unwrap();
    // The operations the place's read lists, and whether it says the place
    // has a change.
    let read = |place: AdminChangePlace| -> (Vec<i64>, bool) {
        let request = PlaceAdminChangesParams::of(&place);
        let mut statement = connection.prepare(PlaceAdminChanges::SQL).unwrap();
        let row = statement
            .query_row(request.params().as_slice(), PlaceAdminChangesParams::read)
            .unwrap();
        let operations = row.listed.changes.iter().map(|change| change.operation).collect();
        (operations, row.place_has_change)
    };
    assert_eq!(read(AdminChangePlace::Library), (vec![1, 4], true));
    assert_eq!(read(AdminChangePlace::Person(ana)), (vec![2, 4], true));
    assert_eq!(read(AdminChangePlace::People), (vec![3, 4], true));
    connection.execute_batch("DELETE FROM admin_operations WHERE operation <> 2").unwrap();
    assert_eq!(read(AdminChangePlace::Library), (vec![], false), "none with the library's name");
    assert_eq!(read(AdminChangePlace::People), (vec![], false), "none on People and devices");
    assert_eq!(read(AdminChangePlace::Person(ana)), (vec![2], true));
}
