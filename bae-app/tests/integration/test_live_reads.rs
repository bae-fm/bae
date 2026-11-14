//! The screens' live reads over the library and its views: each follows
//! the library's state as writes, transfers and transitions change it, and
//! moves to another id or view in place.

#![expect(
    clippy::disallowed_methods,
    reason = "test code: what it spawns ends with the test, which has no crash reporter to report to"
)]

use bae_library::test_libraries::{
    insert_local_release_with_files, insert_release, insert_release_with_queued_uploads,
    storage_library, test_album, test_release, TestLibrary,
};
use bae_records::DbRelease;
use bae_util::test_channels::HANG_GUARD;
use bae_views::album_detail::StorageRowStatus;
use bae_views::LibraryViews;
use coven::MemoryStorage;
use std::sync::Arc;
use tempfile::TempDir;

/// A test library over in-memory storage, with storage not yet set up.
async fn open_library() -> (TestLibrary, Arc<MemoryStorage>, TempDir) {
    let dir = TempDir::new().unwrap();
    let (library, storage) = storage_library(
        dir.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    (library, storage, dir)
}

/// The library's views over its store.
fn views(library: &TestLibrary) -> LibraryViews {
    LibraryViews::new(library.store().clone())
}

/// `count` albums titled "Album 0", "Album 1", …, each with one release.
async fn seed_albums(library: &TestLibrary, count: usize) {
    for i in 0..count {
        let mut album = test_album();
        album.title = format!("Album {i}");
        library.records().insert_album(&album).await.unwrap();
        insert_release(library.records(), &test_release(&album.id)).await;
    }
}

/// A library with storage set up and a release whose one file waits to
/// upload, and that file's id.
async fn queued_upload_library(
    folder: &str,
) -> (TestLibrary, Arc<MemoryStorage>, TempDir, DbRelease, String) {
    let (library, storage, dir) = open_library().await;
    bae_library::test_libraries::connect_storage(library.manager(), library.store(), &storage)
        .await;
    let release = insert_release_with_queued_uploads(
        &library,
        &dir.path().join(folder),
        "Test Album",
        &[("a.flac", &vec![b'a'; 1000])],
    )
    .await;
    let file_id = library.records().get_files_for_release(&release.id).await.unwrap()[0].id.clone();
    (library, storage, dir, release, file_id)
}

/// Pull `next` until `wanted` holds of a value, failing if none ever does.
async fn until<Value, Error: std::fmt::Debug>(
    what: &str,
    mut next: impl AsyncFnMut() -> Result<Value, Error>,
    wanted: impl Fn(&Value) -> bool,
) {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            if wanted(&next().await.unwrap()) {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what}"));
}

/// Opening an album while sync runs still shows the album.
///
/// A running sync cycle keeps coven's reader busy and writes rows the album
/// detail's query reads the whole time, and no rate of either may stop the
/// query from reaching its first value: a subscription that keeps restarting
/// its read delivers neither a value nor an error, which reads on screen as a
/// spinner that never resolves. The release detail and the storage page read
/// the same way, through the same `LiveRead`.
///
/// The load here is sync-shaped without a sync loop: reads keeping coven's
/// reader connection busy so the query's read waits its turn behind them, and
/// a steady stream of writes to a row the query reads, a release of the album
/// entering and leaving the download queue. Both are paced by something
/// other than the CPU — the reads by the connection thread, the writes by a
/// timer — so this runtime stays able to schedule the deadline below, and a
/// regression fails on it instead of hanging.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn album_detail_delivers_while_sync_shaped_load_runs() {
    let (manager, _storage, _temp_dir) = open_library().await;
    let album = test_album();
    manager.records().insert_album(&album).await.unwrap();
    let mut releases = Vec::new();
    for _ in 0..25 {
        let release = test_release(&album.id);
        insert_release(manager.records(), &release).await;
        releases.push(release.id);
    }

    let mut load = Vec::new();
    for _ in 0..8 {
        let db = manager.store().clone();
        let records = bae_records::test_fixtures::test_records(&db);
        let album_id = album.id.clone();
        load.push(tokio::spawn(async move {
            loop {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "this is load on the db; each read's answer is not the point"
                )]
                let _ = records.get_releases_for_album(&album_id).await;
            }
        }));
    }
    let db = manager.store().clone();
    let queued = releases[0].clone();
    let queueing = manager.manager().clone();
    // The worker leaves the entry to this load.
    queueing.set_downloads_paused(true).await.unwrap();
    load.push(tokio::spawn(async move {
        loop {
            let release = format!("'{queued}'");
            db.call(move |sql| {
                bae_transfers::tables::bulk::queue_downloads_on(sql, 1, 2, &release)
            })
            .await
            .unwrap();
            queueing.cancel_download(&queued).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    }));

    let values = bae_app::live_reads::subscribe_album_detail(manager.manager(), &views(&manager));
    values.set(Some(album.id.clone())).unwrap();
    let delivered = tokio::time::timeout(HANG_GUARD, async {
        loop {
            match values.next().await {
                Ok(snapshot) if snapshot.id.is_none() => continue,
                other => return other,
            }
        }
    })
    .await;

    // Before the assertions, so a failure reports rather than leaving the
    // load running.
    for task in load {
        task.abort();
    }
    drop(values);

    let detail = delivered
        .expect("the album detail arrives while sync-shaped load runs")
        .expect("the album detail resolves")
        .value
        .expect("the album is present");
    assert_eq!(detail.album.id, album.id);
}

/// A detail view that moves to another album moves its one read: the next
/// value answers the new id, and clearing the id reads nothing.
#[tokio::test]
async fn album_detail_moves_between_albums_on_one_subscription() {
    let (manager, _storage, _temp_dir) = open_library().await;
    let mut albums = Vec::new();
    for title in ["Album One", "Album Two"] {
        let mut album = test_album();
        album.title = title.to_string();
        manager.records().insert_album(&album).await.unwrap();
        insert_release(manager.records(), &test_release(&album.id)).await;
        albums.push(album);
    }
    let values = bae_app::live_reads::subscribe_album_detail(manager.manager(), &views(&manager));
    let next_for = |id: Option<String>| {
        let values = &values;
        async move {
            loop {
                let snapshot = tokio::time::timeout(HANG_GUARD, values.next())
                    .await
                    .expect("the album detail delivers")
                    .expect("the album detail resolves");
                if snapshot.id == id {
                    return snapshot;
                }
            }
        }
    };

    values.set(Some(albums[0].id.clone())).unwrap();
    let first = next_for(Some(albums[0].id.clone())).await;
    assert_eq!(first.value.expect("present").album.title, "Album One");

    values.set(Some(albums[1].id.clone())).unwrap();
    let second = next_for(Some(albums[1].id.clone())).await;
    assert_eq!(second.value.expect("present").album.title, "Album Two");

    values.set(None).unwrap();
    assert!(next_for(None).await.value.is_none());
}

/// A release's storage choices follow whether the library syncs within its one read:
/// a local release offers none until storage is set up, then offers its
/// upload, with nothing asked of the subscription in between.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn release_detail_offers_storage_once_storage_is_set_up() {
    let (manager, storage, temp_dir) = open_library().await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("local-source"),
        "Test Album",
        &[("a.flac", b"aaa")],
    )
    .await;
    let values = bae_app::live_reads::subscribe_release_detail(manager.manager(), &views(&manager));
    values.set(Some(release.id.clone())).unwrap();
    let actions_until =
        |accept: fn(&[bae_types::release_storage::ReleaseStorageAction]) -> bool| {
            let values = &values;
            async move {
                tokio::time::timeout(HANG_GUARD, async {
                    loop {
                        let snapshot = values.next().await.expect("the release detail resolves");
                        if let Some(detail) = snapshot.value {
                            if accept(&detail.summary.storage_actions) {
                                return detail.summary.storage_actions;
                            }
                        }
                    }
                })
                .await
                .expect("the release detail shows the expected storage choices")
            }
        };
    actions_until(|actions| actions.is_empty()).await;
    bae_library::test_libraries::connect_storage(manager.manager(), manager.store(), &storage)
        .await;
    assert_eq!(
        actions_until(|actions| !actions.is_empty()).await,
        vec![bae_types::release_storage::ReleaseStorageAction::Upload]
    );
}

/// A pin reaches the storage view as it lands, from coven's pin-state watch:
/// no sync cycle runs behind this connection and no transfer reports it.
#[tokio::test]
async fn a_pin_reaches_the_storage_view_without_a_sync_cycle() {
    let (manager, storage, temp_dir) = open_library().await;
    bae_library::test_libraries::connect_storage(manager.manager(), manager.store(), &storage)
        .await;
    let release = insert_local_release_with_files(
        &manager,
        &temp_dir.path().join("pin-watch"),
        "Pin Watch",
        &[("track.flac", b"track-bytes")],
    )
    .await;
    manager.manager().upload_release(&release.id).await.unwrap();
    manager.manager().drain_uploads_expecting_work().await.unwrap();

    let values = bae_app::live_reads::subscribe_storage_browse(
        manager.manager(),
        &views(&manager),
        bae_views::StorageBrowseRequest {
            sort: bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::AlbumTitle,
                direction: bae_store::SortDirection::Ascending,
            },
            filter: bae_views::StorageFilter::All,
            search: String::new(),
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
            selected: Default::default(),
        },
    );
    let pinned = |value: &bae_views::StorageBrowseSnapshot| {
        value
            .windows
            .iter()
            .flat_map(|window| &window.rows)
            .find(|row| row.release.id == release.id)
            .map(|row| {
                matches!(
                    row.release.storage_state,
                    bae_views::album_detail::ReleaseStorageState::InStorage { pinned: true }
                )
            })
    };
    let first = values.next().await.unwrap();
    assert_eq!(pinned(&first), Some(false));

    manager.manager().enqueue_pins(vec![release.id.clone()]).await.unwrap();

    loop {
        let value = tokio::time::timeout(HANG_GUARD, values.next())
            .await
            .expect("the pin reaches the storage view")
            .unwrap();
        if pinned(&value) == Some(true) {
            break;
        }
    }
}

async fn next_storage_value(
    values: &bae_app::live_reads::LiveStorageBrowse,
) -> bae_views::StorageBrowseSnapshot {
    tokio::time::timeout(HANG_GUARD, values.next())
        .await
        .expect("the storage view delivers")
        .expect("the storage view resolves")
}

/// The Storage Manager reads through one subscription: moving its windows,
/// its sort, or its filter delivers the new view on the same subscription,
/// with the filtered set's count and total size beside every value.
#[tokio::test]
async fn the_storage_view_moves_one_subscription() {
    let (manager, _storage, _temp_dir) = open_library().await;
    seed_albums(&manager, 3).await;
    let window = |offset, limit| bae_store::LibraryPageWindow { offset, limit };
    let view = |direction, filter, windows: &[bae_store::LibraryPageWindow]| {
        bae_views::StorageBrowseRequest {
            sort: bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::AlbumTitle,
                direction,
            },
            filter,
            search: String::new(),
            windows: windows.iter().cloned().collect(),
            selected: Default::default(),
        }
    };
    let titles = |value: &bae_views::StorageBrowseSnapshot| {
        value
            .windows
            .iter()
            .map(|window| window.rows.iter().map(|row| row.album.title.clone()).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let ascending = bae_store::SortDirection::Ascending;
    let all = bae_views::StorageFilter::All;
    let values = bae_app::live_reads::subscribe_storage_browse(
        manager.manager(),
        &views(&manager),
        view(ascending, all, &[window(0, 2)]),
    );

    let first = next_storage_value(&values).await;
    assert_eq!(titles(&first), vec![vec!["Album 0", "Album 1"]]);
    assert_eq!(first.total_count, 3);

    values.set(view(ascending, all, &[window(0, 1), window(2, 1)])).unwrap();
    let scrolled = next_storage_value(&values).await;
    assert_eq!(titles(&scrolled), vec![vec!["Album 0"], vec!["Album 2"]]);
    assert_eq!(scrolled.total_count, 3, "one count answers every window");

    let descending = bae_store::SortDirection::Descending;
    values.set(view(descending, all, &[window(0, 2)])).unwrap();
    let resorted = next_storage_value(&values).await;
    assert_eq!(resorted.sort.direction, descending);
    assert_eq!(titles(&resorted), vec![vec!["Album 2", "Album 1"]]);

    let local = bae_views::StorageFilter::Local;
    values.set(view(descending, local, &[window(0, 2)])).unwrap();
    let filtered = next_storage_value(&values).await;
    assert_eq!(filtered.filter, local);
    assert_eq!(filtered.total_count, views(&manager).get_storage_count(local).await.unwrap());
    assert_eq!(filtered.total_size, views(&manager).get_storage_total_size(local).await.unwrap());
}

/// A release queued to be moved to this device reaches the album detail, the release
/// detail and the Storage Manager, each read from its download queue entry:
/// while it waits the release shows it and offers no storage action, and a
/// cancel takes it out of all three.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_to_this_device_entry_reaches_the_reads() {
    let (manager, storage, dir) = open_library().await;
    bae_library::test_libraries::connect_storage(manager.manager(), manager.store(), &storage)
        .await;
    let (release_id, _files) = crate::create_local_release(
        manager.manager(),
        &dir.path().join("originals"),
        &[("a.flac", b"audio")],
    )
    .await;
    manager.manager().upload_release(&release_id).await.unwrap();
    manager.manager().drain_uploads_expecting_work().await.unwrap();
    let album_id =
        manager.records().find_release_by_id(&release_id).await.unwrap().unwrap().album_id;

    let views = views(&manager);
    let album_detail = bae_app::live_reads::subscribe_album_detail(manager.manager(), &views);
    album_detail.set(Some(album_id)).unwrap();
    let release_detail = bae_app::live_reads::subscribe_release_detail(manager.manager(), &views);
    release_detail.set(Some(release_id.clone())).unwrap();
    let storage_browse = bae_app::live_reads::subscribe_storage_browse(
        manager.manager(),
        &views,
        bae_views::StorageBrowseRequest {
            sort: bae_views::StorageSortCriterion {
                field: bae_views::StorageSortField::AlbumTitle,
                direction: bae_store::SortDirection::Ascending,
            },
            filter: bae_views::StorageFilter::All,
            search: String::new(),
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
            selected: Default::default(),
        },
    );
    let waiting = Some(bae_views::album_detail::MoveToThisDeviceStatus::Queued);
    let album_shows = |status: Option<bae_views::album_detail::MoveToThisDeviceStatus>| {
        move |snapshot: &bae_library::DetailSnapshot<bae_views::album_detail::AlbumDetail>| {
            snapshot.value.as_ref().is_some_and(|detail| {
                detail.releases[0].move_to_this_device == status
                    && detail.releases[0].summary.transitioning == status.is_some()
            })
        }
    };
    let release_shows = |status: Option<bae_views::album_detail::MoveToThisDeviceStatus>| {
        move |snapshot: &bae_library::DetailSnapshot<bae_views::album_detail::ReleaseDetail>| {
            snapshot.value.as_ref().is_some_and(|detail| {
                detail.move_to_this_device == status
                    && detail.summary.storage_actions.is_empty() == status.is_some()
            })
        }
    };
    let row_shows = |status: StorageRowStatus| {
        move |snapshot: &bae_views::StorageBrowseSnapshot| {
            snapshot.windows.iter().flat_map(|window| &window.rows).any(|row| row.status == status)
        }
    };

    until("the album shows no move-to-this-device", || album_detail.next(), album_shows(None))
        .await;
    until(
        "the release shows no move-to-this-device",
        || release_detail.next(),
        release_shows(None),
    )
    .await;
    until("the row rests", || storage_browse.next(), row_shows(StorageRowStatus::InStorage)).await;

    // The worker leaves the entry waiting.
    manager.manager().set_downloads_paused(true).await.unwrap();
    let folder = dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    manager
        .manager()
        .move_releases_to_this_device(vec![release_id.clone()], &folder)
        .await
        .unwrap();
    until(
        "the album shows the move-to-this-device",
        || album_detail.next(),
        album_shows(waiting.clone()),
    )
    .await;
    until(
        "the release shows the move-to-this-device",
        || release_detail.next(),
        release_shows(waiting.clone()),
    )
    .await;
    until(
        "the row shows the move-to-this-device",
        || storage_browse.next(),
        row_shows(StorageRowStatus::MovingToThisDevice(
            bae_views::album_detail::DownloadEntryStep::Queued,
        )),
    )
    .await;

    manager.manager().cancel_release_transition(&release_id).await.unwrap();
    until("the album's move-to-this-device ends", || album_detail.next(), album_shows(None)).await;
    until("the release's move-to-this-device ends", || release_detail.next(), release_shows(None))
        .await;
    until("the row rests again", || storage_browse.next(), row_shows(StorageRowStatus::InStorage))
        .await;
}

/// Follow the inspector until it shows a value `accept` takes.
async fn inspector_until(
    inspector: &bae_library::LiveDetail<bae_views::storage_inspector::StorageInspector>,
    accept: impl Fn(&bae_views::storage_inspector::StorageInspector) -> bool,
) -> bae_views::storage_inspector::StorageInspector {
    tokio::time::timeout(HANG_GUARD, async {
        loop {
            let snapshot = inspector.next().await.expect("the inspector reads");
            if let Some(value) = snapshot.value.filter(|value| accept(value)) {
                break value;
            }
        }
    })
    .await
    .expect("the inspector shows the expected value")
}

/// The Storage Manager inspector shows each of its release's files beside
/// its upload while one waits, and the error a failed upload met while this
/// process holds it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_inspector_shows_each_files_upload_and_its_error() {
    let (manager, _storage, temp, release, file_id) = queued_upload_library("inspected").await;
    let inspector =
        bae_app::live_reads::subscribe_storage_inspector(manager.manager(), &views(&manager));
    inspector.set(Some(release.id.clone())).unwrap();
    use bae_views::storage_inspector::StorageInspectorTransfer;
    let waiting = inspector_until(&inspector, |value| {
        value.files.iter().any(|row| row.file.id == file_id && row.upload.is_some())
    })
    .await;
    assert!(waiting
        .transfers
        .iter()
        .any(|transfer| matches!(transfer, StorageInspectorTransfer::Upload { .. })));

    // The upload can't read a source that moved, so it waits on the file
    // with the error it met.
    std::fs::remove_file(temp.path().join("inspected/a.flac")).unwrap();
    assert_eq!(manager.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    inspector_until(&inspector, |value| {
        value.files.iter().any(|row| {
            row.file.id == file_id
                && row
                    .upload
                    .as_ref()
                    .and_then(|upload| upload.pending.as_ref())
                    .is_some_and(|wait| wait.cause.is_some())
        })
    })
    .await;
}

/// An upload storage refused waits on its own record, and the inspector
/// shows the error coven kept beside it.
#[tokio::test]
async fn the_inspector_shows_the_error_an_upload_met() {
    let (manager, storage, _temp, release, file_id) = queued_upload_library("refused").await;
    let inspector =
        bae_app::live_reads::subscribe_storage_inspector(manager.manager(), &views(&manager));
    inspector.set(Some(release.id.clone())).unwrap();
    storage
        .set_faults(coven::Faults {
            refuse_file_uploads: 1,
            failure: coven::StorageFailure::Network,
            response: Some(500),
            ..coven::Faults::none()
        })
        .await;
    assert_eq!(manager.manager().drain_uploads_expecting_work().await.unwrap(), 0);
    inspector_until(&inspector, |value| {
        value.files.iter().any(|row| {
            row.file.id == file_id
                && row
                    .upload
                    .as_ref()
                    .and_then(|upload| upload.pending.as_ref())
                    .and_then(|wait| wait.cause.as_deref())
                    .is_some_and(|cause| cause.contains("500"))
        })
    })
    .await;
}
