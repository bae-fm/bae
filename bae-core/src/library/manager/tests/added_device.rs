//! Add Device's sheet watches for the person's device that joins while it
//! shows the code.

use crate::sync::membership::ActiveDevice;
use crate::sync::test_devices::{run_library_test, TestDevice};
use std::time::Duration;

/// The person's phone restores from the code Add Device shows while the
/// watch on the library's first device is open: that device's next sync pass
/// applies the phone's registration, and the watch reports the phone.
#[test]
fn the_watch_reports_the_first_device_that_joins_while_it_is_open() {
    run_library_test(async |library| {
        let first = library.first();
        let adding = first.manager().add_device().await.unwrap();
        let mut watch = first.manager().subscribe_added_device(adding.watch);
        assert_eq!(
            watch.next().await.expect("the watch stays open").unwrap(),
            None
        );
        let phone = library.restore(&adding.share.code, "Phone").await;
        let added = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                first.manager().trigger_sync();
                tokio::select! {
                    value = watch.next() => {
                        break value.expect("the watch stays open").unwrap();
                    }
                    () = tokio::time::sleep(Duration::from_millis(100)) => {}
                }
            }
        })
        .await
        .expect("the watch reports the phone");
        let phone_device = this_device(&phone).await;
        phone.close().await;
        let added = added.expect("the phone was added");
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
        assert_eq!(
            watch.next().await.expect("the watch stays open").unwrap(),
            None
        );
        let phone = library.restore(&adding.share.code, "Phone").await;
        let added = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let value = watch.next().await.expect("the watch stays open");
                if let Some(device) = value.unwrap() {
                    break device;
                }
            }
        })
        .await
        .expect("an idle pass within 15 s reports the phone");
        let phone_device = this_device(&phone).await;
        phone.close().await;
        assert_eq!(added.id, phone_device.id);
    });
}

/// This device's own registration, as its own membership lists it.
async fn this_device(device: &TestDevice) -> ActiveDevice {
    device
        .database()
        .membership()
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
        tokio::time::timeout(Duration::from_secs(20), async {
            while !first
                .database()
                .membership()
                .await
                .unwrap()
                .members
                .iter()
                .any(|member| {
                    member
                        .active_devices
                        .iter()
                        .any(|d| d.id == phone_device.id)
                })
            {
                first.manager().trigger_sync();
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("the first device applies the phone's registration");
        let mut watch = first.manager().subscribe_added_device(adding.watch);
        let added = watch.next().await.expect("the watch stays open").unwrap();
        phone.close().await;
        let added = added.expect("the phone was added");
        assert_eq!((added.id, added.name), (phone_device.id, phone_device.name));
    });
}
