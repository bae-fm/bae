use super::*;
use crate::membership::fragments::{ActiveDeviceDocument, FormerDeviceDocument};

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
    active: Vec<ActiveDeviceDocument>,
    former: Vec<FormerDeviceDocument>,
    self_is_admin: bool,
}

const NO_DEVICES: MemberValues<'static> = MemberValues {
    account: None,
    access_key_id: None,
    active: Vec::new(),
    former: Vec::new(),
    self_is_admin: false,
};

/// The member that a row of the member fragment holds with `values`.
fn member_of(values: MemberValues<'_>) -> Result<MembershipMember, bae_store::DbError> {
    super::fragments::Member {
        member_id: "00000000-0000-4000-8000-000000000001".parse().unwrap(),
        removed: false,
        role: coven::MemberRole::Member,
        is_self: false,
        account: values.account.map(str::to_string),
        access_key_id: values.access_key_id.map(str::to_string),
        active_devices: values.active,
        former_devices: values.former,
        active_device_count: 0,
        former_device_count: 0,
        viewer: super::fragments::SelfIsAdmin { self_is_admin: values.self_is_admin },
    }
    .into_membership()
}

/// The document of the device `id` in a member row's list of former devices.
fn former(id: u8, name: &str, state: &str) -> FormerDeviceDocument {
    FormerDeviceDocument {
        id: vec![0, 0, 0, 0, 0, 0, 0, id],
        name: name.into(),
        state: state.into(),
    }
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
            active: vec![ActiveDeviceDocument {
                id: vec![0, 0, 0, 0, 0, 0, 0, 1],
                name: "Phone".into(),
                is_self: false,
            }],
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
    let member =
        read(vec![former(2, "Old Phone", "replaced"), former(3, "Tablet", "removed")]).unwrap();
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
    assert!(read(vec![former(2, "Old Phone", "lost")]).is_err());
}

/// Each place's statement returns the changes of its own kinds, and every
/// place's statement returns a change whose operation coven lists with a kind
/// no place shows, so that reading that change fails wherever the person
/// looks. A place with no change lists none and says so.
#[test]
fn every_place_returns_a_change_of_a_kind_no_place_shows() {
    use super::admin_operations::{
        DeleteAdminOperationParams, InsertAdminOperationParams, PlaceAdminChangesParams,
    };
    use crate::admin_change::AdminChangePlace;
    use bae_store::test_fixtures::bare_statements;
    use coven::StoreChange;
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::test_fixtures::TEST_SCHEMA.open_bare_library(&connection);
    let ana: coven::MemberId = "00000000-0000-4000-8000-000000000001".parse().unwrap();
    // Journal a change of each place's kinds and one of a kind no place
    // shows, each with what it changes.
    let journal = |change: StoreChange, member: Option<coven::MemberId>| -> i64 {
        let operation = coven::record_change(&connection, change).unwrap();
        let what = InsertAdminOperationParams {
            operation: operation.0,
            member,
            device: None,
            role: None,
            name: None,
        };
        bare_statements::run(&connection, &what).unwrap();
        operation.0
    };
    let rename = journal(StoreChange::RenameStore { name: "Home".into() }, None);
    let role =
        journal(StoreChange::ChangeRole { member: ana, role: coven::MemberRole::Admin }, Some(ana));
    let removal = journal(StoreChange::RemoveDevice { device: coven::DeviceId(1) }, None);
    let storage = journal(
        StoreChange::SetUpStorage {
            access: coven::MemberAccess::S3AccessKey { access_key_id: "AKIA1".into() },
        },
        None,
    );
    // The operations the place's read lists, and whether it says the place
    // has a change.
    let read = |place: AdminChangePlace| -> (Vec<i64>, bool) {
        let request = PlaceAdminChangesParams::of(&place);
        let row = bare_statements::row(&connection, &request).unwrap();
        let operations = row.listed.changes.iter().map(|change| change.operation).collect();
        (operations, row.place_has_change)
    };
    assert_eq!(read(AdminChangePlace::Library), (vec![rename, storage], true));
    assert_eq!(read(AdminChangePlace::Person(ana)), (vec![role, storage], true));
    assert_eq!(read(AdminChangePlace::People), (vec![removal, storage], true));
    for operation in [rename, removal, storage] {
        bare_statements::run(&connection, &DeleteAdminOperationParams { operation }).unwrap();
    }
    assert_eq!(read(AdminChangePlace::Library), (vec![], false), "none with the library's name");
    assert_eq!(read(AdminChangePlace::People), (vec![], false), "none on People and devices");
    assert_eq!(read(AdminChangePlace::Person(ana)), (vec![role], true));
}
