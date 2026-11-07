//! What a pin's progress counts: each file kept or fetched whole, the share
//! of each download in flight as coven's download watch tells it, and never
//! a file whose download failed.

use super::*;
use crate::manager::tests::*;
use std::collections::BTreeMap;

fn file(n: u128) -> coven::FileId {
    coven::FileId(uuid::Uuid::from_u128(n))
}

/// Whether each named file is kept, as a read of the release's files says.
fn kept(files: &[(&str, bool)]) -> BTreeMap<String, bool> {
    files.iter().map(|(id, kept)| (id.to_string(), *kept)).collect()
}

/// A kept file counts its whole size, as does a file whose download
/// finished and waits for the pin's write; a file downloading counts the
/// share of its size its object has moved, and a file the watch hasn't
/// told of counts none.
#[test]
fn a_pin_holds_its_kept_and_fetched_files_and_each_downloads_share() {
    let sizes: BTreeMap<String, (coven::FileId, u64)> = [
        ("kept".to_string(), (file(10), 1_000)),
        ("fetched".to_string(), (file(11), 2_000)),
        ("downloading".to_string(), (file(1), 400)),
        ("waiting".to_string(), (file(12), 200)),
    ]
    .into_iter()
    .collect();
    let mut count = PinCount::default();
    assert_eq!(count.bytes(&sizes), None, "nothing is counted before the kept files are read");
    count.read(kept(&[
        ("kept", true),
        ("fetched", false),
        ("downloading", false),
        ("waiting", false),
    ]));
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
    count.heard(vec![coven::TransferChange::Removed {
        file: file(11),
        end: coven::TransferEnd::Finished,
    }]);

    assert_eq!(count.bytes(&sizes), Some(1_000 + 2_000 + 100));
}

/// A file counts the share its download moved until the watch says how the
/// download ended, whatever reads of the kept files say meanwhile, and its
/// whole size once it finished: its bytes never fall back while the write
/// keeping it lags.
#[test]
fn a_download_counts_its_share_until_the_watch_says_it_finished() {
    let sizes: BTreeMap<String, (coven::FileId, u64)> =
        [("a".to_string(), (file(1), 400))].into_iter().collect();
    let mut count = PinCount::default();
    count.read(kept(&[("a", false)]));
    count.heard(vec![coven::TransferChange::Updated {
        file: file(1),
        progress: coven::TransferProgress { bytes_transferred: 330, bytes_total: 440 },
    }]);
    assert_eq!(count.bytes(&sizes), Some(300));

    count.read(kept(&[("a", false)]));
    assert_eq!(count.bytes(&sizes), Some(300), "a read that doesn't keep the file yet");
    count.heard(vec![coven::TransferChange::Removed {
        file: file(1),
        end: coven::TransferEnd::Finished,
    }]);
    assert_eq!(count.bytes(&sizes), Some(400), "the watch says the download finished");
    count.read(kept(&[("a", false)]));
    assert_eq!(count.bytes(&sizes), Some(400), "a read before the pin's write");
}

/// A download that failed, was held or was dropped brought none of its file
/// whole, so the pin counts none of it once the watch says how it ended.
#[test]
fn a_download_that_ended_unfinished_counts_none_of_its_file() {
    let sizes: BTreeMap<String, (coven::FileId, u64)> =
        [("a".to_string(), (file(1), 400))].into_iter().collect();
    for end in [coven::TransferEnd::Failed, coven::TransferEnd::Held, coven::TransferEnd::Dropped] {
        let mut count = PinCount::default();
        count.read(kept(&[("a", false)]));
        count.heard(vec![coven::TransferChange::Updated {
            file: file(1),
            progress: coven::TransferProgress { bytes_transferred: 220, bytes_total: 440 },
        }]);
        assert_eq!(count.bytes(&sizes), Some(200));

        count.heard(vec![coven::TransferChange::Removed { file: file(1), end }]);
        assert_eq!(count.bytes(&sizes), Some(0), "a download that ended {end:?}");
    }
}

/// A pin's bytes never fall back as a file lands, even while its query of
/// kept files lags behind coven's download watch: the file counts its share
/// while it downloads and its whole size once kept, never nothing in between.
#[tokio::test]
async fn a_pins_bytes_never_fall_back_as_a_file_lands() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let release_id =
        uploaded_release(manager, &temp_dir.path().join("pin-forward"), "Pin Forward", false).await;
    let file = release_files(manager, &release_id).await.remove(0);
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
            manager.store.store_id_for_test(),
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
        let manager = test.library.manager().clone();
        let release_id = release_id.clone();
        async move {
            let entry = started_pin_entry(&manager, &release_id).await;
            manager
                .pin_release_files_with_progress(&release_id, entry, move |progress| {
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

    // From here the query of kept files runs no more until the pin ends, so
    // it still says the file isn't kept after coven has the whole copy and
    // ended the download.
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

/// What coven's download watch told a test of a pin's two files: whether
/// the first finished, and how far the second moved and how it ended.
struct HeardDownloads {
    whole: coven::FileId,
    failing: coven::FileId,
    whole_finished: bool,
    failing_moved: Option<coven::TransferProgress>,
    failing_ended: Option<coven::TransferEnd>,
}

impl HeardDownloads {
    fn take(&mut self, changes: Vec<coven::TransferChange>) {
        for change in changes {
            match change {
                coven::TransferChange::Reset(all) => {
                    self.failing_moved = all.get(&self.failing).copied().or(self.failing_moved);
                }
                coven::TransferChange::Updated { file, progress } if file == self.failing => {
                    self.failing_moved = Some(progress);
                }
                coven::TransferChange::Removed { file, end } if file == self.whole => {
                    assert_eq!(end, coven::TransferEnd::Finished, "the first file's download");
                    self.whole_finished = true;
                }
                coven::TransferChange::Removed { file, end } if file == self.failing => {
                    self.failing_ended = Some(end);
                }
                _ => {}
            }
        }
    }
}

/// A pin whose second file's download fails never counts that file's bytes
/// beyond the share its download moved: from the first file whole and the
/// second held partway, no progress until the pin's error goes higher.
#[tokio::test]
async fn a_failed_files_bytes_never_count() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let home = connect_test_cloud(&test).await;
    let whole_bytes = vec![1u8; 40_000];
    let failing_bytes = vec![2u8; 200_000];
    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("pin-failing"),
        "Pin Failing",
        &[("1.flac", whole_bytes.as_slice()), ("2.flac", failing_bytes.as_slice())],
    )
    .await;
    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    let mut files = Vec::new();
    for file in release_files(manager, &release.id).await {
        let size = u64::try_from(file.file_size).unwrap();
        let file = manager.release_file_ref(&file.id).await.unwrap();
        manager.evict_file_for_test(file.id()).await.unwrap();
        files.push((file, size));
    }
    files.sort_by_key(|(_, size)| *size);
    let [(whole, whole_size), (failing, failing_size)] = files.try_into().unwrap();
    let coven::FilePlace::Cloud { location, .. } = failing.place() else {
        panic!("the uploaded file is stored");
    };
    let failing_object = coven::ObjectPath::file(
        manager.store.store_id_for_test(),
        location.uploader(),
        location.object(),
    );

    // The failing file's download stops partway, and once let go its bytes
    // stop coming.
    let at = 100_000;
    home.set_faults(coven::Faults {
        body_failures: [(failing_object.clone(), coven::BodyFailure::Interrupted(at + 1_000))]
            .into(),
        ..coven::Faults::none()
    })
    .await;
    let (reached, held) = tokio::sync::oneshot::channel();
    let (resume, resumed) = tokio::sync::oneshot::channel();
    home.hold_body_at(failing_object, at, reached, resumed).await;
    let mut downloads = manager.store.subscribe_download_file_transfers();
    let (bytes_tx, mut bytes) = tokio::sync::mpsc::unbounded_channel();
    let pin = tokio::spawn({
        let manager = test.library.manager().clone();
        let release_id = release.id.clone();
        async move {
            let entry = started_pin_entry(&manager, &release_id).await;
            manager
                .pin_release_files_with_progress(&release_id, entry, move |progress| {
                    bytes_tx.send(progress.bytes_done).unwrap();
                })
                .await
        }
    });
    held.await.unwrap();

    // The first file's download finishes while the failing one is held.
    let share = |moved: Option<coven::TransferProgress>| {
        moved.map_or(0, |moved| {
            (failing_size as f64 * crate::file_transfers::fraction(moved)) as u64
        })
    };
    let mut heard = HeardDownloads {
        whole: whole.id(),
        failing: failing.id(),
        whole_finished: false,
        failing_moved: None,
        failing_ended: None,
    };
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while !heard.whole_finished
            || heard.failing_moved.is_none_or(|moved| moved.bytes_transferred < at)
        {
            heard.take(downloads.next().await.unwrap());
        }
    })
    .await
    .expect("the first file finishes and the failing one reaches its hold");
    // The pin counts the first file whole and the failing one's share.
    let at_hold = whole_size + share(heard.failing_moved);
    let mut seen = Vec::new();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while seen.last() != Some(&at_hold) {
            seen.push(bytes.recv().await.expect("the pin counts until it ends"));
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the pin counts {at_hold} bytes at the hold: {seen:?}"));

    resume.send(()).unwrap();
    tokio::time::timeout(bae_util::test_channels::HANG_GUARD, async {
        while heard.failing_ended.is_none() {
            heard.take(downloads.next().await.unwrap());
        }
    })
    .await
    .expect("the failing file's download ends");
    assert_eq!(heard.failing_ended, Some(coven::TransferEnd::Failed));
    pin.await.unwrap().expect_err("the failing file's download fails the pin");
    while let Ok(done) = bytes.try_recv() {
        seen.push(done);
    }
    let most = whole_size + share(heard.failing_moved);
    assert!(
        seen.iter().all(|done| *done <= most),
        "no progress counts more than {most} bytes: {seen:?}"
    );
}

/// Pin progress counts only the release's files; cover art keeps its own budget.
#[tokio::test]
async fn pin_release_counts_audio_and_keeps_cover_evictable() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    connect_test_cloud(&test).await;
    let file_bytes: &[u8] = b"track-bytes";
    let cover_bytes: &[u8] = b"cover-bytes-are-longer";
    let release = insert_local_release_with_files(
        manager,
        &temp_dir.path().join("unpinned-with-cover"),
        "Unpinned With Cover",
        &[("track.flac", file_bytes)],
    )
    .await;
    store_test_cover(manager, &release.id, cover_bytes).await;

    manager.upload_release(&release.id, false).await.unwrap();
    manager.drain_uploads_expecting_work().await.unwrap();
    assert!(
        !file_pinned(manager, bae_records::synced::COVERS_NAMESPACE, &release.id).await,
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
        .pin_release_files_with_progress(
            &release.id,
            started_pin_entry(manager, &release.id).await,
            |update| progress.push(update),
        )
        .await
        .unwrap();

    let file_id = release_files(manager, &release.id).await[0].id.clone();
    assert!(
        file_pinned(manager, bae_records::synced::RELEASE_FILES_NAMESPACE, &file_id).await,
        "the release file pinned"
    );
    assert!(
        !file_pinned(manager, bae_records::synced::COVERS_NAMESPACE, &release.id).await,
        "the cover remains evictable"
    );
    let last = progress.last().expect("the pin reports progress");
    assert_eq!(last.bytes_done, expected_total);
    assert_eq!(last.bytes_total, expected_total);
    assert_eq!(last.fraction, 1.0, "the bar lands on its denominator");
}
