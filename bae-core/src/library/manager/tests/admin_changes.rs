//! The admin changes a device asks for, as the library lists them from
//! their journaled operations until the person dismisses each outcome
//! (§5.6).

use crate::sync::admin_change::{AdminChange, AdminChangeKind, AdminChangeState};
use crate::sync::test_devices::run_two_device_test;

/// Robin renames the library on her laptop while it is offline. The rename
/// is listed as waiting, and the name stays; once the laptop syncs it is
/// done and the library has the name. Dismissing it before then is refused;
/// after, it leaves the list and coven's list of operations with it.
#[test]
fn a_rename_is_listed_waiting_then_done_until_dismissed() {
    run_two_device_test(async |devices| {
        let laptop = devices.a();
        laptop.stop_sync().await;
        let manager = laptop.manager();
        let mut changes = manager.subscribe_admin_changes();
        crate::followed::value_matching(&mut changes, Vec::is_empty).await;

        manager
            .rename_library(&crate::library_name::LibraryName::parse("Home").unwrap())
            .await
            .unwrap();
        let waiting =
            crate::followed::value_matching(&mut changes, |changes| !changes.is_empty()).await;
        let [AdminChange {
            operation,
            change: AdminChangeKind::RenameLibrary { name },
            state: AdminChangeState::Waiting,
        }] = waiting.as_slice()
        else {
            panic!("one waiting rename: {waiting:?}");
        };
        assert_eq!(name, "Home");
        let operation = *operation;
        assert_eq!(manager.library_name().await.unwrap(), "Two Devices");
        assert!(manager.acknowledge_admin_change(operation).await.is_err());

        laptop.database().start_sync().await.unwrap();
        crate::followed::value_matching(&mut changes, |changes| {
            changes.len() == 1 && changes[0].state == AdminChangeState::Done
        })
        .await;
        assert_eq!(manager.library_name().await.unwrap(), "Home");

        manager.acknowledge_admin_change(operation).await.unwrap();
        crate::followed::value_matching(&mut changes, Vec::is_empty).await;
        let listed = laptop
            .database()
            .query_texts_for_test("SELECT kind FROM coven_operations")
            .await
            .unwrap();
        assert!(listed.is_empty(), "{listed:?}");
    });
}

/// Robin's laptop, offline, asks to remove her tablet, and her tablet
/// removes the laptop first. Once the laptop syncs it applies its own
/// removal, which ends the change it asked for unmade: nothing refused it,
/// and the laptop lists it as stopped, naming the tablet, because the
/// laptop was removed.
#[test]
fn a_change_this_devices_removal_ends_is_listed_as_stopped() {
    run_two_device_test(async |devices| {
        let (laptop, tablet) = (devices.a(), devices.b());
        let (laptop_id, tablet_id) = (laptop.database().device_id(), tablet.database().device_id());
        let active = |device: coven::DeviceId| async move {
            laptop
                .manager()
                .get_members()
                .await
                .unwrap()
                .members
                .iter()
                .any(|member| member.active_devices.iter().any(|d| d.id == device))
        };
        laptop.wait_until("the tablet's registration", || active(tablet_id)).await;
        laptop.stop_sync().await;
        let mut changes = laptop.manager().subscribe_admin_changes();
        laptop.manager().remove_device(tablet_id).await.unwrap();

        tablet.manager().remove_device(laptop_id).await.unwrap();
        tablet
            .wait_until("the laptop's removal", || async {
                tablet
                    .manager()
                    .get_members()
                    .await
                    .unwrap()
                    .members
                    .iter()
                    .all(|member| member.active_devices.iter().all(|d| d.id != laptop_id))
            })
            .await;

        laptop.database().start_sync().await.unwrap();
        let stopped = crate::followed::value_matching(&mut changes, |changes| {
            changes.iter().any(|change| change.state != AdminChangeState::Waiting)
        })
        .await;
        let [AdminChange {
            change: AdminChangeKind::RemoveDevice { device, name },
            state: AdminChangeState::Stopped(coven::DeviceStop::Removed),
            ..
        }] = stopped.as_slice()
        else {
            panic!("one removal, stopped by the laptop's own: {stopped:?}");
        };
        assert_eq!((*device, name.as_str()), (tablet_id, "Device B"));
    });
}
