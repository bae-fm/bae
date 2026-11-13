//! File attachment, uploads, pins, and local copies through the library manager.
//! Coven uses MemoryStorage for cloud reads and writes; bae owns the release
//! transitions, their progress, and their subscribed storage values.

use bae_app::live_reads::LiveStorageBrowse;
use bae_library::storage::transfer::TransferProgress;
use bae_library::LibraryManager;
use bae_records::GallerySource;
use bae_store::SortDirection;
use bae_util::test_channels::HANG_GUARD;
use bae_views::album_detail::ReleaseStorageState;
use bae_views::StorageBrowseRequest;
use bae_views::{StorageFilter, StorageSortCriterion, StorageSortField};
use std::path::Path;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn setup(tmp: &TempDir) -> bae_library::test_libraries::TestLibrary {
    bae_library::test_libraries::open_test_library(tmp.path()).await
}

async fn setup_with_cloud(tmp: &TempDir) -> bae_library::test_libraries::TestLibrary {
    let (library, storage) = bae_library::test_libraries::storage_library(
        tmp.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    bae_library::test_libraries::connect_storage(library.manager(), library.store(), &storage)
        .await;
    library
}

/// Get a Remote (cloud-only, not pinned) release whose files sit in `cloud`:
/// create it Local, attach its files, and finish the uploads.
async fn create_remote_release(
    mgr: &LibraryManager,
    db: &bae_store::Store,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> String {
    let (release_id, named) = crate::create_local_release(mgr, source_dir, files).await;
    mgr.upload_release(&release_id).await.unwrap();
    let count = mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, named.len(), "all files uploaded");
    assert_eq!(
        crate::release_storage(db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: false },
        "Remote (cloud-only) after the drain"
    );
    release_id
}

/// Drain a transfer receiver to its terminal event.
async fn collect_progress(
    mut rx: bae_library::storage::transfer::TransferProgressFeed,
) -> Vec<TransferProgress> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let terminal =
            matches!(event, TransferProgress::Complete { .. } | TransferProgress::Failed { .. });
        events.push(event);
        if terminal {
            break;
        }
    }
    events
}

/// Pin `release_id` as the download worker does: its queue entry started, the
/// pin ending it in its write. Returns the events the pin reported and how it
/// ended. A release the queue passes over (a local one) has no entry, which
/// the pin never gets to end.
async fn pin_with_events(
    library: &bae_library::test_libraries::TestLibrary,
    release_id: &str,
) -> (Vec<TransferProgress>, Result<bae_transfers::Finished, bae_library::LibraryError>) {
    let manager = library.manager();
    manager.set_downloads_paused(true).await.unwrap();
    manager.enqueue_pins(vec![release_id.to_string()]).await.unwrap();
    let started = release_id.to_string();
    library
        .store()
        .call(move |sql| {
            bae_transfers::tables::worker_moves::activate_on::<bae_transfers::DownloadRequest>(
                sql, &started,
            )
        })
        .await
        .unwrap();
    let entry = bae_transfers::tables::worker_moves::active_entry::<bae_transfers::DownloadRequest>(
        release_id,
    );
    let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = events.clone();
    let outcome = manager
        .start_pin_transfer(release_id.to_string(), entry, |progress| async move {
            *sink.lock().unwrap() = collect_progress(progress).await;
            Ok(())
        })
        .finish()
        .await;
    let events = std::mem::take(&mut *events.lock().unwrap());
    (events, outcome)
}

fn failed(events: &[TransferProgress]) -> bool {
    events.iter().any(|e| matches!(e, TransferProgress::Failed { .. }))
}

async fn expect_storage_state(
    subscription: &LiveStorageBrowse,
    release_id: &str,
    expected: ReleaseStorageState,
) {
    loop {
        let value = tokio::time::timeout(HANG_GUARD, subscription.next())
            .await
            .expect("storage subscription delivers a value")
            .expect("storage projection resolves");
        if value
            .windows
            .iter()
            .flat_map(|window| &window.rows)
            .any(|row| row.release.id == release_id && row.release.storage_state == expected)
        {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// The upload enqueue is counted before the drain
// ---------------------------------------------------------------------------

/// An upload attaches the release's files and queues their uploads, which
/// the release's queue count reads the instant the upload returns — before
/// any byte drains — so the upload shows up at once rather than only after
/// it finishes.
#[tokio::test]
async fn uploads_are_counted_before_drain() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let source = tmp.path().join("src");
    let (release_id, named) = crate::create_local_release(
        mgr.manager(),
        &source,
        &[("01.flac", b"aaaaaaaa"), ("02.flac", b"bbbbbbbbbbbb")],
    )
    .await;

    // Enqueue upload through the real coven path; do NOT drain.
    mgr.manager().upload_release(&release_id).await.unwrap();

    assert_eq!(
        mgr.manager().count_pending_uploads_for_release(&release_id).await.unwrap(),
        named.len() as i64,
        "every file is queued immediately after upload"
    );
}

// ---------------------------------------------------------------------------
// Host-provided cover file via coven's local store
// ---------------------------------------------------------------------------

/// A cover is a host-provided file: its bytes go to coven's local store
/// store and coven owns the copy. While the release is Local the cover lives in
/// coven's local store and `read_image_bytes` serves its bytes through the image
/// reference — no cloud round-trip and no bae path into coven's store.
#[tokio::test]
async fn test_cover_file_stored_via_local_files_is_readable() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (release_id, _f) =
        crate::create_local_release(mgr.manager(), &tmp.path().join("src"), &[]).await;

    // No cover row yet → no bytes.
    assert!(
        bae_library::test_libraries::read_cover_image_bytes(mgr.manager(), &release_id)
            .await
            .is_none(),
        "no cover before one is stored"
    );

    // Store the cover bytes and `covers` row in one coven batch, exactly as
    // import / change_cover does.
    let bytes = b"cover-jpeg-bytes";
    mgr.manager()
        .store_library_image(
            &bae_records::DbLibraryImage {
                id: release_id.clone(),
                image_type: bae_records::LibraryImageType::Cover,
                content_type: bae_util::content_type::ContentType::Jpeg,
            },
            bytes,
        )
        .await
        .unwrap();

    // read_image_bytes serves it from coven's local store, byte-for-byte.
    let read = bae_library::test_libraries::read_cover_image_bytes(mgr.manager(), &release_id)
        .await
        .expect("cover resolves to bytes once stored");
    assert_eq!(read, bytes, "the stored cover reads back byte-for-byte");
}

// ---------------------------------------------------------------------------
// Pin / unpin guards
// ---------------------------------------------------------------------------

/// Pin rejects a Local release: there is no cloud file to fetch into the pinned
/// cache.
#[tokio::test]
async fn test_pin_rejects_local_release() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (release_id, _f) =
        crate::create_local_release(mgr.manager(), &tmp.path().join("src"), &[("a.flac", b"a")])
            .await;

    let (events, outcome) = pin_with_events(&mgr, &release_id).await;
    assert!(failed(&events), "pin must fail for a local release");
    outcome.expect_err("a failed pin ends no queue entry");
}

/// Unpin rejects a Local release: it has no remote files to drop from the cache.
#[tokio::test]
async fn test_unpin_rejects_local_release() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (release_id, _f) =
        crate::create_local_release(mgr.manager(), &tmp.path().join("src"), &[("a.flac", b"a")])
            .await;

    assert!(
        mgr.manager().unpin_releases(std::slice::from_ref(&release_id)).await.is_err(),
        "unpin must fail for a local release"
    );
}

// ---------------------------------------------------------------------------
// Pin / unpin success — through coven's cache
// ---------------------------------------------------------------------------

/// A release's detail carries its download control's state, read from its
/// entry in the download queue.
#[tokio::test]
async fn a_release_detail_says_where_its_download_stands() {
    use bae_views::album_detail::ReleaseDownloadStatus;
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let release_id = create_remote_release(
        mgr.manager(),
        &db,
        &tmp.path().join("src"),
        &[("a.flac", b"bytes-a")],
    )
    .await;
    let status = |db: bae_store::Store, release_id: String| async move {
        bae_views::LibraryViews::new(db)
            .find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("the library holds the release")
            .download_status
    };
    assert_eq!(
        status(db.clone(), release_id.clone()).await,
        Some(ReleaseDownloadStatus::Available)
    );

    mgr.manager().set_downloads_paused(true).await.unwrap();
    mgr.manager().enqueue_pins(vec![release_id.clone()]).await.unwrap();
    assert_eq!(status(db.clone(), release_id.clone()).await, Some(ReleaseDownloadStatus::Queued));
}

/// Pin a Remote cloud-only release: coven fetches its files from the cloud into
/// the pinned cache, so it reads as (Remote, pinned). Its progress counts
/// against the release's whole size, never goes back, and reaches that size
/// before the pin completes. Which steps it reports before that depends on
/// how coven's download watch races the query of kept files, so none is
/// asserted.
#[tokio::test]
async fn test_pin_remote_fetches_into_pinned_cache() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let release_id = create_remote_release(
        mgr.manager(),
        &db,
        &tmp.path().join("src"),
        &[("a.flac", b"pin-bytes-a"), ("b.flac", b"pin-bytes-b")],
    )
    .await;

    let (events, outcome) = pin_with_events(&mgr, &release_id).await;
    outcome.expect("pin task completes");
    let Some((TransferProgress::Complete { .. }, before)) = events.split_last() else {
        panic!("a pin completes: {events:?}");
    };
    let progress: Vec<_> = before
        .iter()
        .map(|event| match event {
            TransferProgress::Progress { progress } => progress,
            other => panic!("only progress before completion: {other:?}"),
        })
        .collect();
    assert!(
        progress.iter().all(|progress| progress.bytes_total == 22),
        "progress counts against the release's size: {progress:?}"
    );
    assert!(
        progress.windows(2).all(|pair| pair[0].bytes_done <= pair[1].bytes_done),
        "progress never goes back: {progress:?}"
    );
    assert_eq!(
        progress.last().map(|progress| progress.bytes_done),
        Some(22),
        "the pin reports every byte before it completes"
    );
    assert_eq!(
        crate::release_storage(&db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: true }
    );
}

/// The release and album detail views show a release pinned once its files
/// are, and unpinned again once they aren't, each from its own live read.
#[tokio::test]
async fn the_detail_views_follow_a_releases_pin() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        mgr.manager(),
        mgr.store(),
        &tmp.path().join("src"),
        &[("a.flac", b"follow-bytes")],
    )
    .await;
    let album_id = mgr
        .manager()
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .expect("the library holds the release")
        .album_id;
    let views = bae_views::LibraryViews::new(mgr.store().clone());
    let release = bae_app::live_reads::subscribe_release_detail(mgr.manager(), &views);
    release.set(Some(release_id.clone())).unwrap();
    let album = bae_app::live_reads::subscribe_album_detail(mgr.manager(), &views);
    album.set(Some(album_id)).unwrap();
    let shown = |pinned: bool| {
        let (release, album) = (&release, &album);
        async move {
            tokio::time::timeout(HANG_GUARD, async {
                loop {
                    let value = release.next().await.expect("the release detail resolves");
                    if value.value.is_some_and(|detail| {
                        detail.summary.storage_state == ReleaseStorageState::InStorage { pinned }
                    }) {
                        break;
                    }
                }
                loop {
                    let value = album.next().await.expect("the album detail resolves");
                    if value.value.is_some_and(|detail| {
                        detail.releases[0].summary.storage_state
                            == ReleaseStorageState::InStorage { pinned }
                    }) {
                        break;
                    }
                }
            })
            .await
            .unwrap_or_else(|_| panic!("the detail views never showed pinned = {pinned}"));
        }
    };
    shown(false).await;

    pin_with_events(&mgr, &release_id).await.1.expect("pin task completes");
    shown(true).await;

    mgr.manager().unpin_releases(std::slice::from_ref(&release_id)).await.unwrap();
    shown(false).await;
}

/// Unpin a Remote pinned release: coven moves its files out of the pinned cache
/// into the evictable cache, so it reads as (Remote, not pinned).
#[tokio::test]
async fn test_unpin_remote_drops_from_pinned_cache() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let db = mgr.store().clone();
    let release_id = create_remote_release(
        mgr.manager(),
        &db,
        &tmp.path().join("src"),
        &[("a.flac", b"unpin-bytes")],
    )
    .await;

    pin_with_events(&mgr, &release_id).await.1.expect("pin task completes");
    assert_eq!(
        crate::release_storage(&db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: true }
    );

    mgr.manager().unpin_releases(std::slice::from_ref(&release_id)).await.unwrap();
    assert_eq!(
        crate::release_storage(&db, &release_id).await,
        ReleaseStorageState::InStorage { pinned: false }
    );
}

// ---------------------------------------------------------------------------
// Reading a release file surfaces a missing original as an error
// ---------------------------------------------------------------------------

/// Reading a Local release whose original file vanished returns an error.
#[tokio::test]
async fn test_missing_external_source_maps_to_error() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (release_id, _f) =
        crate::create_local_release(mgr.manager(), &source_dir, &[("a.flac", b"present")]).await;
    let file =
        mgr.manager().get_files_for_release(&release_id).await.unwrap().into_iter().next().unwrap();

    // The user moved or deleted their original file.
    tokio::fs::remove_file(source_dir.join("a.flac")).await.unwrap();

    let result = mgr
        .manager()
        .read_gallery_bytes(&release_id, &GallerySource::ReleaseFile { file_id: file.id })
        .await;
    assert!(result.is_err(), "a vanished external source must surface a read error");
}

// ---------------------------------------------------------------------------
// Transition completions deliver new storage values
// ---------------------------------------------------------------------------

/// A drained upload and audio detachment both change the subscribed Storage
/// Manager value without a separate library event or another read from the UI.
#[tokio::test]
async fn transition_completions_deliver_storage_values() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (release_id, _named) =
        crate::create_local_release(mgr.manager(), &source_dir, &[("a.flac", b"round-trip-bytes")])
            .await;
    let views = bae_views::LibraryViews::new(mgr.store().clone());
    let values = bae_app::live_reads::subscribe_storage_browse(
        mgr.manager(),
        &views,
        StorageBrowseRequest {
            sort: StorageSortCriterion {
                field: StorageSortField::AlbumTitle,
                direction: SortDirection::Ascending,
            },
            filter: StorageFilter::All,
            search: String::new(),
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect(),
            selected: Default::default(),
        },
    );
    expect_storage_state(&values, &release_id, ReleaseStorageState::OnOneDevice).await;

    mgr.manager().upload_release(&release_id).await.unwrap();
    mgr.manager().drain_uploads_expecting_work().await.unwrap();
    expect_storage_state(&values, &release_id, ReleaseStorageState::InStorage { pinned: false })
        .await;

    let dest = tmp.path().join("brought-back");
    std::fs::create_dir(&dest).unwrap();
    mgr.manager().make_releases_local(vec![release_id.clone()], &dest).await.unwrap();
    expect_storage_state(&values, &release_id, ReleaseStorageState::OnOneDevice).await;
}

/// The Storage Manager's inspector shows, and its actions act on, a release
/// only while the list holds it: the list answers with the row of each
/// selected release it holds wherever it sits, in a window or not, and a
/// selected release whose upload finishes leaves the Local list and the
/// answer together.
#[tokio::test]
async fn selected_releases_leave_the_local_list_with_their_upload() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let (first, _) = crate::create_local_release(
        mgr.manager(),
        &tmp.path().join("first"),
        &[("a.flac", b"first")],
    )
    .await;
    let (second, _) = crate::create_local_release(
        mgr.manager(),
        &tmp.path().join("second"),
        &[("b.flac", b"second")],
    )
    .await;
    mgr.manager().upload_release(&first).await.unwrap();
    mgr.manager().upload_release(&second).await.unwrap();

    let views = bae_views::LibraryViews::new(mgr.store().clone());
    let selected: std::collections::BTreeSet<String> =
        [first.clone(), second.clone()].into_iter().collect();
    let values = bae_app::live_reads::subscribe_storage_browse(
        mgr.manager(),
        &views,
        StorageBrowseRequest {
            sort: StorageSortCriterion {
                field: StorageSortField::AlbumTitle,
                direction: SortDirection::Ascending,
            },
            filter: StorageFilter::Local,
            search: String::new(),
            // One row on screen, so one selected release sits outside it.
            windows: [bae_store::LibraryPageWindow { offset: 0, limit: 1 }].into_iter().collect(),
            selected: selected.clone(),
        },
    );
    let next = || async {
        tokio::time::timeout(HANG_GUARD, values.next())
            .await
            .expect("storage subscription delivers a value")
            .expect("storage projection resolves")
    };

    let queued = next().await;
    assert_eq!(queued.total_count, 2);
    assert_eq!(queued.windows[0].rows.len(), 1);
    let selected_ids = queued
        .selected
        .iter()
        .map(|row| row.release.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        selected_ids, selected,
        "a selected release outside every window is still in the list"
    );
    let outside = queued
        .selected
        .iter()
        .find(|row| row.release.id != queued.windows[0].rows[0].release.id)
        .expect("one selected release sits outside the window");
    assert_eq!(
        (outside.album.title.as_str(), outside.release.file_count),
        ("Album Title", 1),
        "the row of a selected release outside every window is read whole"
    );

    mgr.manager().drain_uploads_expecting_work().await.unwrap();
    loop {
        let value = next().await;
        if value.total_count == 0 {
            assert!(
                value.selected.is_empty(),
                "an uploaded release is no longer in the Local list"
            );
            break;
        }
    }
}

/// The Storage Manager offers a selection only the storage actions every
/// selected release allows, and the list's value says which those are,
/// whether or not each selected release sits in a window.
#[tokio::test]
async fn a_selection_is_offered_the_actions_all_its_releases_allow() {
    bae_util::test_logs::install();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let pinned = create_remote_release(
        mgr.manager(),
        mgr.store(),
        &tmp.path().join("pinned"),
        &[("a.flac", b"pinned")],
    )
    .await;
    let unpinned = create_remote_release(
        mgr.manager(),
        mgr.store(),
        &tmp.path().join("unpinned"),
        &[("b.flac", b"unpinned")],
    )
    .await;
    pin_with_events(&mgr, &pinned).await.1.expect("pin task completes");

    let views = bae_views::LibraryViews::new(mgr.store().clone());
    let view = |selected: &[&String]| StorageBrowseRequest {
        sort: StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        },
        filter: StorageFilter::All,
        search: String::new(),
        // One row on screen, so one selected release sits outside it.
        windows: [bae_store::LibraryPageWindow { offset: 0, limit: 1 }].into_iter().collect(),
        selected: selected.iter().map(|id| (*id).clone()).collect(),
    };
    let values = bae_app::live_reads::subscribe_storage_browse(
        mgr.manager(),
        &views,
        view(&[&pinned, &unpinned]),
    );
    let next = || async {
        tokio::time::timeout(HANG_GUARD, values.next())
            .await
            .expect("storage subscription delivers a value")
            .expect("storage projection resolves")
    };

    use bae_types::release_storage::ReleaseStorageAction::{DetachAudio, Pin};
    assert_eq!(
        next().await.selection_actions,
        [DetachAudio],
        "a pinned and an unpinned release share only detaching their audio"
    );

    values.set(view(&[&unpinned])).unwrap();
    loop {
        let value = next().await;
        if value.selected.len() == 1 {
            assert_eq!(value.selection_actions, [Pin, DetachAudio]);
            break;
        }
    }
}
