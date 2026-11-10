//! Pinning a release: what a pin keeps, how its failures read, a release
//! kept here once its copies land, and the pinned count each release's
//! storage row keeps. What a pin's progress counts is in
//! `pin_progress_tests.rs`.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use super::*;
use crate::manager::tests::*;
use bae_records::DbRelease;
/// Each release whose storage row's pinned or file count differs from the
/// one probed from its files, or that has no storage row, and each whose
/// listing row is missing or holds a pinned or file count other than its
/// storage row's: none when the triggers kept up.
async fn storage_disagreements(manager: &LibraryManager) -> Vec<String> {
    manager
        .store
        .query_texts_for_test(
            "SELECT r.id || ': stored ' \
                 || COALESCE(s.pinned_count || ' of ' || s.file_count, 'nothing') \
                 || ', listed ' || COALESCE(l.pinned_count || ' of ' || l.file_count, 'nothing') \
                 || ', probed ' || probe.pinned || ' of ' || probe.files \
             FROM releases r \
             LEFT JOIN release_storage s ON s.release_id = r.id \
             LEFT JOIN release_listing l ON l.release_id = r.id \
             JOIN (SELECT r2.id AS release_id, \
                     (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r2.id) AS files, \
                     (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r2.id \
                        AND EXISTS (SELECT 1 FROM coven_pins p WHERE p.file = f.file)) AS pinned \
                   FROM releases r2) probe ON probe.release_id = r.id \
             WHERE s.release_id IS NULL OR s.pinned_count <> probe.pinned \
                OR s.file_count <> probe.files \
                OR l.release_id IS NULL OR l.pinned_count <> s.pinned_count \
                OR l.file_count <> s.file_count",
        )
        .await
        .unwrap()
}

/// The releases whose every file `coven_pins` lists, probed file by
/// file.
async fn probed_pinned_releases(manager: &LibraryManager) -> std::collections::BTreeSet<String> {
    manager
        .store
        .query_texts_for_test(
            "SELECT r.id FROM releases r \
             WHERE EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id) \
               AND NOT EXISTS (SELECT 1 FROM release_files f WHERE f.release_id = r.id \
                   AND NOT EXISTS (SELECT 1 FROM coven_pins p \
                                   WHERE p.file = f.file))",
        )
        .await
        .unwrap()
        .into_iter()
        .collect()
}

/// The pinned marker each release shows in the Storage Manager list.
async fn listed_pinned_releases(manager: &LibraryManager) -> std::collections::BTreeSet<String> {
    bae_views::LibraryViews::new(manager.store.clone())
        .get_storage_page(
            &bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::AlbumTitle,
                direction: bae_store::SortDirection::Ascending,
            },
            bae_views::StorageFilter::All,
            0,
            100,
        )
        .await
        .unwrap()
        .rows
        .into_iter()
        .filter(|row| {
            matches!(
                row.release.storage_state,
                bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true }
            )
        })
        .map(|row| row.release.id)
        .collect()
}

/// Random writes against the probe: after each, every release's pinned and
/// file counts, in its storage row and as its listing row copies them, are
/// the probed ones, and so are the list's pinned markers read from them.
#[tokio::test]
async fn the_listed_pinned_count_follows_every_pin_and_file_write() {
    use rand::{Rng, SeedableRng};
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0x5eed);
    let mut releases: Vec<HarnessRelease> = Vec::new();
    // The releases moving to the cloud to be kept here.
    let mut kept_moves: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut ran: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    let mut most_pinned = 0usize;
    for step in 0..400 {
        let pick = |rng: &mut rand::rngs::StdRng, releases: &[HarnessRelease], stage| {
            let matching: Vec<usize> =
                (0..releases.len()).filter(|&index| releases[index].stage == stage).collect();
            (!matching.is_empty()).then(|| matching[rng.random_range(0..matching.len())])
        };
        let op = match rng.random_range(0..8) {
            0 | 1 if releases.len() < 6 => {
                let files: Vec<(String, Vec<u8>)> = (0..rng.random_range(1..=3))
                    .map(|index| (format!("{index}.flac"), format!("{step}-{index}").into_bytes()))
                    .collect();
                let files: Vec<(&str, &[u8])> =
                    files.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())).collect();
                let release = insert_local_release_with_files(
                    manager,
                    &temp_dir.path().join(format!("release-{step}")),
                    &format!("Album Title {step}"),
                    &files,
                )
                .await;
                releases.push(HarnessRelease { id: release.id, stage: HarnessStage::Local });
                "add release"
            }
            2 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Local) else {
                    continue;
                };
                let pin = rng.random_bool(0.5);
                manager.upload_release(&releases[index].id, pin).await.unwrap();
                releases[index].stage = HarnessStage::Attached;
                if pin {
                    kept_moves.insert(releases[index].id.clone());
                }
                if pin {
                    "attach pinned"
                } else {
                    "attach"
                }
            }
            3 => {
                if !releases.iter().any(|release| release.stage == HarnessStage::Attached) {
                    continue;
                }
                manager.drain_uploads_expecting_work().await.unwrap();
                // coven keeps a release moved to be kept here by fetching its
                // copy after the move ends; until it has, a later step's
                // reads race that write.
                for release in std::mem::take(&mut kept_moves) {
                    wait_for_kept_release(manager, &release).await;
                }
                for release in &mut releases {
                    if release.stage == HarnessStage::Attached {
                        release.stage = HarnessStage::Remote;
                    }
                }
                "complete uploads"
            }
            4 | 5 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let refs = release_file_refs(manager, &releases[index].id).await;
                let count = rng.random_range(1..=refs.len());
                manager.store.pin(&refs[..count]).await.unwrap();
                "pin"
            }
            6 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let refs = release_file_refs(manager, &releases[index].id).await;
                let one = rng.random_range(0..refs.len());
                if rng.random_bool(0.5) {
                    manager.store.unpin(&[refs[one].id()]).await.unwrap();
                    "unpin"
                } else {
                    manager.evict_file_for_test(refs[one].id()).await.unwrap();
                    "evict"
                }
            }
            7 if !releases.is_empty() => {
                let index = rng.random_range(0..releases.len());
                if releases[index].stage == HarnessStage::Attached {
                    continue;
                }
                let release = releases.remove(index);
                manager.delete_release(&release.id).await.unwrap();
                "delete release"
            }
            _ => continue,
        };
        *ran.entry(op).or_default() += 1;

        assert_eq!(
            storage_disagreements(manager).await,
            Vec::<String>::new(),
            "after step {step}, {op}"
        );
        let probed_pinned = probed_pinned_releases(manager).await;
        most_pinned = most_pinned.max(probed_pinned.len());
        assert_eq!(
            listed_pinned_releases(manager).await,
            probed_pinned,
            "pinned markers after step {step}, {op}"
        );
    }
    for op in [
        "add release",
        "attach",
        "attach pinned",
        "complete uploads",
        "pin",
        "unpin",
        "evict",
        "delete release",
    ] {
        assert!(ran.contains_key(op), "the walk never ran {op}: {ran:?}");
    }
    assert!(most_pinned > 0, "no release was ever wholly pinned");
}

/// `release`'s storage row on `store`: its file count, total size, whether
/// it is stored, its pinned and queued counts, and its first queued place
/// against the first place coven's queue holds for its files.
async fn storage_row(store: &bae_store::Store, release: &str) -> Vec<String> {
    store
        .query_texts_for_test(&format!(
            "SELECT s.file_count || ' files, ' || s.total_size || ' bytes, stored ' || s.stored \
                 || ', ' || s.pinned_count || ' pinned, ' || s.queued_count || ' queued, ' \
                 || CASE WHEN s.first_queued_position IS (\
                        SELECT MIN(queued.position) FROM release_files f \
                        JOIN coven_upload_queue queued ON queued.file = f.file \
                        WHERE f.release_id = s.release_id) \
                    THEN 'first place as queued' ELSE 'first place not as queued' END \
             FROM release_storage s WHERE s.release_id = '{release}'"
        ))
        .await
        .unwrap()
}

/// A release another device imported and moved to the cloud arrives with its
/// storage row; pinning its file here counts the pin in that row, and this
/// device's own move of a release to the cloud counts the files it queues.
#[test]
fn a_release_s_storage_counts_its_pins_and_queued_files_on_each_device() {
    crate::test_libraries::run_two_device_test(async |devices| {
        let originals = tempfile::TempDir::new().unwrap();
        let theirs = insert_local_release_with_files(
            devices.b().manager(),
            &originals.path().join("theirs"),
            "Album Title B",
            &[("track.flac", b"track-bytes")],
        )
        .await;
        devices.b().manager().upload_release(&theirs.id, false).await.unwrap();
        let in_the_cloud =
            vec!["1 files, 11 bytes, stored 1, 0 pinned, 0 queued, first place as queued"
                .to_string()];
        devices
            .settle("the other device's release in the cloud", async |_, database| {
                storage_row(database, &theirs.id).await == in_the_cloud
            })
            .await;

        let manager = devices.a().manager();
        manager
            .pin_release_files_with_progress(
                &theirs.id,
                started_pin_entry(manager, &theirs.id).await,
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(
            storage_row(&manager.store, &theirs.id).await,
            vec!["1 files, 11 bytes, stored 1, 1 pinned, 0 queued, first place as queued"]
        );

        manager.store.set_uploads_paused(true).await.unwrap();
        let ours = insert_local_release_with_files(
            manager,
            &originals.path().join("ours"),
            "Album Title A",
            &[("one.flac", b"first audio"), ("two.flac", b"second audio")],
        )
        .await;
        manager.upload_release(&ours.id, false).await.unwrap();
        assert_eq!(
            storage_row(&manager.store, &ours.id).await,
            vec!["2 files, 23 bytes, stored 0, 0 pinned, 2 queued, first place as queued"]
        );
    });
}

// Which of a pinning release's files coven is downloading now, and how far
// each has got, as the release's read and the pin file watch tell them.

/// While a pin downloads a file, the release reads it as downloading, by the
/// file coven fetches, and the pin file watch names that file; once kept, it
/// reads as kept and downloads no more.
#[tokio::test]
async fn a_file_a_pin_downloads_reads_as_downloading() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release_id =
        uploaded_release(manager, &temp_dir.path().join("pin-files"), "Pin Files", false).await;
    let file = release_files(manager, &release_id).await.remove(0);
    manager.evict_file_for_test(file.file).await.unwrap();
    let mut files = manager.watch_pin_file_transfers();
    assert_eq!(
        files.next().await.unwrap(),
        vec![crate::FileTransferChange::Reset(Default::default())],
        "nothing downloads before the pin"
    );

    // The memory provider holds the read of the stored file until the test
    // has seen it downloading.
    let files_prefix = coven::ObjectPrefix::files(manager.store.store_id_for_test());
    let stored = home
        .request_trace()
        .into_iter()
        .rev()
        .find_map(|request| match request {
            coven::MemoryRequest::Create(path) if files_prefix.contains(&path) => Some(path),
            _ => None,
        })
        .expect("the upload created the stored file");
    let (observed, reading) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_read(stored, observed, resumed).await;
    let pin = tokio::spawn({
        let manager = test.library.manager().clone();
        let release_id = release_id.clone();
        async move {
            let entry = started_pin_entry(&manager, &release_id).await;
            manager.pin_release_files_with_progress(&release_id, entry, |_| {}).await
        }
    });
    let fetched = tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        loop {
            for change in files.next().await.unwrap() {
                if let crate::FileTransferChange::Updated { file, .. } = change {
                    return file;
                }
            }
        }
    })
    .await
    .expect("the pin file watch names the file the pin downloads");
    let pin_of = |rows: bae_views::DbStorageInspector| {
        rows.files.into_iter().find(|row| row.file.id == file.id).expect("the release's file").pin
    };
    assert_eq!(
        pin_of(inspector_rows_until(manager, &release_id, |_| true).await),
        Some(bae_views::album_detail::FilePin::Downloading { file: fetched }),
        "the file reads as downloading by the file coven fetches"
    );
    reading.await.unwrap();
    resume.send(()).unwrap();

    pin.await.unwrap().unwrap();
    assert_eq!(
        pin_of(inspector_rows_until(manager, &release_id, |_| true).await),
        Some(bae_views::album_detail::FilePin::Kept),
        "a kept file downloads no more"
    );
}

/// Each release of an album carries its own pin state, read from that
/// release's own listing row.
#[tokio::test]
async fn album_detail_releases_carry_their_own_pin_state() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud_uploading(&test).await;

    let mut album = create_test_album();
    album.title = "Two Pressings".to_string();
    manager.records.insert_album(&album).await.unwrap();

    let mut pressings = Vec::new();
    for (index, pin) in [false, true].into_iter().enumerate() {
        let release = insert_local_release_in_album(
            manager,
            &album.id,
            &temp_dir.path().join(format!("pressing-{index}")),
            &[("track.flac", b"track-bytes")],
        )
        .await;
        manager.upload_release(&release.id, pin).await.unwrap();
        wait_for_landed_upload(manager, &release.id).await;
        if pin {
            wait_for_kept_release(manager, &release.id).await;
        }
        pressings.push((release.id, pin));
    }

    let detail =
        views(manager).find_album_detail(&album.id).await.unwrap().expect("the album surfaces");
    let pinned: std::collections::HashMap<_, _> = detail
        .releases
        .iter()
        .map(|release| {
            (
                release.summary.id.clone(),
                matches!(
                    release.summary.storage_state,
                    bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true }
                ),
            )
        })
        .collect();

    assert_eq!(pinned.len(), pressings.len());
    for (release_id, pin) in pressings {
        assert_eq!(
            pinned[&release_id], pin,
            "the pin answer for {release_id} belongs to that release",
        );
    }
}

/// A pin the provider refuses — bad credentials, a missing bucket — is about
/// the cloud setup the person fixes, not an internal fault.
#[tokio::test]
async fn a_pin_the_provider_refuses_is_a_storage_setup_failure() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("pin-refused"),
        "Pin Refused",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let file = release_files(manager, &release.id).await[0].file;
    manager.evict_file_for_test(file).await.unwrap();
    // The pin's read of the stored file is refused, not whatever request
    // comes next: sync runs, and its passes make requests of their own.
    let files = coven::ObjectPrefix::files(manager.store.store_id_for_test());
    let stored = home
        .request_trace()
        .into_iter()
        .rev()
        .find_map(|request| match request {
            coven::MemoryRequest::Create(path) if files.contains(&path) => Some(path),
            _ => None,
        })
        .expect("the upload created the stored file");

    for failure in [coven::StorageFailure::Authentication, coven::StorageFailure::ContainerNotFound]
    {
        home.set_faults(coven::Faults {
            read_failures: [(stored.clone(), failure)].into(),
            ..coven::Faults::none()
        })
        .await;
        let error = manager
            .pin_release_files_with_progress(
                &release.id,
                started_pin_entry(manager, &release.id).await,
                |_| {},
            )
            .await
            .expect_err("the provider refuses the read");

        assert_eq!(
            error.category(),
            bae_diagnostics::UiErrorCategory::Storage(failure),
            "{failure:?}: {error}"
        );
    }
}

/// A pin that cannot reach the cloud is a network failure the person retries
/// once they are back online, not an internal fault: coven's typed reason
/// reaches the category the UI renders.
#[tokio::test]
async fn a_pin_the_cloud_cannot_serve_is_a_network_failure() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("pin-offline"),
        "Pin Offline",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let file = release_files(manager, &release.id).await[0].file;
    manager.evict_file_for_test(file).await.unwrap();
    home.set_online(false);

    let error = manager
        .pin_release_files_with_progress(
            &release.id,
            started_pin_entry(manager, &release.id).await,
            |_| {},
        )
        .await
        .expect_err("the pin cannot fetch the file");

    assert_eq!(
        error.category(),
        bae_diagnostics::UiErrorCategory::Storage(coven::StorageFailure::Network),
        "{error}"
    );
}

/// A release moved to the cloud to be kept here, whose move finished, and
/// whose files coven keeps here by downloading their copies: the stored files
/// can't be read, so coven can't fetch their copies until the test clears
/// the storage's faults. The manager must already be connected via
/// [`connect_test_cloud`].
async fn release_kept_here_waiting_for_its_copy(
    manager: &TestManager,
    dir: &std::path::Path,
    album_title: &str,
) -> DbRelease {
    let release = insert_local_release_with_files(
        manager.library.manager(),
        dir,
        album_title,
        &[("track.flac", b"kept-bytes")],
    )
    .await;
    manager.library.manager().upload_release(&release.id, true).await.unwrap();

    // Hold the file's upload as it is created, and from then on refuse every
    // read of the stored file, so coven's pin keeping it here can't fetch
    // its copy.
    let home = &manager.storage;
    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.library.manager().store.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.manager().clone();
        async move { manager.drain_uploads_expecting_work().await }
    });
    created.await.unwrap();
    let stored = home
        .request_trace()
        .into_iter()
        .rev()
        .find_map(|request| match request {
            coven::MemoryRequest::Create(path) => Some(path),
            _ => None,
        })
        .expect("the held upload is the last creation");
    home.set_faults(coven::Faults {
        read_failures: [(stored, coven::StorageFailure::Network)].into(),
        ..coven::Faults::none()
    })
    .await;
    resume.send(()).unwrap();
    drain.await.unwrap().unwrap();
    release
}

/// The coven file each of a release's files carries.
async fn release_file_refs(manager: &LibraryManager, release_id: &str) -> Vec<coven::FileRef> {
    let mut refs = Vec::new();
    for file in release_files(manager, release_id).await {
        refs.push(manager.release_file_ref(&file.id).await.unwrap());
    }
    refs
}

/// A release moved to the cloud to be kept here goes through no download
/// queue entry: once its move finishes, coven keeps each file whole on this
/// device itself, and the release is pinned.
#[tokio::test]
async fn a_release_kept_here_is_pinned_once_its_move_finishes() {
    let (test, temp) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let release = insert_local_release_with_files(
        manager,
        &temp.path().join("kept"),
        "Album",
        &[("audio.flac", b"audio")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();
    assert_eq!(moving_file_count(manager, &release.id).await, 1);
    assert!(download_queue_now(manager).await.rows.is_empty());
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 1);
    wait_for_kept_release(manager, &release.id).await;
    assert!(
        views(manager)
            .find_release_detail(&release.id)
            .await
            .unwrap()
            .unwrap()
            .summary
            .storage_state
            == bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true }
    );
}

/// A release kept here as it moves to the cloud is pinned only once its
/// files are whole on this device. The write switching its files to the cloud
/// asks coven to keep each, and coven downloads its copy from storage; until
/// that copy lands, the Storage Manager's list and the release's own summary
/// both read the release as not pinned, and the copy landing pins it.
#[tokio::test]
async fn a_kept_release_is_pinned_once_its_copy_lands() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release =
        release_kept_here_waiting_for_its_copy(&test, &temp_dir.path().join("kept"), "Album Title")
            .await;
    let file_id = release_files(manager, &release.id).await[0].id.clone();
    let file = manager.release_file_ref(&file_id).await.unwrap();
    assert_eq!(
        manager.store.files_pinned(&[file.id()]).await.unwrap(),
        [Some(false)],
        "the file's copy hasn't landed"
    );
    let summary_pinned = || async {
        views(manager)
            .find_release_detail(&release.id)
            .await
            .unwrap()
            .expect("the library holds the release")
            .summary
            .storage_state
            == bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true }
    };
    assert!(!summary_pinned().await, "the release's summary");
    assert!(
        !listed_pinned_releases(manager).await.contains(&release.id),
        "the Storage Manager's list"
    );

    home.set_faults(coven::Faults::none()).await;
    manager.store.pin(std::slice::from_ref(&file)).await.unwrap();
    assert!(summary_pinned().await, "the release's summary");
    assert!(
        listed_pinned_releases(manager).await.contains(&release.id),
        "the Storage Manager's list"
    );
}

/// A release whose kept copies haven't landed sorts with the releases being
/// pinned, ahead of those in the cloud only, and as pinned once they land.
#[tokio::test]
async fn a_kept_release_sorts_as_pinned_once_its_copies_land() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let cloud =
        uploaded_release(manager, &temp_dir.path().join("cloud"), "Album Title A", false).await;
    let waiting = release_kept_here_waiting_for_its_copy(
        &test,
        &temp_dir.path().join("waiting"),
        "Album Title B",
    )
    .await;
    let by_storage = || async {
        bae_views::LibraryViews::new(manager.store.clone())
            .get_storage_page(
                &bae_views::StorageSortCriterion {
                    field: bae_views::StorageSortField::Storage,
                    direction: bae_store::SortDirection::Ascending,
                },
                bae_views::StorageFilter::All,
                0,
                100,
            )
            .await
            .unwrap()
            .rows
            .into_iter()
            .map(|row| (row.release.id, row.status))
            .collect::<Vec<_>>()
    };
    use bae_views::album_detail::StorageRowStatus;
    assert_eq!(
        by_storage().await,
        [
            (waiting.id.clone(), StorageRowStatus::CopyingHere),
            (cloud.clone(), StorageRowStatus::InStorage),
        ]
    );

    home.set_faults(coven::Faults::none()).await;
    let refs = release_file_refs(manager, &waiting.id).await;
    manager.store.pin(&refs).await.unwrap();
    assert_eq!(
        by_storage().await,
        [
            (waiting.id.clone(), StorageRowStatus::Pinned),
            (cloud.clone(), StorageRowStatus::InStorage),
        ]
    );
    inspector_rows_until(manager, &waiting.id, |rows| {
        rows.files.iter().all(|file| file.pin == Some(bae_views::album_detail::FilePin::Kept))
    })
    .await;
}

/// A release moved to the cloud to be kept here reads Copying here while
/// coven's request to keep its files is open, and Stop keeping here, the
/// release's unpin, ends the request in its write, so coven does not pin the
/// file when its copy can be fetched.
#[tokio::test]
async fn stop_keeping_here_ends_the_request_a_release_reads_copying_here_for() {
    use bae_views::album_detail::StorageRowStatus;
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release =
        release_kept_here_waiting_for_its_copy(&test, &temp_dir.path().join("kept"), "Album Title")
            .await;
    let requests = || async {
        manager.store.query_texts_for_test("SELECT file FROM coven_pin_requests").await.unwrap()
    };
    let row_status = || async {
        bae_views::LibraryViews::new(manager.store.clone())
            .get_storage_page(
                &bae_views::StorageSortCriterion {
                    field: bae_views::StorageSortField::Storage,
                    direction: bae_store::SortDirection::Ascending,
                },
                bae_views::StorageFilter::All,
                0,
                100,
            )
            .await
            .unwrap()
            .rows
            .into_iter()
            .find(|row| row.release.id == release.id)
            .map(|row| row.status)
    };

    assert_eq!(requests().await.len(), 1, "coven's request to keep the file is open");
    assert_eq!(row_status().await, Some(StorageRowStatus::CopyingHere));
    let summary = views(manager).find_release_detail(&release.id).await.unwrap().unwrap().summary;
    assert!(summary.keeping_here);
    assert_eq!(
        summary.storage_actions,
        [ReleaseStorageAction::Unpin, ReleaseStorageAction::DetachAudio]
    );

    manager.unpin_releases(std::slice::from_ref(&release.id)).await.unwrap();

    assert!(requests().await.is_empty(), "the unpin ends coven's request");
    assert_eq!(row_status().await, Some(StorageRowStatus::InStorage));
    let detail = views(manager).find_release_detail(&release.id).await.unwrap().unwrap();
    assert!(!detail.summary.keeping_here);
    assert_eq!(
        detail.summary.storage_actions,
        [ReleaseStorageAction::Pin, ReleaseStorageAction::DetachAudio]
    );

    // Coven could now fetch the copy, and has no request to make a pin for.
    home.set_faults(coven::Faults::none()).await;
    let refs = release_file_refs(manager, &release.id).await;
    assert_eq!(
        manager.store.files_pinned(&[refs[0].id()]).await.unwrap(),
        [Some(false)],
        "the file stays unpinned"
    );
}
