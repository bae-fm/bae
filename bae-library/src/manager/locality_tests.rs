//! A release's storage transitions: making uploaded audio local again
//! through the download queue, unpinning, cancelling a transition, and a
//! release whose audio is on another device.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use crate::manager::tests::*;
use bae_records::OtherDevice;
use bae_store::FormerDeviceState;
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

/// The folder name the releases made local in these tests are placed in.
const FOLDER: &str = "Release Folder";

async fn check_release_detachment(devices: &crate::test_libraries::TwoDevices, uploaded: bool) {
    devices.pause().await;
    let manager = devices.a().manager();
    manager.store.set_uploads_paused(true).await.unwrap();
    let original = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let album = create_test_album();
    manager.records.insert_album(&album).await.unwrap();
    let release = insert_local_release_in_album(
        manager,
        &album.id,
        original.path(),
        &[("a.flac", b"first audio"), ("b.flac", b"second audio")],
    )
    .await;
    manager.records.set_source_folder_name_for_test(&release.id, FOLDER).await.unwrap();
    store_test_cover(manager, &release.id, b"cover").await;
    let cover = cover_file(manager, &release.id).await.id();
    manager.upload_release(&release.id).await.unwrap();
    let files = release_files(manager, &release.id).await;
    assert_eq!(files.len(), 2);
    assert_eq!(moving_file_count(manager, &release.id).await, 2);

    if uploaded {
        manager.store.set_uploads_paused(false).await.unwrap();
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
            assert_eq!(devices.b().records().read_release_file(&file.id).await.unwrap(), expected);
        }
        devices.pause().await;
        manager.store.unlock_store_key().await.unwrap();
        make_local_and_wait(manager, std::slice::from_ref(&release.id), destination.path())
            .await
            .unwrap();
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
        let row = manager.records.find_file_by_id(&file.id).await.unwrap().unwrap();
        assert_eq!(row.file, file.file);
        assert!(!manager.records.find_file_by_id(&file.id).await.unwrap().unwrap().stored);
        let path = if uploaded {
            destination.path().join(FOLDER).join(&file.original_filename)
        } else {
            original.path().join(&file.original_filename)
        };
        assert_eq!(manager.file_local_path(&file.id).await.unwrap(), Some(path));
        let expected = std::fs::read(original.path().join(&file.original_filename)).unwrap();
        assert_eq!(manager.records.read_release_file(&file.id).await.unwrap(), expected);
    }
    assert_eq!(cover_file(manager, &release.id).await.id(), cover);
    manager.store.set_uploads_paused(false).await.unwrap();
    devices.resume().await;
    devices
        .converge(
            "SELECT f.id, f.file, hex(c.hash), c.coordinates FROM release_files AS f \
                 LEFT JOIN coven_files AS c ON c.id = f.file",
        )
        .await;
    for file in &files {
        assert!(!devices.b().records().find_file_by_id(&file.id).await.unwrap().unwrap().stored);
        assert!(devices.b().manager().file_local_path(&file.id).await.unwrap().is_none());
        let read = devices.b().records().read_release_file(&file.id).await;
        assert!(
            matches!(
                &read,
                Err(bae_store::DbError::FileRead(error))
                    if matches!(**error, coven::FileReadError::DeviceOnly { .. })
            ),
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

/// The next value of a release's live read for which `wanted` holds.
async fn next_playability(
    live: &mut coven::ReconfigurableLiveQuery<
        Option<String>,
        bae_views::ReleaseDetailProjection,
        bae_store::DbError,
    >,
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
        let mut live = bae_views::LibraryViews::new(manager.store.clone())
            .subscribe_release_detail(Some(release.id.clone()));

        assert_eq!(
            next_playability(&mut live, &release.id, |_| true).await,
            ReleasePlayability::OnOtherDevice(OtherDevice::Active { name: "Device A".to_string() })
        );

        devices.a().manager().upload_release(&release.id).await.unwrap();
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

        devices.a().manager().upload_release(&release.id).await.unwrap();
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
            bae_views::LibraryViews::new(manager.store.clone())
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

        manager.membership.remove_device(devices.b().store().device_id()).await.unwrap();
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

/// The bytes coven keeps for one namespace.
async fn namespace_bytes(
    manager: &crate::LibraryManager,
    namespace: &str,
) -> coven::NamespaceBytes {
    manager
        .store
        .local_bytes()
        .await
        .unwrap()
        .namespaces
        .into_iter()
        .find(|bytes| bytes.namespace == namespace)
        .unwrap_or_else(|| panic!("{namespace} is declared"))
}

/// Unpinning removes the release's audio from this device with budget room to
/// spare, and leaves its cover in the covers cache.
#[tokio::test]
async fn unpinning_a_release_removes_its_audio_from_this_device_and_leaves_its_cover_cached() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("pinned-with-cover"),
        "Pinned With Cover",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    store_test_cover(manager, &release.id, b"cover-bytes").await;
    finish_bound_uploads(manager).await;

    manager.upload_release(&release.id).await.unwrap();
    let uploaded = manager.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(uploaded, 1, "the release's file uploads; its cover is stored already");
    pin_directly(manager, &release.id).await;
    let file_id = release_files(manager, &release.id).await[0].id.clone();
    assert!(
        file_pinned(manager, bae_records::synced::RELEASE_FILES_NAMESPACE, &file_id).await,
        "pinning the release pins its file"
    );
    let audio = namespace_bytes(manager, bae_records::synced::RELEASE_FILES_NAMESPACE).await;
    assert!(audio.pinned > 0, "the pinned audio is measured as pinned: {audio:?}");
    assert!(audio.budget > audio.pinned, "the budget has room to spare");
    let covers_before = namespace_bytes(manager, bae_records::synced::COVERS_NAMESPACE).await;
    assert_eq!(covers_before.pinned, 0, "a cover is never pinned");

    manager.unpin_releases(std::slice::from_ref(&release.id)).await.unwrap();

    assert!(
        !file_pinned(manager, bae_records::synced::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file unpinned"
    );
    let audio = namespace_bytes(manager, bae_records::synced::RELEASE_FILES_NAMESPACE).await;
    assert_eq!((audio.pinned, audio.cached), (0, 0), "the audio left this device: {audio:?}");
    let covers = namespace_bytes(manager, bae_records::synced::COVERS_NAMESPACE).await;
    assert_eq!(covers.cached, covers_before.cached, "the cover stays cached");
}

/// Pinned copies don't count toward the namespace's budget: a pinned release
/// larger than the budget leaves room to cache a track played from the cloud.
#[tokio::test]
async fn a_pinned_release_over_the_budget_leaves_room_to_cache_a_played_track() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let pinned_bytes = vec![7_u8; 4096];
    let played_bytes = vec![9_u8; 512];
    let big = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("big"),
        "Big",
        &[("big.flac", pinned_bytes.as_slice())],
    )
    .await;
    let played = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("played"),
        "Played",
        &[("played.flac", played_bytes.as_slice())],
    )
    .await;
    for release in [&big, &played] {
        manager.upload_release(&release.id).await.unwrap();
    }
    manager.drain_uploads_expecting_work().await.unwrap();
    let played_file = release_files(manager, &played.id).await.remove(0);
    manager.evict_file_for_test(played_file.file).await.unwrap();

    manager
        .store
        .write(|sql| {
            sql.set_cache_budget(bae_records::synced::RELEASE_FILES_NAMESPACE, 1024)
                .map_err(bae_store::DbError::from)
        })
        .await
        .unwrap();
    pin_directly(manager, &big.id).await;
    let before = namespace_bytes(manager, bae_records::synced::RELEASE_FILES_NAMESPACE).await;
    assert!(before.pinned > before.budget, "the pinned release is over the budget: {before:?}");

    let played_ref = manager.release_file_ref(&played_file.id).await.unwrap();
    assert_eq!(manager.store.read_file(&played_ref).await.unwrap(), played_bytes);

    let after = namespace_bytes(manager, bae_records::synced::RELEASE_FILES_NAMESPACE).await;
    assert_eq!(after.pinned, before.pinned, "the pinned copy stays: {after:?}");
    assert!(after.cached > 0, "the played track is cached: {after:?}");
}

/// Unpinning a selection releases every pin in one write: no release shows an
/// unpin on the way, because no entry of one is written.
#[tokio::test]
async fn unpinning_a_selection_releases_every_pin_in_one_write() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let mut pinned = Vec::new();
    for name in ["a", "b"] {
        let release = insert_local_release_with_files(
            manager,
            &temp_dir.path().join(name),
            &format!("Kept {name}"),
            &[("track.flac", name.as_bytes())],
        )
        .await;
        manager.upload_release(&release.id).await.unwrap();
        manager.drain_uploads_expecting_work().await.unwrap();
        pin_directly(manager, &release.id).await;
        let file = release_files(manager, &release.id).await.remove(0);
        assert!(file_pinned(manager, bae_records::synced::RELEASE_FILES_NAMESPACE, &file.id).await);
        pinned.push((release.id, file.id));
    }

    let rows = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut transitions = manager.store.subscribe(|sql| {
        Ok(sql.query_row("SELECT COUNT(*) FROM release_storage_action", [], |row| {
            row.get::<_, i64>(0)
        })?)
    });
    let seen = rows.clone();
    let collector = tokio::spawn(async move {
        while let Some(Ok(count)) = transitions.next().await {
            seen.lock().unwrap().push(count);
        }
    });

    let ids: Vec<String> = pinned.iter().map(|(release, _)| release.clone()).collect();
    manager.unpin_releases(&ids).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    collector.abort();

    for (release, file) in &pinned {
        assert!(
            !file_pinned(manager, bae_records::synced::RELEASE_FILES_NAMESPACE, file).await,
            "release {release} unpinned"
        );
    }
    assert!(
        rows.lock().unwrap().iter().all(|count| *count == 0),
        "no release shows a transition: {:?}",
        rows.lock().unwrap()
    );
}

/// Making a release local switches its files in the write that ends its
/// queue entry: a write that can't end it switches nothing, so the release
/// is never local with its entry still queued.
#[tokio::test]
async fn the_write_that_switches_a_releases_files_ends_its_entry() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("uploaded"),
        "Uploaded",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.records.set_source_folder_name_for_test(&release.id, FOLDER).await.unwrap();
    manager.upload_release(&release.id).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let files = release_files(manager, &release.id).await;
    assert!(files.iter().all(|file| file.stored));

    // The copies are ready, but the release has no entry in the queue to end:
    // the write is refused and switches no file.
    let refused_folder = temp_dir.path().join("refused");
    std::fs::create_dir(&refused_folder).unwrap();
    let paths = files
        .iter()
        .map(|file| {
            let path = bae_records::stored_name::copy_path(
                &release.id,
                FOLDER,
                &file.id,
                &file.original_filename,
            )
            .unwrap();
            (file.file, path)
        })
        .collect();
    let (_stop, stopped) = tokio::sync::watch::channel(false);
    let prepared =
        manager.store.prepare_device_only(&refused_folder, paths, &stopped, |_| {}).await.unwrap();
    let entry = bae_transfers::tables::worker_moves::active_entry::<bae_transfers::DownloadRequest>(
        &release.id,
    );
    let refused = manager
        .store
        .call(move |sql| {
            sql.make_device_only(prepared)?;
            entry.finish_on(sql)
        })
        .await;
    assert!(
        matches!(refused, Err(bae_store::DbError::EntryCancelled { .. })),
        "no entry was queued for the write to end: {refused:?}"
    );
    assert!(
        release_files(manager, &release.id).await.iter().all(|file| file.stored),
        "a write that can't end the entry switches no file"
    );

    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    make_local_and_wait(manager, std::slice::from_ref(&release.id), &folder).await.unwrap();
    assert!(
        release_files(manager, &release.id).await.iter().all(|file| !file.stored),
        "making it local switched every file here"
    );
    assert!(download_queue_now(manager).await.rows.is_empty(), "and took it out of the queue");
}

/// A move to the cloud queues its files in one write; a release with no
/// files has none to queue.
#[tokio::test]
async fn the_write_that_queues_a_moves_files_is_the_move() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("moved"),
        "Moved",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id).await.unwrap();
    assert_eq!(moving_file_count(manager, &release.id).await, 1);

    let empty = insert_release_without_files(&test).await;
    let refused = manager.upload_release(&empty).await;
    assert!(refused.is_err(), "a release with no files has none to queue");
}

/// A release with no files, in an album of its own in `manager`'s library.
async fn insert_release_without_files(manager: &TestManager) -> String {
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.library.manager().records.insert_album(&album).await.unwrap();
    insert_release(manager.library.manager(), &release).await;
    release.id
}

/// While a release's copies download into the person's folder, the release
/// is listed with the bytes of the file coven downloads and of the whole
/// release; once its files have switched it is listed no more.
#[tokio::test]
async fn a_release_made_local_lists_its_bytes_while_its_copies_download() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release_id =
        uploaded_release(manager, &temp_dir.path().join("made-local"), "Made Local", false).await;
    manager.records.set_source_folder_name_for_test(&release_id, FOLDER).await.unwrap();
    let file = release_files(manager, &release_id).await.remove(0);
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
            manager.store.store_id_for_test(),
            location.uploader(),
            location.object(),
        ),
        12,
        reached,
        resumed,
    )
    .await;
    let mut transfers = manager.watch_download_transfers();
    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    manager.make_releases_local(vec![release_id.clone()], &folder).await.unwrap();
    observed.await.unwrap();
    let listed = tokio::time::timeout(
        bae_util::test_channels::HANG_GUARD,
        transfers.until(|transfers| {
            matches!(
                transfers.get(&release_id),
                Some(bae_transfers::DownloadTransfer::MakeLocal(progress))
                    if progress.file_bytes_total > 0
            )
        }),
    )
    .await
    .expect("the release is listed while its copy downloads")
    .unwrap()[&release_id]
        .clone();
    let bae_transfers::DownloadTransfer::MakeLocal(listed) = listed else {
        panic!("a make-local lists its own progress: {listed:?}");
    };
    assert!(listed.file_bytes_done < listed.file_bytes_total, "{listed:?}");
    assert!(listed.bytes_done < listed.bytes_total, "{listed:?}");
    assert!(listed.bytes_total >= listed.file_bytes_total, "{listed:?}");

    resume.send(()).unwrap();
    tokio::time::timeout(
        bae_util::test_channels::HANG_GUARD,
        transfers.until(|transfers| transfers.is_empty()),
    )
    .await
    .expect("a release whose files switched is listed no more")
    .unwrap();
    assert!(download_queue_now(manager).await.rows.is_empty());
}

#[tokio::test]
async fn cancelling_a_release_nothing_runs_for_is_no_error() {
    let (test, _temp_dir) = setup_test_manager().await;
    // Nothing in progress for an unknown release → no-op, no error.
    test.library.manager().cancel_release_transition(REL_NONE).await.unwrap();
}

/// Cancelling a make-local while its copies download stops it: no copy is
/// left, the release stays in the cloud, and its entry is gone.
#[tokio::test]
async fn cancelling_a_make_local_leaves_the_release_in_the_cloud() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release_id =
        uploaded_release(manager, &temp_dir.path().join("r1"), "Album One", false).await;
    manager.records.set_source_folder_name_for_test(&release_id, FOLDER).await.unwrap();
    let file = release_files(manager, &release_id).await.remove(0);
    let reference = manager.release_file_ref(&file.id).await.unwrap();
    manager.evict_file_for_test(file.file).await.unwrap();
    let coven::FilePlace::Cloud { location, .. } = reference.place() else {
        panic!("the uploaded file is stored");
    };
    let (reached, observed) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_body_at(
        coven::ObjectPath::file(
            manager.store.store_id_for_test(),
            location.uploader(),
            location.object(),
        ),
        12,
        reached,
        resumed,
    )
    .await;

    let dest = temp_dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    manager.make_releases_local(vec![release_id.clone()], &dest).await.unwrap();
    observed.await.unwrap();
    manager.cancel_release_transition(&release_id).await.unwrap();
    // The aborted download dropped the held read with it.
    drop(resume);

    assert!(download_queue_now(manager).await.rows.is_empty(), "the cancel took the entry out");
    // Coven removes what a dropped call made in the next write.
    manager.set_downloads_paused(true).await.unwrap();
    manager.set_downloads_paused(false).await.unwrap();
    assert!(
        release_files(manager, &release_id).await.iter().all(|file| file.stored),
        "cancellation leaves every file in the cloud"
    );
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 0, "no copy is left");
}

const REL_NONE: &str = "cdb5eed8-db50-45f6-814a-dc623215ae04"; // was "rel-none"

/// A release whose pin failed has a failed entry in the download queue;
/// making it local queues the make-local in the entry's place, and the
/// write that switches its files takes it out.
#[tokio::test]
async fn making_a_release_local_replaces_its_failed_pin() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_pinnable_release(&test).await;
    manager.records.set_source_folder_name_for_test(&release_id, FOLDER).await.unwrap();
    test.storage.set_online(false);
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    download_queue_until(manager, |queue| {
        matches!(
            queue.rows.first().map(|row| &row.op.state),
            Some(bae_transfers::DownloadPhase::Failed { .. })
        )
    })
    .await;
    test.storage.set_online(true);

    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    make_local_and_wait(manager, std::slice::from_ref(&release_id), &folder).await.unwrap();

    assert!(
        download_queue_now(manager).await.rows.is_empty(),
        "a release made local has no download entry"
    );
    assert!(release_files(manager, &release_id).await.iter().all(|file| !file.stored));
}

/// A make-local waits behind the entry already downloading, and is passed over
/// for a release that already has an entry waiting or running.
#[tokio::test]
async fn a_make_local_passes_over_a_release_already_queued() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let release_id = insert_pinnable_release(&test).await;
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![release_id.clone()]).await.unwrap();

    manager.make_releases_local(vec![release_id.clone()], temp_dir.path()).await.unwrap();

    let rows = download_queue_now(manager).await.rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].op.payload, bae_transfers::DownloadRequest::Pin, "the pin is still queued");
}
