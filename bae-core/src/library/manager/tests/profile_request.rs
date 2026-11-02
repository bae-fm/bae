//! The profile a person asked for, as the library shows it while their
//! private circle is made, and once this device's stop for good ends it
//! (§5.4).

use crate::people::{RequestedProfile, RequestedProfileState};
use crate::sync::test_devices::run_two_device_test;

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

        let tablet_id = tablet.database().device_id();
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

        tablet.database().start_sync().await.unwrap();
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
