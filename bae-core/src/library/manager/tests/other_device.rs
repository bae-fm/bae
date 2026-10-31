//! A release whose audio is on another device only: what each device says
//! of it, and what it offers, as coven's records of its files and of that
//! device change.

use super::*;
use crate::album_detail::{OtherDevice, ReleasePlayability, ReleaseStorageAction};
use crate::sync::membership::FormerDeviceState;

/// How long a test waits for a live read to deliver a value.
const DELIVERY: std::time::Duration = std::time::Duration::from_secs(20);

/// The next value of a release's live read for which `wanted` holds, with
/// what each device's disk has said of no original.
async fn next_playability(
    manager: &LibraryManager,
    live: &mut coven::ReconfigurableLiveQuery<Option<String>, crate::db::ReleaseDetailProjection>,
    release_id: &str,
    wanted: impl Fn(&ReleasePlayability) -> bool,
) -> ReleasePlayability {
    tokio::time::timeout(DELIVERY, async {
        loop {
            let projection = live
                .next()
                .await
                .expect("the release's read stays open")
                .result
                .expect("the release reads");
            let playability = manager
                .resolve_release_detail_projection(
                    release_id,
                    projection,
                    &crate::album_detail::OriginalsSeen::default(),
                )
                .unwrap()
                .expect("the release exists")
                .playability;
            if wanted(&playability) {
                return playability;
            }
        }
    })
    .await
    .expect("the release's read delivers the playability looked for")
}

/// The release's live read on another device names the device its audio
/// is on, and says it plays once that device moves it to the cloud and the
/// switch arrives.
#[test]
fn the_live_release_names_the_device_its_audio_is_on_until_it_moves_to_the_cloud() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let originals = TempDir::new().unwrap();
        let release = release_on_the_first_device(devices, originals.path(), "Album").await;
        let manager = devices.b().manager();
        let mut live = manager.subscribe_release_detail(Some(release.id.clone()));

        assert_eq!(
            next_playability(manager, &mut live, &release.id, |_| true).await,
            ReleasePlayability::OnOtherDevice(OtherDevice::Active {
                name: "Device A".to_string()
            })
        );

        devices
            .a()
            .manager()
            .upload_release(&release.id, false)
            .await
            .unwrap();
        devices
            .converge(
                "SELECT f.id, f.file, c.coordinates FROM release_files AS f \
                 JOIN coven_files AS c ON c.id = f.file",
            )
            .await;
        next_playability(manager, &mut live, &release.id, |playability| {
            *playability == ReleasePlayability::Playable
        })
        .await;
    });
}

/// The storage actions a release offers on `manager`'s device, and its
/// download control.
async fn offered(
    manager: &LibraryManager,
    release_id: &str,
) -> (
    Vec<ReleaseStorageAction>,
    Option<crate::album_detail::ReleaseDownloadStatus>,
) {
    let detail = manager
        .find_release_detail(release_id)
        .await
        .unwrap()
        .expect("the release exists");
    (detail.summary.storage_actions, detail.download_status)
}

/// Pin keeps stored files on this device, so neither device offers it for a
/// release whose audio is on one device only, and both offer it once that
/// audio is in the cloud.
#[test]
fn pin_is_offered_only_for_a_release_in_the_cloud() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let originals = TempDir::new().unwrap();
        let release = release_on_the_first_device(devices, originals.path(), "Album").await;
        assert_eq!(
            offered(devices.a().manager(), &release.id).await,
            (vec![ReleaseStorageAction::Upload], None)
        );
        assert_eq!(
            offered(devices.b().manager(), &release.id).await,
            (Vec::new(), None)
        );

        devices
            .a()
            .manager()
            .upload_release(&release.id, false)
            .await
            .unwrap();
        devices
            .settle("the release in the cloud", |_, database| {
                let release_id = release.id.clone();
                async move {
                    database
                        .find_release_storage_summary(&release_id)
                        .await
                        .unwrap()
                        .expect("the release exists")
                        .remote
                }
            })
            .await;

        let in_the_cloud = (
            vec![ReleaseStorageAction::Pin, ReleaseStorageAction::DetachAudio],
            Some(crate::album_detail::ReleaseDownloadStatus::Available),
        );
        assert_eq!(
            offered(devices.a().manager(), &release.id).await,
            in_the_cloud
        );
        assert_eq!(
            offered(devices.b().manager(), &release.id).await,
            in_the_cloud
        );
    });
}

/// A device removed from the library keeps its files, and no device can move
/// them to the cloud any more: the release names it as removed.
#[test]
fn a_release_on_a_removed_device_names_it_as_removed() {
    crate::sync::test_devices::run_two_device_test(async |devices| {
        let originals = TempDir::new().unwrap();
        let release = insert_local_release_with_files(
            devices.b().manager(),
            originals.path(),
            "Album",
            &[("track1.flac", b"audio")],
        )
        .await;
        devices
            .converge(
                "SELECT f.id, f.file, c.coordinates FROM release_files AS f \
                 JOIN coven_files AS c ON c.id = f.file",
            )
            .await;
        let manager = devices.a().manager();
        let playability = async || {
            manager
                .find_release_detail(&release.id)
                .await
                .unwrap()
                .expect("the release exists")
                .playability
        };
        assert_eq!(
            playability().await,
            ReleasePlayability::OnOtherDevice(OtherDevice::Active {
                name: "Device B".to_string()
            })
        );

        manager
            .remove_device(devices.b().database().device_id())
            .await
            .unwrap();
        assert_eq!(
            playability().await,
            ReleasePlayability::OnOtherDevice(OtherDevice::Former {
                name: "Device B".to_string(),
                state: FormerDeviceState::Removed,
            })
        );
    });
}
