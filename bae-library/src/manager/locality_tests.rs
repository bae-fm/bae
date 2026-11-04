//! A release's storage transitions: the row each transition shows until the
//! write that finishes it, making uploaded audio local again, unpinning,
//! cancelling a transition, and a release whose audio is on another device.

use super::*;
use crate::manager::tests::*;
use bae_db::membership::FormerDeviceState;
use bae_db::OtherDevice;
use bae_transfers::StoredTransition;
use bae_types::release_storage::ReleaseStorageAction;
use bae_views::album_detail::ReleasePlayability;
use tempfile::TempDir;

#[test]
fn cancelling_queued_audio_keeps_only_this_device_playable() {
    crate::test_libraries::run_two_device_test(async |devices| {
        check_release_detachment(devices, false).await
    });
}

#[test]
fn making_uploaded_audio_local_keeps_only_this_device_playable() {
    crate::test_libraries::run_two_device_test(async |devices| {
        check_release_detachment(devices, true).await
    });
}

async fn check_release_detachment(devices: &crate::test_libraries::TwoDevices, uploaded: bool) {
    devices.pause().await;
    let manager = devices.a().manager();
    manager.database.set_uploads_paused(true).await.unwrap();
    let original = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let album = create_test_album();
    manager.database.insert_album(&album).await.unwrap();
    let release = insert_local_release_in_album(
        manager,
        &album.id,
        original.path(),
        &[("a.flac", b"first audio"), ("b.flac", b"second audio")],
    )
    .await;
    store_test_cover(manager, &release.id, b"cover").await;
    let cover = cover_file(manager, &release.id).await.id();
    manager.upload_release(&release.id, false).await.unwrap();
    let files = release_files(manager, &release.id).await;
    assert_eq!(files.len(), 2);
    assert_eq!(moving_file_count(manager, &release.id).await, 2);

    if uploaded {
        manager.database.set_uploads_paused(false).await.unwrap();
        devices.resume().await;
        devices
            .converge(
                "SELECT f.id, f.file, hex(c.hash), c.coordinates FROM release_files AS f \
                 LEFT JOIN coven_files AS c ON c.id = f.file",
            )
            .await;
        assert_eq!(moving_file_count(manager, &release.id).await, 0);
        // The second device has cached the audio; detachment must still prevent playback.
        for file in &files {
            let expected = std::fs::read(original.path().join(&file.original_filename)).unwrap();
            assert_eq!(devices.b().database().read_release_file(&file.id).await.unwrap(), expected);
        }
        devices.pause().await;
        manager.database.unlock_store_key().await.unwrap();
        manager.detach_release_audio(&release.id, destination.path()).await.unwrap();
    } else {
        manager.cancel_release_transition(&release.id).await.unwrap();
        // Retrying the command cannot undo the local state.
        manager.cancel_release_transition(&release.id).await.unwrap();
    }

    // Cancelling the move leaves every file on this device only, as it was;
    // making the release local again switches every file to it. No row
    // changes either way.
    assert_eq!(moving_file_count(manager, &release.id).await, 0);
    for file in &files {
        let row = manager.database.find_file_by_id(&file.id).await.unwrap().unwrap();
        assert_eq!(row.file, file.file);
        assert!(!manager.database.find_file_by_id(&file.id).await.unwrap().unwrap().stored);
        let path = if uploaded { destination.path() } else { original.path() }
            .join(&file.original_filename);
        assert_eq!(manager.file_local_path(&file.id).await.unwrap(), Some(path));
        let expected = std::fs::read(original.path().join(&file.original_filename)).unwrap();
        assert_eq!(manager.database.read_release_file(&file.id).await.unwrap(), expected);
    }
    assert_eq!(cover_file(manager, &release.id).await.id(), cover);
    manager.database.set_uploads_paused(false).await.unwrap();
    devices.resume().await;
    devices
        .converge(
            "SELECT f.id, f.file, hex(c.hash), c.coordinates FROM release_files AS f \
                 LEFT JOIN coven_files AS c ON c.id = f.file",
        )
        .await;
    for file in &files {
        assert!(!devices.b().database().find_file_by_id(&file.id).await.unwrap().unwrap().stored);
        assert!(devices.b().manager().file_local_path(&file.id).await.unwrap().is_none());
        let read = devices.b().database().read_release_file(&file.id).await;
        assert!(
            matches!(read, Err(bae_db::DbError::FileRead(coven::FileReadError::DeviceOnly { .. }))),
            "{read:?}"
        );
        assert_eq!(
            playback_error_reason_for_file(devices.b().manager(), file).await,
            bae_playback::PlaybackErrorReason::OnOtherDevice(OtherDevice::Active {
                name: "Device A".to_string()
            })
        );
    }
}

/// Cancelling the move of a release to be kept here leaves it on this device
/// only: nothing is left to keep, and it plays from its original.
#[tokio::test]
async fn cancelling_the_upload_of_a_release_kept_here_detaches_audio() {
    let (manager, temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        temp.path(),
        "Album",
        &[("track.flac", b"audio")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();
    manager.cancel_release_transition(&release.id).await.unwrap();
    assert!(download_queue_now(&manager).await.rows.is_empty());
    assert_eq!(manager.database.queued_upload_count_for_test().await.unwrap(), 0);
    let files = release_files(&manager, &release.id).await;
    assert!(files.iter().all(|file| !file.stored));
    assert_eq!(manager.database.read_release_file(&files[0].id).await.unwrap(), b"audio");
}

/// The next value of a release's live read for which `wanted` holds.
async fn next_playability(
    live: &mut coven::ReconfigurableLiveQuery<Option<String>, bae_views::ReleaseDetailProjection>,
    release_id: &str,
    wanted: impl Fn(&ReleasePlayability) -> bool,
) -> ReleasePlayability {
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            let projection = live
                .next()
                .await
                .expect("the release's read stays open")
                .result
                .expect("the release reads");
            let playability =
                bae_views::release::resolve_release_detail_projection(release_id, projection)
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
    crate::test_libraries::run_two_device_test(async |devices| {
        let originals = TempDir::new().unwrap();
        let release = release_on_the_first_device(devices, originals.path(), "Album").await;
        let manager = devices.b().manager();
        let mut live = bae_views::LibraryViews::new(manager.database.clone())
            .subscribe_release_detail(Some(release.id.clone()));

        assert_eq!(
            next_playability(&mut live, &release.id, |_| true).await,
            ReleasePlayability::OnOtherDevice(OtherDevice::Active { name: "Device A".to_string() })
        );

        devices.a().manager().upload_release(&release.id, false).await.unwrap();
        devices
            .converge(
                "SELECT f.id, f.file, c.coordinates FROM release_files AS f \
                 JOIN coven_files AS c ON c.id = f.file",
            )
            .await;
        next_playability(&mut live, &release.id, |playability| {
            *playability == ReleasePlayability::Playable
        })
        .await;
    });
}

/// Pin keeps stored files on this device, so neither device offers it for a
/// release whose audio is on one device only, and both offer it once that
/// audio is in the cloud.
#[test]
fn pin_is_offered_only_for_a_release_in_the_cloud() {
    crate::test_libraries::run_two_device_test(async |devices| {
        let originals = TempDir::new().unwrap();
        let release = release_on_the_first_device(devices, originals.path(), "Album").await;
        assert_eq!(
            offered(devices.a().manager(), &release.id).await,
            (vec![ReleaseStorageAction::Upload], None)
        );
        assert_eq!(offered(devices.b().manager(), &release.id).await, (Vec::new(), None));

        devices.a().manager().upload_release(&release.id, false).await.unwrap();
        devices
            .settle("the release in the cloud", async |_, database| {
                matches!(
                    bae_views::LibraryViews::new(database.clone())
                        .find_release_detail(&release.id)
                        .await
                        .unwrap()
                        .expect("the release exists")
                        .summary
                        .storage_state,
                    bae_views::album_detail::ReleaseStorageState::InStorage { .. }
                )
            })
            .await;

        let in_the_cloud = (
            vec![ReleaseStorageAction::Pin, ReleaseStorageAction::DetachAudio],
            Some(bae_views::album_detail::ReleaseDownloadStatus::Available),
        );
        assert_eq!(offered(devices.a().manager(), &release.id).await, in_the_cloud);
        assert_eq!(offered(devices.b().manager(), &release.id).await, in_the_cloud);
    });
}

/// A device removed from the library keeps its files, and no device can move
/// them to the cloud any more: the release names it as removed.
#[test]
fn a_release_on_a_removed_device_names_it_as_removed() {
    crate::test_libraries::run_two_device_test(async |devices| {
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
            bae_views::LibraryViews::new(manager.database.clone())
                .find_release_detail(&release.id)
                .await
                .unwrap()
                .expect("the release exists")
                .playability
        };
        assert_eq!(
            playability().await,
            ReleasePlayability::OnOtherDevice(OtherDevice::Active { name: "Device B".to_string() })
        );

        manager.remove_device(devices.b().database().device_id()).await.unwrap();
        // The write journals the removal; a pass of this device claims it.
        devices
            .a()
            .wait_until("Device B's removal", || async {
                playability().await
                    == ReleasePlayability::OnOtherDevice(OtherDevice::Former {
                        name: "Device B".to_string(),
                        state: FormerDeviceState::Removed,
                    })
            })
            .await;
        // Playing it names the removed device the same way.
        let file = release_files(manager, &release.id).await.remove(0);
        assert_eq!(
            playback_error_reason_for_file(manager, &file).await,
            bae_playback::PlaybackErrorReason::OnOtherDevice(OtherDevice::Former {
                name: "Device B".to_string(),
                state: FormerDeviceState::Removed,
            })
        );
    });
}

/// Audio stays pinned while the cover remains in its separate evictable cache.
#[tokio::test]
async fn unpin_release_releases_audio_and_keeps_cover_evictable() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pinned-with-cover"),
        "Pinned With Cover",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    store_test_cover(&manager, &release.id, b"cover-bytes").await;
    finish_bound_uploads(&manager).await;

    manager.upload_release(&release.id, true).await.unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, 1, "the release's file uploads; its cover is stored already");
    wait_for_kept_release(&manager, &release.id).await;
    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    assert!(
        file_pinned(&manager, bae_db::synced::RELEASE_FILES_NAMESPACE, &file_id).await,
        "a move to be kept here leaves the release file pinned"
    );
    assert!(
        !file_pinned(&manager, bae_db::synced::COVERS_NAMESPACE, &release.id).await,
        "covers remain evictable when audio is pinned"
    );

    manager.unpin_release(&release.id).await.unwrap();

    assert!(
        !file_pinned(&manager, bae_db::synced::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file unpinned"
    );
    assert!(
        !file_pinned(&manager, bae_db::synced::COVERS_NAMESPACE, &release.id).await,
        "the cover remains evictable"
    );
}

/// A transition's row is there while its work runs; a work that fails or
/// panics has it deleted in a write of its own. A work that succeeds deletes
/// it in the write that finishes it, as the unpin and make-local tests below
/// show.
#[tokio::test]
async fn a_failed_transition_ends_in_a_write_of_its_own() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let shown = || async {
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id,
        )
        .await
        .unwrap()
    };

    let failed = manager
        .run_storage_action(&release.id, StoredTransition::Unpin, async {
            Err::<(), _>(LibraryError::Storage(format!(
                "the work failed, showing {:?}",
                shown().await
            )))
        })
        .await;
    let Err(LibraryError::Storage(failure)) = failed else {
        panic!("{failed:?}");
    };
    assert!(failure.ends_with("Some(Unpin)"), "the row is there while it runs: {failure}");
    assert_eq!(shown().await, None, "a failure ends it");

    let panicked = manager
        .run_storage_action(&release.id, StoredTransition::Unpin, async {
            if release.id.is_empty() {
                return Ok(());
            }
            panic!("the work panicked")
        })
        .await;
    assert!(panicked.is_err());
    assert_eq!(shown().await, None, "a panic ends it");
}

/// Unpinning releases a release's pins in the write that ends its
/// transition: a write that can't end it releases nothing, so the release
/// never shows the unpin once its files are evictable.
#[tokio::test]
async fn the_write_that_releases_a_releases_pins_ends_its_unpin() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("kept"),
        "Kept",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    wait_for_kept_release(&manager, &release.id).await;
    let file = release_files(&manager, &release.id).await.remove(0);
    let pinned =
        || async { file_pinned(&manager, bae_db::synced::RELEASE_FILES_NAMESPACE, &file.id).await };
    assert!(pinned().await);

    let refused = bae_transfers::release_storage_action::unpin_release(
        &manager.database,
        &release.id,
        vec![file.file],
    )
    .await;
    assert!(refused.is_err(), "no unpin was admitted for the write to end");
    assert!(pinned().await, "a write that can't end the transition releases no pin");

    manager.unpin_release(&release.id).await.unwrap();
    assert!(!pinned().await, "the unpin released the pin");
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
        .await
        .unwrap(),
        None,
        "and ended the transition"
    );
}

/// Making a release local switches its files in the write that ends its
/// transition: a write that can't end it switches nothing, so the release
/// never shows the transition once its files are here.
#[tokio::test]
async fn the_write_that_switches_a_releases_files_ends_its_make_local() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("uploaded"),
        "Uploaded",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let files = release_files(&manager, &release.id).await;
    assert!(files.iter().all(|file| file.stored));

    let refused_folder = temp_dir.path().join("refused");
    std::fs::create_dir(&refused_folder).unwrap();
    let paths = files
        .iter()
        .map(|file| {
            let path =
                bae_db::stored_name::file_path(&release.id, &file.id, &file.original_filename)
                    .unwrap();
            (file.file, path)
        })
        .collect();
    let (_stop, stopped) = tokio::sync::watch::channel(false);
    let prepared = manager
        .database
        .prepare_device_only(&refused_folder, paths, &stopped, |_| {})
        .await
        .unwrap();
    let refused = bae_transfers::release_storage_action::make_release_local(
        &manager.database,
        &release.id,
        prepared,
    )
    .await;
    assert!(refused.is_err(), "no make-local was admitted for the write to end");
    assert!(
        release_files(&manager, &release.id).await.iter().all(|file| file.stored),
        "a write that can't end the transition switches no file"
    );

    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    manager
        .detach_release_audio_into(&release.id, &folder, &crate::CancellationToken::new())
        .await
        .unwrap();
    assert!(
        release_files(&manager, &release.id).await.iter().all(|file| !file.stored),
        "making it local switched every file here"
    );
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
        .await
        .unwrap(),
        None,
        "and ended the transition"
    );
}

/// A release in a transition refuses another, whose work never runs, and
/// keeps the transition it is in.
#[tokio::test]
async fn a_release_in_a_transition_refuses_another() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    assert!(bae_transfers::release_storage_action::begin_release_storage_action(
        &manager.database,
        &release.id,
        StoredTransition::Unpin
    )
    .await
    .unwrap());

    let ran = std::sync::atomic::AtomicBool::new(false);
    let refused = manager
        .run_storage_action(&release.id, StoredTransition::DetachAudio, async {
            ran.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await;
    assert!(matches!(refused, Err(LibraryError::Storage(_))), "{refused:?}");
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst), "the refused work never runs");
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
        .await
        .unwrap(),
        Some(ReleaseStorageAction::Unpin)
    );
}

/// A move to the cloud shows as a transition until the write that queues its
/// files, which ends it; one whose files can't be queued ends it in a write
/// of its own.
#[tokio::test]
async fn the_write_that_queues_a_moves_files_ends_its_transition() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("moved"),
        "Moved",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, false).await.unwrap();
    assert_eq!(moving_file_count(&manager, &release.id).await, 1);
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
        .await
        .unwrap(),
        None
    );

    let empty = insert_release_without_files(&manager).await;
    let refused = manager.upload_release(&empty, false).await;
    assert!(refused.is_err(), "a release with no files has none to queue");
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(&manager.database, &empty)
            .await
            .unwrap(),
        None
    );
}

/// A release with no files, in an album of its own in `manager`'s library.
async fn insert_release_without_files(manager: &TestManager) -> String {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.database.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    release.id
}

/// While a release's copies download into the person's folder, the release
/// is listed with the bytes of the file coven downloads and of the whole
/// release; once its files have switched it is listed no more.
#[tokio::test]
async fn a_release_made_local_lists_its_bytes_while_its_copies_download() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release_id =
        uploaded_release(&manager, &temp_dir.path().join("made-local"), "Made Local", false).await;
    let file = release_files(&manager, &release_id).await.remove(0);
    let reference = manager.release_file_ref(&file.id).await.unwrap();
    manager.evict_file_for_test(file.file).await.unwrap();
    let coven::FilePlace::Cloud { location, .. } = reference.place() else {
        panic!("the uploaded file is stored");
    };

    // Hold the download one byte into the file's encrypted chunk.
    let (reached, observed) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_body_at(
        coven::ObjectPath::file(
            manager.database.store_id_for_test(),
            location.uploader(),
            location.object(),
        ),
        12,
        reached,
        resumed,
    )
    .await;
    let mut transfers = manager.watch_make_local_transfers();
    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    let making = tokio::spawn({
        let manager = manager.library.clone();
        let release_id = release_id.clone();
        async move {
            manager
                .detach_release_audio_into(&release_id, &folder, &crate::CancellationToken::new())
                .await
        }
    });
    observed.await.unwrap();
    let listed = tokio::time::timeout(
        bae_util::test_channels::HANG_GUARD,
        transfers.until(|transfers| transfers.contains_key(&release_id)),
    )
    .await
    .expect("the release is listed while its copy downloads")
    .unwrap()[&release_id];
    assert!(listed.file_bytes_total > 0, "{listed:?}");
    assert!(listed.file_bytes_done < listed.file_bytes_total, "{listed:?}");
    assert!(listed.bytes_done < listed.bytes_total, "{listed:?}");
    assert!(listed.bytes_total >= listed.file_bytes_total, "{listed:?}");

    resume.send(()).unwrap();
    making.await.unwrap().unwrap();
    tokio::time::timeout(
        bae_util::test_channels::HANG_GUARD,
        transfers.until(|transfers| transfers.is_empty()),
    )
    .await
    .expect("a release whose files switched is listed no more")
    .unwrap();
}

#[tokio::test]
async fn cancel_release_transition_fires_a_registered_transfer_token() {
    let (manager, _temp_dir) = setup_test_manager().await;

    // A registered audio detachment token is fired by the unified cancel.
    let token = crate::CancellationToken::new();
    let _registration = manager
        .detach_cancels
        .register(REL_X, token.clone())
        .expect("no other transfer of the release holds a token");
    manager.cancel_release_transition(REL_X).await.unwrap();
    assert!(token.is_cancelled(), "transfer token fired");

    // Nothing in progress for an unknown release → no-op, no error.
    manager.cancel_release_transition(REL_NONE).await.unwrap();
}

// Needs the mock cloud home with uploads running: audio detachment is only
// offered a release whose upload has been accepted, so the transition has
// to run all the way through a cycle and retire its intent first.
#[tokio::test]
async fn unmanage_cancelled_before_copy_leaves_release_remote() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_uploading(&manager).await;
    // Really Remote, through the real transition: audio detachment resolves the
    // release's current locality from coven, which a fabricated `remote` column
    // with no cloud objects behind it cannot answer.
    let release_id =
        upload_release_unpaused(&manager, &temp_dir.path().join("r1"), "Album One", false).await;

    // A token cancelled before coven downloads anything: the call stops
    // before its write and removes whatever it made.
    let token = crate::CancellationToken::new();
    token.cancel();
    let dest = temp_dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    manager
        .detach_release_audio_into(&release_id, &dest, &token)
        .await
        .expect_err("a cancelled audio detachment must not report completion");

    assert!(
        release_files(&manager, &release_id).await.iter().all(|file| file.stored),
        "cancellation leaves every file in the cloud"
    );
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0, "no copy is left");
}

const REL_NONE: &str = "cdb5eed8-db50-45f6-814a-dc623215ae04"; // was "rel-none"

const REL_X: &str = "e6b79143-6ab2-4358-8f53-460594936d69"; // was "rel-x"
