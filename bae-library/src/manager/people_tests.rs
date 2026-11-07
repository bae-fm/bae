//! People and devices: the Add Device watch, the admin changes a device asks
//! for, invites, a profile request, and the membership read without a provider.

use crate::manager::tests::*;
use crate::test_libraries::run_two_device_test;
use crate::test_libraries::{run_library_test, TestDevice};
use bae_people::admin_change::{AdminChange, AdminChangeKind, AdminChangeState};
use bae_people::membership::ActiveDevice;
use bae_people::people::{RequestedProfile, RequestedProfileState};

/// The person's phone restores from the code Add Device shows while the
/// watch on the library's first device is open: that device's next sync pass
/// applies the phone's registration, and the watch reports the phone.
#[test]
fn the_watch_reports_the_first_device_that_joins_while_it_is_open() {
    run_library_test(async |library| {
        let first = library.first();
        let adding = first.manager().add_device().await.unwrap();
        let mut watch = first.manager().subscribe_added_device(adding.watch);
        assert_eq!(watch.next().await.expect("the watch stays open").unwrap(), None);
        let phone = library.restore(&adding.share.code, "Phone").await;
        let phone_device = this_device(&phone).await;
        first
            .wait_until("the first device applies the phone's registration", || {
                has_member_device(first, &phone_device.id)
            })
            .await;
        let added = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            loop {
                let value = watch.next().await.expect("the watch stays open");
                if let Some(device) = value.unwrap() {
                    break device;
                }
            }
        })
        .await
        .expect("the watch reports the phone");
        phone.close().await;
        assert_eq!((added.id, added.name), (phone_device.id, phone_device.name));
    });
}

/// While the sheet watches, this device syncs every 5 seconds instead of
/// every 30: with no other request for a pass, the watch reports the phone
/// well before an idle pass 30 seconds after the library's last one would.
#[test]
fn the_watch_syncs_often_while_it_is_open() {
    run_library_test(async |library| {
        let first = library.first();
        let adding = first.manager().add_device().await.unwrap();
        let mut watch = first.manager().subscribe_added_device(adding.watch);
        assert_eq!(watch.next().await.expect("the watch stays open").unwrap(), None);
        let phone = library.restore(&adding.share.code, "Phone").await;
        let added = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
            loop {
                let value = watch.next().await.expect("the watch stays open");
                if let Some(device) = value.unwrap() {
                    break device;
                }
            }
        })
        .await
        .expect("an idle pass reports the phone");
        let phone_device = this_device(&phone).await;
        phone.close().await;
        assert_eq!(added.id, phone_device.id);
    });
}

/// Whether `device`'s membership lists the device `id` as active.
async fn has_member_device(device: &TestDevice, id: &coven::DeviceId) -> bool {
    device
        .membership()
        .members()
        .await
        .unwrap()
        .members
        .iter()
        .any(|member| member.active_devices.iter().any(|d| &d.id == id))
}

/// This device's own registration, as its own membership lists it.
async fn this_device(device: &TestDevice) -> ActiveDevice {
    device
        .membership()
        .members()
        .await
        .unwrap()
        .members
        .into_iter()
        .flat_map(|member| member.active_devices)
        .find(|device| device.is_self)
        .expect("the device is registered")
}

/// The watch's first look can come late: its subscription starts on the
/// library's runtime after the sheet has shown the code, and starts again
/// whenever the app subscribes again. A phone that restored from the code
/// the sheet showed, and that this device applied before that first look,
/// still joined after the code was shown, and is reported.
#[test]
fn the_watch_reports_a_device_that_joined_before_its_first_look() {
    run_library_test(async |library| {
        let first = library.first();
        let adding = first.manager().add_device().await.unwrap();
        let phone = library.restore(&adding.share.code, "Phone").await;
        let phone_device = this_device(&phone).await;
        first
            .wait_until("the first device applies the phone's registration", || {
                has_member_device(first, &phone_device.id)
            })
            .await;
        let mut watch = first.manager().subscribe_added_device(adding.watch);
        let added = watch.next().await.expect("the watch stays open").unwrap();
        phone.close().await;
        let added = added.expect("the phone was added");
        assert_eq!((added.id, added.name), (phone_device.id, phone_device.name));
    });
}

/// Robin renames the library on her laptop while it is offline. The rename
/// is listed as waiting, and the name stays; once the laptop syncs it is
/// done and the library has the name. Dismissing it before then is refused;
/// after, it leaves the list and coven's list of operations with it, and
/// dismissing it again is refused as no admin change of the laptop's.
#[test]
fn a_rename_is_listed_waiting_then_done_until_dismissed() {
    run_two_device_test(async |devices| {
        let laptop = devices.a();
        laptop.stop_sync().await;
        let manager = laptop.manager();
        let mut changes = manager.subscribe_admin_changes();
        crate::followed::value_matching(&mut changes, Vec::is_empty).await;

        manager.rename_library(&bae_store::EnteredName::parse("Home").unwrap()).await.unwrap();
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
        assert_eq!(manager.store.store_name().await.unwrap(), "Two Devices");
        assert!(manager.acknowledge_admin_change(operation).await.is_err());

        laptop.store().start_sync().await.unwrap();
        crate::followed::value_matching(&mut changes, |changes| {
            changes.len() == 1 && changes[0].state == AdminChangeState::Done
        })
        .await;
        assert_eq!(manager.store.store_name().await.unwrap(), "Home");

        manager.acknowledge_admin_change(operation).await.unwrap();
        crate::followed::value_matching(&mut changes, Vec::is_empty).await;
        let listed = laptop
            .store()
            .query_texts_for_test("SELECT kind FROM coven_operations WHERE kind = 'RenameStore'")
            .await
            .unwrap();
        assert!(listed.is_empty(), "{listed:?}");
        let again = manager.acknowledge_admin_change(operation).await;
        assert!(
            matches!(
                again,
                Err(crate::manager::LibraryError::AdminChangeDismiss(
                    bae_people::admin_change::AdminChangeDismissError::NotAdminChange(again)
                )) if again == operation
            ),
            "{again:?}"
        );
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
        let (laptop_id, tablet_id) = (laptop.store().device_id(), tablet.store().device_id());
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

        laptop.store().start_sync().await.unwrap();
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
        assert_eq!((*device, name.as_deref()), (tablet_id, Some("Device B")));
    });
}

/// Robin's laptop asks to remove a device it hasn't read: coven journals
/// the removal, since a device registered elsewhere may not have reached
/// the laptop yet. The change is listed waiting, with no name to show, and
/// once the laptop syncs and still finds no such device, the library
/// refuses it as gone.
#[test]
fn a_removal_of_a_device_this_device_hasnt_read_is_refused_as_gone() {
    run_two_device_test(async |devices| {
        let laptop = devices.a();
        laptop.stop_sync().await;
        let mut changes = laptop.manager().subscribe_admin_changes();
        crate::followed::value_matching(&mut changes, Vec::is_empty).await;
        let unknown = coven::DeviceId(u64::MAX);
        laptop.manager().remove_device(unknown).await.unwrap();
        let waiting =
            crate::followed::value_matching(&mut changes, |changes| !changes.is_empty()).await;
        let [AdminChange {
            change: AdminChangeKind::RemoveDevice { device, name: None },
            state: AdminChangeState::Waiting,
            ..
        }] = waiting.as_slice()
        else {
            panic!("one waiting removal of an unnamed device: {waiting:?}");
        };
        assert_eq!(*device, unknown);

        laptop.store().start_sync().await.unwrap();
        let refused = crate::followed::value_matching(&mut changes, |changes| {
            changes.iter().any(|change| change.state != AdminChangeState::Waiting)
        })
        .await;
        let [AdminChange {
            state:
                AdminChangeState::Refused(coven::OperationRejection::TargetGone(
                    coven::EntryTarget::Device(gone),
                )),
            ..
        }] = refused.as_slice()
        else {
            panic!("one removal refused as gone: {refused:?}");
        };
        assert_eq!(*gone, unknown);
    });
}

/// An invite this device makes is listed from the write that makes it,
/// granting its access, then with its code once a pass has granted it, until
/// it is cancelled, read from coven's open invites as they change.
#[test]
fn an_invite_is_listed_until_it_is_cancelled() {
    run_library_test(async |library| {
        let manager = library.first().manager();
        let views = bae_views::LibraryViews::new(library.first().store().clone());
        let mut invites = views.subscribe_invites();
        crate::followed::value_matching(&mut invites, Vec::is_empty).await;
        let invite = manager
            .store
            .invite_person(
                coven::MemberRole::Member,
                "Dan".into(),
                coven::InviteAccess::S3AccessKey {
                    access_key_id: "dan-key".into(),
                    secret_access_key: coven::SecretText::new("dan-secret".into()),
                },
                coven::InviteLifetime::Day,
            )
            .await
            .unwrap();
        let listed =
            crate::followed::value_matching(&mut invites, |invites| !invites.is_empty()).await;
        assert_eq!(
            listed
                .iter()
                .map(|listed| (listed.invite.id, listed.invite.invitee.clone()))
                .collect::<Vec<_>>(),
            vec![(invite, Some("Dan".to_string()))]
        );
        let granted = crate::followed::value_matching(&mut invites, |invites| {
            invites.iter().any(|listed| {
                matches!(
                    crate::people::invite_shown(&listed.invite),
                    crate::people::InviteShown::Code { .. }
                )
            })
        })
        .await;
        assert!(granted.iter().all(|listed| listed.held.is_none()));
        manager.cancel_invite(&invite).await.unwrap();
        crate::followed::value_matching(&mut invites, Vec::is_empty).await;
    });
}

#[tokio::test]
async fn membership_reads_committed_local_state_without_a_provider() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let membership = manager.get_members().await.unwrap();
    assert!(!manager.is_sync_configured().await.unwrap());
    assert!(membership.members.iter().all(|member| member.is_self));
}

/// Robin asks for her profile on her tablet while it is offline, and her
/// laptop removes the tablet before the request syncs. The tablet shows the
/// profile being made; once it syncs and applies its own removal, coven
/// ends the circle unmade, and the tablet shows that its removal stopped
/// it. Asking again there is refused, and the stop stays shown.
#[test]
fn a_profile_request_shows_the_stop_that_ended_its_circle() {
    run_two_device_test(async |devices| {
        let tablet = devices.b();
        tablet.stop_sync().await;
        let mut requested = tablet.manager().subscribe_requested_profile();
        crate::followed::value_matching(&mut requested, Option::is_none).await;
        tablet.manager().create_profile("Robin").await.unwrap();
        crate::followed::value_matching(&mut requested, |requested| {
            requested
                == &Some(RequestedProfile {
                    name: "Robin".into(),
                    state: RequestedProfileState::Making,
                })
        })
        .await;

        let tablet_id = tablet.store().device_id();
        let laptop = devices.a();
        let tablet_active = || async {
            laptop
                .manager()
                .get_members()
                .await
                .unwrap()
                .members
                .iter()
                .any(|member| member.active_devices.iter().any(|d| d.id == tablet_id))
        };
        laptop.wait_until("the tablet's registration", tablet_active).await;
        laptop.manager().remove_device(tablet_id).await.unwrap();
        laptop.wait_until("the tablet's removal", || async { !tablet_active().await }).await;

        tablet.store().start_sync().await.unwrap();
        let stopped = RequestedProfile {
            name: "Robin".into(),
            state: RequestedProfileState::Stopped(coven::DeviceStop::Removed),
        };
        crate::followed::value_matching(&mut requested, |requested| {
            requested.as_ref() == Some(&stopped)
        })
        .await;
        assert!(tablet.manager().create_profile("Robin").await.is_err());
        let mut after = tablet.manager().subscribe_requested_profile();
        let first = after.next().await.unwrap().unwrap();
        assert_eq!(first, Some(stopped));
    });
}
