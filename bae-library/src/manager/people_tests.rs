//! People and devices: Add Device's pairing, the admin changes a device asks
//! for, invites, a profile request, and the membership read without a provider.

use super::DeviceCutOff;
use crate::manager::tests::*;
use crate::test_libraries::run_two_device_test;
use crate::test_libraries::{run_library_test, TestDevice};
use bae_people::admin_change::{AdminChange, AdminChangeKind, AdminChangeState};
use bae_people::membership::ActiveDevice;
use bae_people::people::{RequestedProfile, RequestedProfileState};

/// The first device's status of a pairing, once the new device's request has
/// come: waiting for the person's decision, with the name the new device gave
/// itself.
async fn request_that_comes(
    first: &TestDevice,
    pairing: coven::AddDeviceId,
) -> coven::AddDeviceRequest {
    let mut status = first.manager().follow_add_device(pairing);
    let status = crate::followed::value_matching(&mut status, |status| {
        matches!(status, Some(coven::AddDeviceStatus::Waiting { request: Some(_) }))
    })
    .await;
    let Some(coven::AddDeviceStatus::Waiting { request: Some(request) }) = status else {
        unreachable!("the status matched a waiting request")
    };
    request
}

/// The person's phone pairs with the QR code Add Device shows, which carries
/// the library's S3 key. Its request reaches the first device while the
/// status is followed, which syncs often; approving it lets the phone in, and
/// the status ends as the phone added.
#[test]
fn an_approved_request_adds_the_device_that_scanned_the_qr_code() {
    run_library_test(async |library| {
        let first = library.first();
        let adding = first.manager().add_device().await.unwrap();
        let (phone, ()) = tokio::join!(library.pair(&adding.qr_text, None, "Phone"), async {
            let request = request_that_comes(first, adding.id).await;
            assert_eq!(request.device_name, "Phone");
            first.manager().approve_add_device(&request).await.unwrap();
        });
        let phone = phone.expect("the approved request lets the phone in");
        let phone_device = this_device(&phone).await;
        let mut status = first.manager().follow_add_device(adding.id);
        let ended = crate::followed::value_matching(&mut status, |status| {
            matches!(status, Some(coven::AddDeviceStatus::Ended(_)))
        })
        .await;
        first
            .wait_until("the first device applies the phone's registration", || {
                has_member_device(first, &phone_device.id)
            })
            .await;
        phone.close().await;
        let Some(coven::AddDeviceStatus::Ended(coven::AddDeviceEnd::Added { device, name })) =
            ended
        else {
            panic!("the pairing ended as the phone added: {ended:?}");
        };
        assert_eq!((device, name), (phone_device.id, phone_device.name));
        first.manager().end_add_device(adding.id).await.unwrap();
    });
}

/// The code to type, in groups of four, carries no key: the person types it
/// on the phone with the library's S3 key, and the request goes the same way.
#[test]
fn the_code_to_type_pairs_with_the_s3_key_typed_beside_it() {
    run_library_test(async |library| {
        let first = library.first();
        let adding = first.manager().add_device().await.unwrap();
        assert_ne!(adding.code, adding.qr_text, "the QR code carries a key the typed one does not");
        let (phone, ()) = tokio::join!(
            library.pair(&adding.code, Some(bae_store::test_devices::owner_credentials()), "Phone"),
            async {
                let request = request_that_comes(first, adding.id).await;
                first.manager().approve_add_device(&request).await.unwrap();
            }
        );
        let phone = phone.expect("the approved request lets the phone in");
        phone.close().await;
    });
}

/// Declining the request ends the pairing without the phone: the phone's call
/// returns none, and the status ends as declined.
#[test]
fn a_declined_request_ends_the_pairing_without_the_device() {
    run_library_test(async |library| {
        let first = library.first();
        let adding = first.manager().add_device().await.unwrap();
        let (phone, ()) = tokio::join!(library.pair(&adding.qr_text, None, "Phone"), async {
            let request = request_that_comes(first, adding.id).await;
            first.manager().decline_add_device(&request).await.unwrap();
        });
        assert!(phone.is_none(), "a declined request lets no device in");
        let mut status = first.manager().follow_add_device(adding.id);
        crate::followed::value_matching(&mut status, |status| {
            matches!(status, Some(coven::AddDeviceStatus::Ended(coven::AddDeviceEnd::Declined)))
        })
        .await;
        first.manager().end_add_device(adding.id).await.unwrap();
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

        manager
            .membership
            .rename_store(bae_store::EnteredName::parse("Home").unwrap().as_str())
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
        assert_eq!(manager.store.store_name().await.unwrap(), "Two Devices");
        assert!(manager.membership.acknowledge_admin_change(operation).await.is_err());

        laptop.store().start_sync().await.unwrap();
        crate::followed::value_matching(&mut changes, |changes| {
            changes.len() == 1 && changes[0].state == AdminChangeState::Done
        })
        .await;
        assert_eq!(manager.store.store_name().await.unwrap(), "Home");

        manager.membership.acknowledge_admin_change(operation).await.unwrap();
        crate::followed::value_matching(&mut changes, Vec::is_empty).await;
        let listed = laptop
            .store()
            .query_texts_for_test("SELECT kind FROM coven_operations WHERE kind = 'RenameStore'")
            .await
            .unwrap();
        assert!(listed.is_empty(), "{listed:?}");
        let again = manager.membership.acknowledge_admin_change(operation).await;
        assert!(
            matches!(
                again,
                Err(bae_people::admin_change::AdminChangeDismissError::NotAdminChange(again))
                    if again == operation
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
                .membership()
                .members()
                .await
                .unwrap()
                .members
                .iter()
                .any(|member| member.active_devices.iter().any(|d| d.id == device))
        };
        laptop.wait_until("the tablet's registration", || active(tablet_id)).await;
        laptop.stop_sync().await;
        let mut changes = laptop.manager().subscribe_admin_changes();
        laptop.membership().remove_device(tablet_id).await.unwrap();

        tablet.membership().remove_device(laptop_id).await.unwrap();
        tablet
            .wait_until("the laptop's removal", || async {
                tablet
                    .membership()
                    .members()
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
            state: AdminChangeState::Stopped(coven::DeviceStop::Removed(coven::Removal::Device)),
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
        laptop.membership().remove_device(unknown).await.unwrap();
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
                coven::InviteAccess::S3AccessKey {
                    access_key_id: "dan-key".into(),
                    secret_access_key: coven::SecretText::new("dan-secret".into()),
                },
                coven::InviteLifetime::SevenDays,
                "Ana".into(),
            )
            .await
            .unwrap();
        let listed =
            crate::followed::value_matching(&mut invites, |invites| !invites.is_empty()).await;
        assert_eq!(
            listed
                .iter()
                .filter_map(bae_views::InviteEntry::open)
                .map(|listed| (listed.invite.id, listed.invite.admits))
                .collect::<Vec<_>>(),
            vec![(invite, coven::InviteAdmits::NewPerson)]
        );
        let granted = crate::followed::value_matching(&mut invites, |invites| {
            invites.iter().any(|listed| {
                matches!(
                    crate::people::invite_shown(&listed.open().unwrap().invite),
                    crate::people::InviteShown::Code { .. }
                )
            })
        })
        .await;
        assert!(granted
            .iter()
            .filter_map(bae_views::InviteEntry::open)
            .all(|listed| listed.held.is_none()));
        manager.cancel_invite(&invite).await.unwrap();
        crate::followed::value_matching(&mut invites, Vec::is_empty).await;
    });
}

#[tokio::test]
async fn membership_reads_committed_local_state_without_a_provider() {
    let (test, _temp) = setup_test_manager().await;
    let manager = test.library.manager();
    let membership = manager.membership.members().await.unwrap();
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
        tablet.people().create_profile("Robin").await.unwrap();
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
                .membership()
                .members()
                .await
                .unwrap()
                .members
                .iter()
                .any(|member| member.active_devices.iter().any(|d| d.id == tablet_id))
        };
        laptop.wait_until("the tablet's registration", tablet_active).await;
        laptop.membership().remove_device(tablet_id).await.unwrap();
        laptop.wait_until("the tablet's removal", || async { !tablet_active().await }).await;

        tablet.store().start_sync().await.unwrap();
        let stopped = RequestedProfile {
            name: "Robin".into(),
            state: RequestedProfileState::Stopped(coven::DeviceStop::Removed(
                coven::Removal::Device,
            )),
        };
        crate::followed::value_matching(&mut requested, |requested| {
            requested.as_ref() == Some(&stopped)
        })
        .await;
        assert!(tablet.people().create_profile("Robin").await.is_err());
        let mut after = tablet.manager().subscribe_requested_profile();
        let first = after.next().await.unwrap().unwrap();
        assert_eq!(first, Some(stopped));
    });
}

/// Robin's library is on S3, where no account is shared and none holds the
/// storage: removing her own tablet leaves nothing to do at the provider and
/// offers no cut-off, and confirming one is refused as an error, since it
/// can't succeed (§14.2).
#[test]
fn on_s3_no_member_holds_the_storage_and_a_cut_off_cant_be_confirmed() {
    run_two_device_test(async |devices| {
        let (laptop, tablet) = (devices.a(), devices.b());
        let tablet_id = tablet.store().device_id();
        laptop
            .wait_until("the tablet's registration", || has_member_device(laptop, &tablet_id))
            .await;
        let removal = laptop.manager().remove_device(tablet_id).await.unwrap();
        assert!(removal.instruction.is_none() && !removal.offers_cut_off, "{removal:?}");
        let answer = laptop.manager().confirm_device_cut_off(tablet_id).await;
        assert!(
            matches!(
                answer,
                Err(crate::manager::LibraryError::Sync(coven::SyncError::CutOffRefused(
                    coven::CutOffRefusal::NoAccountShared
                )))
            ),
            "{answer:?}"
        );
    });
}

/// A provider request to one of the accounts runs when Robin confirms: coven
/// changes nothing and says to call again once it ends, which is not a failure
/// of the library, so the person is asked to try again.
#[test]
fn a_running_provider_request_asks_to_confirm_again() {
    use super::device_cut_off;
    let not_gone = coven::SyncError::CutOffRefused(coven::CutOffRefusal::DeviceNotGone);
    assert_eq!(device_cut_off(Err(not_gone)).unwrap(), DeviceCutOff::DeviceNotGone);
    let refused =
        device_cut_off(Err(coven::SyncError::CutOffRefused(coven::CutOffRefusal::NotStoreOwner)));
    assert!(matches!(refused, Err(crate::manager::LibraryError::Sync(_))), "{refused:?}");
    let running = Err(coven::SyncError::from(coven::DbError::ProviderRequestRunning));
    assert_eq!(device_cut_off(running).unwrap(), DeviceCutOff::ProviderRequestRunning);
    assert_eq!(device_cut_off(Ok(())).unwrap(), DeviceCutOff::Confirmed);
    let refused = device_cut_off(Err(coven::SyncError::NoStorage));
    assert!(
        matches!(refused, Err(crate::manager::LibraryError::Sync(coven::SyncError::NoStorage))),
        "{refused:?}"
    );
}
