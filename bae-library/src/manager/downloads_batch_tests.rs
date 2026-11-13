//! The Activity panel's Pinning and Moving to This Mac lines as the download
//! queue's pins and make-locals run: each line counts the releases of its own
//! batch, done of all it has held since its kind last had none waiting.

use super::*;
use crate::manager::tests::*;

fn batch(done: u32, total: u32) -> bae_views::TransferBatch {
    bae_views::TransferBatch { done, total }
}

/// The activity as one read finds it now.
async fn activity_now(manager: &LibraryManager) -> bae_views::SyncActivity {
    crate::followed::value_matching(&mut views(manager).subscribe_sync_activity(), |_| true).await
}

/// The Pinning line's batch now; `None` while it shows no line.
async fn pins_now(manager: &LibraryManager) -> Option<bae_views::TransferBatch> {
    activity_now(manager).await.pins.map(|pins| pins.batch)
}

/// `count` releases stored in the cloud, none of their files kept here, each
/// with bytes of its own.
async fn pinnable_releases(test: &TestManager, count: usize) -> Vec<String> {
    let manager = test.library.manager();
    connect_test_cloud(test).await;
    let mut releases = Vec::with_capacity(count);
    for index in 0..count {
        let bytes = format!("release-{index}-bytes").into_bytes();
        let release = uploaded_release_with_files(
            manager,
            &manager.library_path().join(format!("pin-source-{index}")),
            &format!("Album Title {index}"),
            &[("track.flac", &bytes)],
            false,
        )
        .await;
        for file in release_files(manager, &release).await {
            manager.evict_file_for_test(file.file).await.unwrap();
        }
        releases.push(release);
    }
    releases
}

/// Queue `releases` to pin with the queue paused, so the worker leaves them
/// to the test.
async fn queue_pins(manager: &LibraryManager, releases: &[String]) {
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(releases.to_vec()).await.unwrap();
}

/// Pin `release` as the download worker does: its entry started, and ended
/// in the write that pins every file. The queue is paused again after, as
/// the test found it.
async fn pin(manager: &LibraryManager, release: &str) {
    let entry = started_pin_entry(manager, release).await;
    manager.pin_release_files_with_progress(release, entry, |_| {}).await.unwrap();
    manager.set_downloads_paused(true).await.unwrap();
}

/// The Pinning line counts the releases its batch has held, a release done
/// once it is pinned whole and out of the queue: one of three pinned, one
/// more queued, the batch's last release pinned ending the line, and the
/// next pin starting a new batch from zero.
#[tokio::test]
async fn a_line_counts_the_releases_done_since_its_queue_was_last_empty() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let releases = pinnable_releases(&test, 5).await;

    queue_pins(manager, &releases[0..3]).await;
    assert_eq!(pins_now(manager).await, Some(batch(0, 3)));
    pin(manager, &releases[0]).await;
    assert_eq!(pins_now(manager).await, Some(batch(1, 3)));
    queue_pins(manager, &releases[3..4]).await;
    assert_eq!(pins_now(manager).await, Some(batch(1, 4)), "the batch only grows");

    for release in &releases[1..3] {
        pin(manager, release).await;
    }
    assert_eq!(pins_now(manager).await, Some(batch(3, 4)));
    pin(manager, &releases[3]).await;
    assert_eq!(pins_now(manager).await, None, "every release of the batch is done");

    queue_pins(manager, &releases[4..5]).await;
    assert_eq!(pins_now(manager).await, Some(batch(0, 1)), "a new batch starts from zero");
}

/// A pin cancelled before it finished stays in its batch's count and never
/// counts done, while the batch lasts.
#[tokio::test]
async fn a_cancelled_pin_stays_in_the_count_and_never_counts_done() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let releases = pinnable_releases(&test, 3).await;

    queue_pins(manager, &releases).await;
    manager.cancel_download(&releases[0]).await.unwrap();
    assert_eq!(pins_now(manager).await, Some(batch(0, 3)));
    pin(manager, &releases[1]).await;
    assert_eq!(pins_now(manager).await, Some(batch(1, 3)));
    manager.cancel_download(&releases[2]).await.unwrap();
    assert_eq!(pins_now(manager).await, None, "no pin waits: the batch is over");
}

/// Pins and make-locals share the download queue but each line counts its
/// own batch: a make-local done counts only on its own line, a failed pin
/// queued again as a make-local joins the make-local batch and stays in the
/// pin batch it left, and both lines carry the queue's one hold.
#[cfg(desktop)]
#[tokio::test]
async fn pins_and_make_locals_are_counted_apart() {
    let (test, temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let releases = pinnable_releases(&test, 4).await;
    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();

    queue_pins(manager, &[releases[0].clone(), releases[1].clone()]).await;
    manager
        .make_releases_local(vec![releases[2].clone(), releases[3].clone()], &folder)
        .await
        .unwrap();
    let activity = activity_now(manager).await;
    let line = |batch| bae_views::DownloadQueueActivity {
        batch,
        wait: Some(bae_transfers::QueueWait::Paused),
    };
    assert_eq!(
        (activity.pins, activity.making_local),
        (Some(line(batch(0, 2))), Some(line(batch(0, 2))))
    );

    // The make-local of the third release, as the worker runs it: its files
    // switched to this device in the write that ends its entry. Starting it
    // resumes the queue, so the worker must have read the paused one first.
    settled_download_worker(manager).await;
    let made_local = releases[2].clone();
    let activated = manager
        .store
        .call(move |sql| {
            bae_transfers::tables::worker_moves::activate_on::<bae_transfers::DownloadRequest>(
                sql,
                &made_local,
            )
        })
        .await
        .unwrap();
    assert!(activated);
    let files = release_files(manager, &releases[2]).await;
    let paths = files
        .iter()
        .map(|file| {
            let path = bae_records::stored_name::copy_path(
                crate::test_libraries::TEST_RELEASE_FOLDER,
                &file.original_filename,
            );
            (file.file, path)
        })
        .collect();
    let (_stop, stopped) = tokio::sync::watch::channel(false);
    let prepared =
        manager.store.prepare_device_only(&folder, paths, &stopped, |_| {}).await.unwrap();
    let entry = bae_transfers::tables::worker_moves::active_entry::<bae_transfers::DownloadRequest>(
        &releases[2],
    );
    manager
        .store
        .call(move |sql| {
            sql.make_device_only(prepared)?;
            entry.finish_on(sql)
        })
        .await
        .unwrap();
    manager.set_downloads_paused(true).await.unwrap();
    let activity = activity_now(manager).await;
    assert_eq!(
        (activity.pins.map(|pins| pins.batch), activity.making_local.map(|lines| lines.batch)),
        (Some(batch(0, 2)), Some(batch(1, 2))),
        "the make-local done counts on its own line"
    );

    let failed = releases[0].clone();
    manager
        .store
        .call(move |sql| {
            bae_transfers::tables::worker_moves::fail_on::<bae_transfers::DownloadRequest>(
                sql,
                &failed,
                "the network connection was lost",
            )
        })
        .await
        .unwrap();
    manager.make_releases_local(vec![releases[0].clone()], &folder).await.unwrap();
    let activity = activity_now(manager).await;
    assert_eq!(
        (activity.pins.map(|pins| pins.batch), activity.making_local.map(|lines| lines.batch)),
        (Some(batch(0, 2)), Some(batch(1, 3))),
        "the failed pin, queued again to make local, is in both batches"
    );

    manager.cancel_download(&releases[1]).await.unwrap();
    assert_eq!(pins_now(manager).await, None, "no pin waits");
}

/// Watch the Pinning line's bar until it is `want`.
async fn pin_bar_until(bars: &mut crate::ActivityBarsWatch, want: Option<crate::BatchBar>) {
    loop {
        let told = bars.next().await.expect("the watch stays open").unwrap();
        if told.pins == want {
            return;
        }
    }
}

/// The release's whole size, as its storage row holds it.
async fn release_size(manager: &LibraryManager, release: &str) -> u64 {
    let release = release.to_string();
    manager
        .store
        .read(move |sql| {
            Ok(sql.query_row(
                "SELECT total_size FROM release_storage WHERE release_id = ?1",
                [release],
                |row| bae_store::read::count_column(row, "total_size"),
            )?)
        })
        .await
        .unwrap()
}

/// The Pinning line's bar is the bytes of its batch's releases pinned, of
/// all of theirs: none while they wait, the first release's size once it is
/// pinned, and no bar once the batch's last release is and the batch is
/// over.
#[tokio::test]
async fn the_pin_bar_fills_with_each_release_pinned() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let releases = pinnable_releases(&test, 2).await;
    let first = release_size(manager, &releases[0]).await;
    let total = first + release_size(manager, &releases[1]).await;
    let mut bars = manager.watch_activity_bars();

    queue_pins(manager, &releases).await;
    pin_bar_until(&mut bars, Some(crate::BatchBar { bytes_done: 0, bytes_total: total })).await;
    pin(manager, &releases[0]).await;
    pin_bar_until(&mut bars, Some(crate::BatchBar { bytes_done: first, bytes_total: total })).await;
    pin(manager, &releases[1]).await;
    pin_bar_until(&mut bars, None).await;
}
