//! Pinning a release: what a pin counts and keeps, how its progress and
//! failures read, a release kept here once its copies land, and the pinned
//! count the listing keeps.

use super::*;
use crate::manager::tests::*;
use bae_db::DbFilePin;
use bae_db::DbRelease;
use std::collections::BTreeMap;

fn file(n: u128) -> coven::FileId {
    coven::FileId(uuid::Uuid::from_u128(n))
}

fn pin(kept: bool, downloading: Option<coven::FileId>) -> DbFilePin {
    DbFilePin { kept, downloading }
}

/// A kept file counts its whole size, as does a file whose download
/// ended and waits for the pin's write; a file downloading counts the
/// share of its size its object has moved, and a file waiting, or
/// downloading but not yet told, counts none.
#[test]
fn a_pin_holds_its_kept_and_fetched_files_and_each_downloads_share() {
    let sizes: BTreeMap<String, (coven::FileId, u64)> = [
        ("kept".to_string(), (file(10), 1_000)),
        ("fetched".to_string(), (file(11), 2_000)),
        ("downloading".to_string(), (file(1), 400)),
        ("untold".to_string(), (file(2), 300)),
        ("waiting".to_string(), (file(12), 200)),
    ]
    .into_iter()
    .collect();
    let mut count = PinCount::default();
    assert_eq!(count.bytes(&sizes), None, "nothing is counted before the pins are read");
    count.read(
        [
            ("kept".to_string(), pin(true, None)),
            ("fetched".to_string(), pin(false, None)),
            ("downloading".to_string(), pin(false, Some(file(1)))),
            ("untold".to_string(), pin(false, Some(file(2)))),
            ("waiting".to_string(), pin(false, None)),
        ]
        .into_iter()
        .collect(),
    );
    count.heard(vec![
        coven::TransferChange::Updated {
            file: file(1),
            progress: coven::TransferProgress { bytes_transferred: 110, bytes_total: 440 },
        },
        coven::TransferChange::Updated {
            file: file(11),
            progress: coven::TransferProgress { bytes_transferred: 1, bytes_total: 2 },
        },
    ]);
    count.heard(vec![coven::TransferChange::Removed { file: file(11) }]);

    assert_eq!(count.bytes(&sizes), Some(1_000 + 2_000 + 100));
}

/// A file whose download the pins say ended before the watch does keeps
/// counting: its bytes never fall back while the watch catches up.
#[test]
fn a_download_the_pins_end_before_the_watch_keeps_its_bytes() {
    let sizes: BTreeMap<String, (coven::FileId, u64)> =
        [("a".to_string(), (file(1), 400)), ("b".to_string(), (file(2), 400))]
            .into_iter()
            .collect();
    let mut count = PinCount::default();
    count.read(
        [
            ("a".to_string(), pin(false, Some(file(1)))),
            ("b".to_string(), pin(false, Some(file(2)))),
        ]
        .into_iter()
        .collect(),
    );
    count.heard(vec![coven::TransferChange::Updated {
        file: file(1),
        progress: coven::TransferProgress { bytes_transferred: 440, bytes_total: 440 },
    }]);
    assert_eq!(count.bytes(&sizes), Some(400));

    // The download of `a` ended; `b` still downloads, so neither is kept.
    count.read(
        [("a".to_string(), pin(false, None)), ("b".to_string(), pin(false, Some(file(2))))]
            .into_iter()
            .collect(),
    );
    assert_eq!(count.bytes(&sizes), Some(400));
    // Then the watch says so too.
    assert!(count.heard(vec![coven::TransferChange::Removed { file: file(1) }]));
    assert_eq!(count.bytes(&sizes), Some(400));
}

/// Each release whose listed pinned or file count differs from the one probed
/// from its files, or that has no listing row: none when the triggers kept
/// up.
async fn listing_disagreements(manager: &LibraryManager) -> Vec<String> {
    manager
        .database
        .query_texts_for_test(
            "SELECT r.id || ': listed ' \
                 || COALESCE(l.pinned_count || ' of ' || l.file_count, 'nothing') \
                 || ', probed ' || probe.pinned || ' of ' || probe.files \
             FROM releases r \
             LEFT JOIN release_listing l ON l.release_id = r.id \
             JOIN (SELECT r2.id AS release_id, \
                     (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r2.id) AS files, \
                     (SELECT COUNT(*) FROM release_files f WHERE f.release_id = r2.id \
                        AND EXISTS (SELECT 1 FROM coven_pins p WHERE p.file = f.file)) AS pinned \
                   FROM releases r2) probe ON probe.release_id = r.id \
             WHERE l.release_id IS NULL OR l.pinned_count <> probe.pinned \
                OR l.file_count <> probe.files",
        )
        .await
        .unwrap()
}

/// The releases whose every file `coven_pins` lists, probed file by
/// file.
async fn probed_pinned_releases(manager: &LibraryManager) -> std::collections::BTreeSet<String> {
    manager
        .database
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
    bae_views::LibraryViews::new(manager.database.clone())
        .get_storage_page(
            &bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::AlbumTitle,
                direction: bae_db::SortDirection::Ascending,
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

/// Random writes against the probe: after each, every release's listed pinned
/// and file counts are the probed ones, and so are the list's pinned markers
/// read from them.
#[tokio::test]
async fn the_listed_pinned_count_follows_every_pin_and_file_write() {
    use rand::{Rng, SeedableRng};
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
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
        let op = match rng.random_range(0..10) {
            0 | 1 if releases.len() < 6 => {
                let files: Vec<(String, Vec<u8>)> = (0..rng.random_range(1..=3))
                    .map(|index| (format!("{index}.flac"), format!("{step}-{index}").into_bytes()))
                    .collect();
                let files: Vec<(&str, &[u8])> =
                    files.iter().map(|(name, bytes)| (name.as_str(), bytes.as_slice())).collect();
                let release = insert_local_release_with_files(
                    &manager,
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
                    wait_for_kept_release(&manager, &release).await;
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
                let refs = release_file_refs(&manager, &releases[index].id).await;
                let count = rng.random_range(1..=refs.len());
                manager.database.pin(&refs[..count]).await.unwrap();
                "pin"
            }
            6 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let refs = release_file_refs(&manager, &releases[index].id).await;
                let one = rng.random_range(0..refs.len());
                if rng.random_bool(0.5) {
                    manager.database.unpin(&[refs[one].id()]).await.unwrap();
                    "unpin"
                } else {
                    manager.evict_file_for_test(refs[one].id()).await.unwrap();
                    "evict"
                }
            }
            7 => {
                let Some(from) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let Some(to) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let files = release_files(&manager, &releases[from].id).await;
                if from == to || files.len() < 2 {
                    continue;
                }
                let file = &files[rng.random_range(0..files.len())];
                manager
                    .database
                    .execute_for_test(
                        "UPDATE release_files SET release_id = ?1 WHERE id = ?2",
                        vec![releases[to].id.clone(), file.id.clone()],
                    )
                    .await
                    .unwrap();
                "move file"
            }
            8 => {
                let Some(index) = pick(&mut rng, &releases, HarnessStage::Remote) else {
                    continue;
                };
                let files = release_files(&manager, &releases[index].id).await;
                if files.len() < 2 {
                    continue;
                }
                let file = &files[rng.random_range(0..files.len())];
                manager
                    .database
                    .execute_for_test(
                        "DELETE FROM release_files WHERE id = ?1",
                        vec![file.id.clone()],
                    )
                    .await
                    .unwrap();
                "delete file"
            }
            9 if !releases.is_empty() => {
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
            listing_disagreements(&manager).await,
            Vec::<String>::new(),
            "after step {step}, {op}"
        );
        let probed_pinned = probed_pinned_releases(&manager).await;
        most_pinned = most_pinned.max(probed_pinned.len());
        assert_eq!(
            listed_pinned_releases(&manager).await,
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
        "move file",
        "delete file",
        "delete release",
    ] {
        assert!(ran.contains_key(op), "the walk never ran {op}: {ran:?}");
    }
    assert!(most_pinned > 0, "no release was ever wholly pinned");
}

// Which of a pinning release's files coven is downloading now, and how far
// each has got, as the release's read and the pin file watch tell them.

/// While a pin downloads a file, the release reads it as downloading, by the
/// file coven fetches, and the pin file watch names that file; once kept, it
/// reads as kept and downloads no more.
#[tokio::test]
async fn a_file_a_pin_downloads_reads_as_downloading() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release_id =
        uploaded_release(&manager, &temp_dir.path().join("pin-files"), "Pin Files", false).await;
    let file = release_files(&manager, &release_id).await.remove(0);
    manager.evict_file_for_test(file.file).await.unwrap();
    let mut files = manager.watch_pin_file_transfers();
    assert_eq!(
        files.next().await.unwrap(),
        vec![crate::FileTransferChange::Reset(Default::default())],
        "nothing downloads before the pin"
    );

    // The memory provider holds each request's answer.
    home.set_faults(coven::Faults {
        delay: std::time::Duration::from_secs(2),
        ..coven::Faults::none()
    })
    .await;
    let pin = tokio::spawn({
        let manager = manager.library.clone();
        let release_id = release_id.clone();
        async move { manager.pin_release_files_with_progress(&release_id, |_| {}).await }
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
        pin_of(inspector_rows_until(&manager, &release_id, |_| true).await),
        Some(bae_views::album_detail::FilePin::Downloading { file: fetched }),
        "the file reads as downloading by the file coven fetches"
    );

    pin.await.unwrap().unwrap();
    assert_eq!(
        pin_of(inspector_rows_until(&manager, &release_id, |_| true).await),
        Some(bae_views::album_detail::FilePin::Kept),
        "a kept file downloads no more"
    );
}

/// A pin's bytes never fall back as a file lands, even while its pins query
/// lags behind coven's download watch: the file counts its share while it
/// downloads and its whole size once kept, never nothing in between.
#[tokio::test]
async fn a_pins_bytes_never_fall_back_as_a_file_lands() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release_id =
        uploaded_release(&manager, &temp_dir.path().join("pin-forward"), "Pin Forward", false)
            .await;
    let file = release_files(&manager, &release_id).await.remove(0);
    let size = u64::try_from(file.file_size).unwrap();
    let file = manager.release_file_ref(&file.id).await.unwrap();
    manager.evict_file_for_test(file.id()).await.unwrap();
    let coven::FilePlace::Cloud { location, .. } = file.place() else {
        panic!("the uploaded file is stored");
    };

    // Hold the download one byte into the file's encrypted chunk, past its
    // 11-byte header.
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
    let (bytes_tx, mut bytes) = tokio::sync::mpsc::unbounded_channel();
    let pin = tokio::spawn({
        let manager = manager.library.clone();
        let release_id = release_id.clone();
        async move {
            manager
                .pin_release_files_with_progress(&release_id, move |progress| {
                    bytes_tx.send(progress.bytes_done).unwrap();
                })
                .await
        }
    });
    observed.await.unwrap();
    let mut seen = Vec::new();
    while seen.last().is_none_or(|done| *done == 0) {
        seen.push(bytes.recv().await.expect("the pin counts the held download's share"));
    }

    // From here the pins query runs no more until the pin ends, so it still
    // says the file downloads after coven has the whole copy and ended the
    // download.
    let reads = manager.pin_reads_hold.hold();
    resume.send(()).unwrap();
    pin.await.unwrap().unwrap();
    drop(reads);
    while let Ok(done) = bytes.try_recv() {
        seen.push(done);
    }
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "the bytes never fall back: {seen:?}");
    assert_eq!(seen.last(), Some(&size), "the pin ends holding the file");
}

/// Each release of an album carries its own pin state, read from that
/// release's own listing row.
#[tokio::test]
async fn album_detail_releases_carry_their_own_pin_state() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud_uploading(&manager).await;

    let mut album = create_test_album();
    album.title = "Two Pressings".to_string();
    manager.database.insert_album(&album).await.unwrap();

    let mut pressings = Vec::new();
    for (index, pin) in [false, true].into_iter().enumerate() {
        let release = insert_local_release_in_album(
            &manager,
            &album.id,
            &temp_dir.path().join(format!("pressing-{index}")),
            &[("track.flac", b"track-bytes")],
        )
        .await;
        manager.upload_release(&release.id, pin).await.unwrap();
        wait_for_landed_upload(&manager, &release.id).await;
        if pin {
            wait_for_kept_release(&manager, &release.id).await;
        }
        pressings.push((release.id, pin));
    }

    let detail =
        views(&manager).find_album_detail(&album.id).await.unwrap().expect("the album surfaces");
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

/// Pin progress counts only the release's files; cover art keeps its own budget.
#[tokio::test]
async fn pin_release_counts_audio_and_keeps_cover_evictable() {
    let (manager, temp_dir) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let file_bytes: &[u8] = b"track-bytes";
    let cover_bytes: &[u8] = b"cover-bytes-are-longer";
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("unpinned-with-cover"),
        "Unpinned With Cover",
        &[("track.flac", file_bytes)],
    )
    .await;
    store_test_cover(&manager, &release.id, cover_bytes).await;

    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    assert!(
        !file_pinned(&manager, bae_db::synced::COVERS_NAMESPACE, &release.id).await,
        "upload without pin leaves the cover evictable"
    );

    let expected_total = file_bytes.len() as u64;
    assert_eq!(
        manager.initial_download_progress(&release.id).await.unwrap().bytes_total,
        expected_total,
        "the pane's denominator counts every file the pin fetches"
    );

    let mut progress = Vec::new();
    manager
        .pin_release_files_with_progress(&release.id, |update| progress.push(update))
        .await
        .unwrap();

    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    assert!(
        file_pinned(&manager, bae_db::synced::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file pinned"
    );
    assert!(
        !file_pinned(&manager, bae_db::synced::COVERS_NAMESPACE, &release.id).await,
        "the cover remains evictable"
    );
    let last = progress.last().expect("the pin reports progress");
    assert_eq!(last.bytes_done, expected_total);
    assert_eq!(last.bytes_total, expected_total);
    assert_eq!(last.fraction, 1.0, "the bar lands on its denominator");
}

/// A pin the provider refuses — bad credentials, a missing bucket — is about
/// the cloud setup the person fixes, not an internal fault.
#[tokio::test]
async fn a_pin_the_provider_refuses_is_a_cloud_setup_failure() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pin-refused"),
        "Pin Refused",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let file = release_files(&manager, &release.id).await[0].file;
    manager.evict_file_for_test(file).await.unwrap();
    // The pin's read of the stored file is refused, not whatever request
    // comes next: sync runs, and its passes make requests of their own.
    let files = coven::ObjectPrefix::files(manager.database.store_id_for_test());
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
            .pin_release_files_with_progress(&release.id, |_| {})
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
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pin-offline"),
        "Pin Offline",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let file = release_files(&manager, &release.id).await[0].file;
    manager.evict_file_for_test(file).await.unwrap();
    home.set_online(false);

    let error = manager
        .pin_release_files_with_progress(&release.id, |_| {})
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
        manager,
        dir,
        album_title,
        &[("track.flac", b"kept-bytes")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();

    // Hold the file's upload as it is created, and from then on refuse every
    // read of the stored file, so coven's pin keeping it here can't fetch
    // its copy.
    let home = &manager.storage;
    let (started, created) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_next_creation(
        coven::ObjectPrefix::files(manager.database.store_id_for_test()),
        started,
        resumed,
    )
    .await;
    let drain = tokio::spawn({
        let manager = manager.library.clone();
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
    let (manager, temp) = setup_test_manager().await;
    connect_test_cloud(&manager).await;
    let release = insert_local_release_with_files(
        &manager,
        &temp.path().join("kept"),
        "Album",
        &[("audio.flac", b"audio")],
    )
    .await;
    manager.upload_release(&release.id, true).await.unwrap();
    assert_eq!(moving_file_count(&manager, &release.id).await, 1);
    assert!(download_queue_now(&manager).await.rows.is_empty());
    assert_eq!(manager.drain_uploads_expecting_work().await.unwrap(), 1);
    wait_for_kept_release(&manager, &release.id).await;
    assert!(
        views(&manager)
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
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release = release_kept_here_waiting_for_its_copy(
        &manager,
        &temp_dir.path().join("kept"),
        "Album Title",
    )
    .await;
    let file_id = release_files(&manager, &release.id).await[0].id.clone();
    let file = manager.release_file_ref(&file_id).await.unwrap();
    assert_eq!(
        manager.database.files_pinned(&[file.id()]).await.unwrap(),
        [Some(false)],
        "the file's copy hasn't landed"
    );
    let summary_pinned = || async {
        views(&manager)
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
        !listed_pinned_releases(&manager).await.contains(&release.id),
        "the Storage Manager's list"
    );

    home.set_faults(coven::Faults::none()).await;
    manager.database.pin(std::slice::from_ref(&file)).await.unwrap();
    assert!(summary_pinned().await, "the release's summary");
    assert!(
        listed_pinned_releases(&manager).await.contains(&release.id),
        "the Storage Manager's list"
    );
}

/// A release whose kept copies haven't landed sorts with the releases in the
/// cloud only, and as pinned once they land.
#[tokio::test]
async fn a_kept_release_sorts_as_pinned_once_its_copies_land() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let cloud =
        uploaded_release(&manager, &temp_dir.path().join("cloud"), "Album Title A", false).await;
    let waiting = release_kept_here_waiting_for_its_copy(
        &manager,
        &temp_dir.path().join("waiting"),
        "Album Title B",
    )
    .await;
    let by_storage = || async {
        bae_views::LibraryViews::new(manager.database.clone())
            .get_storage_page(
                &bae_views::StorageSortCriterion {
                    field: bae_views::StorageSortField::Storage,
                    direction: bae_db::SortDirection::Ascending,
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
        [(cloud.clone(), StorageRowStatus::Cloud), (waiting.id.clone(), StorageRowStatus::Cloud),]
    );

    home.set_faults(coven::Faults::none()).await;
    let refs = release_file_refs(&manager, &waiting.id).await;
    manager.database.pin(&refs).await.unwrap();
    assert_eq!(
        by_storage().await,
        [(waiting.id.clone(), StorageRowStatus::Pinned), (cloud.clone(), StorageRowStatus::Cloud),]
    );
    inspector_rows_until(&manager, &waiting.id, |rows| {
        rows.files.iter().all(|file| file.pin == Some(bae_views::album_detail::FilePin::Kept))
    })
    .await;
}
