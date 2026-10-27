#![cfg(feature = "test-utils")]
//! File attachment, uploads, pins, and local copies through the library manager.
//! Coven uses MemoryStorage for cloud reads and writes; bae owns the release
//! transitions, their progress, and their subscribed storage values.

use bae_test_support as support;
use support::tracing_init;

use bae_core::album_detail::ReleaseStorageState;
use bae_core::db::{
    DbAlbum, DbFile, DbRelease, SortDirection, StorageFilter, StorageSortCriterion,
    StorageSortField,
};
use bae_core::library::{
    AppServices, CancellationToken, LibraryManager, StorageBrowseSubscription, StorageBrowseView,
};
use bae_core::storage::transfer::{read_release_file_bytes, TransferProgress, TransferService};
use bae_util::content_type::ContentType;
use chrono::Utc;
use std::path::Path;
use tempfile::TempDir;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// (storage state, pinned) for a release — the post-transition assertion target.
async fn storage(mgr: &LibraryManager, release_id: &str) -> (ReleaseStorageState, bool) {
    let s = mgr
        .find_release_storage_summary(release_id)
        .await
        .unwrap()
        .unwrap();
    (s.storage_state, s.pinned)
}

async fn setup(tmp: &TempDir) -> LibraryManager {
    support::open_test_library(tmp.path()).await.0
}

async fn setup_with_cloud(tmp: &TempDir) -> LibraryManager {
    let (manager, _, storage) =
        support::storage_library(tmp.path(), bae_core::providers::Providers::offline()).await;
    support::connect_storage(&manager, &storage).await;
    manager
}

/// Insert a Local release: its originals under `source_dir`, one `DbFile` per
/// file, and each original path in bae's device-local table.
async fn create_local_release(
    mgr: &LibraryManager,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> (String, String, Vec<(String, Vec<u8>)>) {
    let now = Utc::now();
    let album = DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: "Album Title".to_string(),
        year: Some(2024),
        is_compilation: false,
        created_at: now,
    };
    let release = DbRelease {
        id: Uuid::new_v4().to_string(),
        album_id: album.id.clone(),
        release_name: None,
        pressing: bae_types::pressing::Pressing::blank(),
        source_folder_name: None,
        album_loudness_lufs: None,
        album_peak_linear: None,
        created_at: now,
    };
    let (album_id, release_id) = (album.id.clone(), release.id.clone());
    mgr.insert_album_with_release_and_tracks(&album, &[], &release, &[], &[], &[])
        .await
        .unwrap();

    tokio::fs::create_dir_all(source_dir).await.unwrap();
    let mut result = Vec::new();
    for (name, data) in files {
        let path = source_dir.join(name);
        tokio::fs::write(&path, data).await.unwrap();
        let file = DbFile::new(
            &release_id,
            name,
            data.len() as i64,
            ContentType::Flac,
            Uuid::new_v4().to_string(),
            now,
        );
        mgr.add_external_file_for_test(&file, &path).await.unwrap();
        result.push((name.to_string(), data.to_vec()));
    }
    (album_id, release_id, result)
}

/// Get a Remote (cloud-only, not pinned) release whose files sit in `cloud`:
/// create it Local, attach its files, and finish the uploads.
async fn create_remote_release(
    mgr: &LibraryManager,
    source_dir: &Path,
    files: &[(&str, &[u8])],
) -> String {
    let (_album_id, release_id, named) = create_local_release(mgr, source_dir, files).await;
    mgr.upload_release(&release_id, false).await.unwrap();
    let count = mgr.drain_uploads_expecting_work().await.unwrap();
    assert_eq!(count, named.len(), "all files uploaded");
    assert_eq!(
        storage(mgr, &release_id).await,
        (ReleaseStorageState::Remote, false),
        "Remote (cloud-only) after the drain"
    );
    release_id
}

/// Drain a transfer receiver to its terminal event.
async fn collect_progress(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<TransferProgress>,
) -> Vec<TransferProgress> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        let terminal = matches!(
            event,
            TransferProgress::Complete { .. } | TransferProgress::Failed { .. }
        );
        events.push(event);
        if terminal {
            break;
        }
    }
    events
}

fn failed(events: &[TransferProgress]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, TransferProgress::Failed { .. }))
}

fn completed(events: &[TransferProgress]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, TransferProgress::Complete { .. }))
}

async fn expect_storage_state(
    subscription: &StorageBrowseSubscription,
    release_id: &str,
    expected: ReleaseStorageState,
) {
    loop {
        let value = tokio::time::timeout(std::time::Duration::from_secs(5), subscription.next())
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
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let source = tmp.path().join("src");
    let (_album_id, release_id, named) = create_local_release(
        &mgr,
        &source,
        &[("01.flac", b"aaaaaaaa"), ("02.flac", b"bbbbbbbbbbbb")],
    )
    .await;

    // Enqueue upload through the real coven path; do NOT drain.
    mgr.upload_release(&release_id, false).await.unwrap();

    assert_eq!(
        mgr.count_pending_uploads_for_release(&release_id)
            .await
            .unwrap(),
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
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (_a, release_id, _f) = create_local_release(&mgr, &tmp.path().join("src"), &[]).await;

    // No cover row yet → no bytes.
    assert!(
        support::read_cover_image_bytes(&mgr, &release_id)
            .await
            .is_none(),
        "no cover before one is stored"
    );

    // Store the cover bytes and `covers` row in one coven batch, exactly as
    // import / change_cover does.
    let bytes = b"cover-jpeg-bytes";
    mgr.store_library_image(
        &bae_core::db::DbLibraryImage {
            id: release_id.clone(),
            image_type: bae_core::db::LibraryImageType::Cover,
            content_type: bae_util::content_type::ContentType::Jpeg,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source: "local".to_string(),
            source_url: None,
            created_at: chrono::Utc::now(),
            revision: uuid::Uuid::new_v4().to_string(),
        },
        bytes,
    )
    .await
    .unwrap();

    // read_image_bytes serves it from coven's local store, byte-for-byte.
    let read = support::read_cover_image_bytes(&mgr, &release_id)
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
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_local_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    let events = TransferService::new(mgr.clone())
        .pin_release(release_id, |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");
    assert!(failed(&events), "pin must fail for a local release");
}

/// Unpin rejects a Local release: it has no remote files to drop from the cache.
#[tokio::test]
async fn test_unpin_rejects_local_release() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let (_a, release_id, _f) =
        create_local_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"a")]).await;

    assert!(
        mgr.unpin_release(&release_id).await.is_err(),
        "unpin must fail for a local release"
    );
}

// ---------------------------------------------------------------------------
// Pin / unpin success — through coven's cache
// ---------------------------------------------------------------------------

/// Pin a Remote cloud-only release: coven fetches its files from the cloud into
/// the pinned cache, so it reads as (Remote, pinned).
/// A release's detail carries its download control's state, read from its
/// entry in the download queue.
#[tokio::test]
async fn a_release_detail_says_where_its_download_stands() {
    use bae_core::album_detail::ReleaseDownloadStatus;
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let release_id =
        create_remote_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"bytes-a")]).await;
    let status = |mgr: LibraryManager, release_id: String| async move {
        mgr.find_release_detail(&release_id)
            .await
            .unwrap()
            .expect("the library holds the release")
            .download_status
    };
    assert_eq!(
        status(mgr.clone(), release_id.clone()).await,
        Some(ReleaseDownloadStatus::Available)
    );

    mgr.set_downloads_paused(true).await.unwrap();
    mgr.enqueue_pins(vec![release_id.clone()]).await.unwrap();
    assert_eq!(
        status(mgr.clone(), release_id.clone()).await,
        Some(ReleaseDownloadStatus::Queued)
    );
}

#[tokio::test]
async fn test_pin_remote_fetches_into_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &mgr,
        &tmp.path().join("src"),
        &[("a.flac", b"pin-bytes-a"), ("b.flac", b"pin-bytes-b")],
    )
    .await;

    let events = TransferService::new(mgr.clone())
        .pin_release(release_id.clone(), |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");
    assert!(completed(&events) && !failed(&events), "pin succeeds");
    assert!(
        matches!(events.first(), Some(TransferProgress::Started)),
        "pin starts"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            TransferProgress::Progress { progress }
                if progress.bytes_done == 0 && progress.bytes_total == 22
        )),
        "pin reports known byte total"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            TransferProgress::Progress { progress }
                if progress.bytes_done == 22 && progress.bytes_total == 22
        )),
        "pin reports completed bytes before completion"
    );
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, true)
    );
}

/// The release and album detail views show a release pinned once its files
/// are, and unpinned again once they aren't, each from its own live read.
#[tokio::test]
async fn the_detail_views_follow_a_releases_pin() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let release_id = create_remote_release(
        &mgr,
        &tmp.path().join("src"),
        &[("a.flac", b"follow-bytes")],
    )
    .await;
    let album_id = mgr
        .get_release_by_id(&release_id)
        .await
        .unwrap()
        .expect("the library holds the release")
        .album_id;
    let services = AppServices::for_test(mgr.clone()).await.unwrap();
    let release = services.subscribe_release_detail();
    release.set(Some(release_id.clone())).unwrap();
    let album = services.subscribe_album_detail();
    album.set(Some(album_id)).unwrap();
    let shown = |pinned: bool| {
        let (release, album) = (&release, &album);
        async move {
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
                loop {
                    let value = release.next().await.expect("the release detail resolves");
                    if value
                        .value
                        .is_some_and(|detail| detail.summary.pinned == pinned)
                    {
                        break;
                    }
                }
                loop {
                    let value = album.next().await.expect("the album detail resolves");
                    if value
                        .value
                        .is_some_and(|detail| detail.releases[0].summary.pinned == pinned)
                    {
                        break;
                    }
                }
            })
            .await
            .unwrap_or_else(|_| panic!("the detail views never showed pinned = {pinned}"));
        }
    };
    shown(false).await;

    TransferService::new(mgr.clone())
        .pin_release(release_id.clone(), |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");
    shown(true).await;

    mgr.unpin_release(&release_id).await.unwrap();
    shown(false).await;
}

/// Unpin a Remote pinned release: coven moves its files out of the pinned cache
/// into the evictable cache, so it reads as (Remote, not pinned).
#[tokio::test]
async fn test_unpin_remote_drops_from_pinned_cache() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let release_id =
        create_remote_release(&mgr, &tmp.path().join("src"), &[("a.flac", b"unpin-bytes")]).await;

    TransferService::new(mgr.clone())
        .pin_release(release_id.clone(), |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, true)
    );

    mgr.unpin_release(&release_id).await.unwrap();
    assert_eq!(
        storage(&mgr, &release_id).await,
        (ReleaseStorageState::Remote, false)
    );
}

// ---------------------------------------------------------------------------
// read_release_file_bytes surfaces a missing original as an error
// ---------------------------------------------------------------------------

/// Reading a Local release whose original file vanished returns an error.
#[tokio::test]
async fn test_missing_external_source_maps_to_error() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _f) =
        create_local_release(&mgr, &source_dir, &[("a.flac", b"present")]).await;
    let file = mgr
        .get_files_for_release(&release_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    // The user moved or deleted their original file.
    tokio::fs::remove_file(source_dir.join("a.flac"))
        .await
        .unwrap();

    let result = read_release_file_bytes(&file, &mgr).await;
    assert!(
        result.is_err(),
        "a vanished external source must surface a read error"
    );
}

// ---------------------------------------------------------------------------
// Transition completions deliver new storage values
// ---------------------------------------------------------------------------

/// A drained upload and audio detachment both change the subscribed Storage
/// Manager value without a separate library event or another read from the UI.
#[tokio::test]
async fn transition_completions_deliver_storage_values() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let source_dir = tmp.path().join("src");
    let (_a, release_id, _named) =
        create_local_release(&mgr, &source_dir, &[("a.flac", b"round-trip-bytes")]).await;
    let services = AppServices::for_test(mgr.clone()).await.unwrap();
    let values = services.subscribe_storage_browse(StorageBrowseView {
        sort: StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        },
        filter: StorageFilter::All,
        windows: [bae_core::library::LibraryPageWindow {
            offset: 0,
            limit: 50,
        }]
        .into_iter()
        .collect(),
        selected: Default::default(),
    });
    expect_storage_state(&values, &release_id, ReleaseStorageState::Local).await;

    mgr.upload_release(&release_id, false).await.unwrap();
    mgr.drain_uploads_expecting_work().await.unwrap();
    expect_storage_state(&values, &release_id, ReleaseStorageState::Remote).await;

    let dest = tmp.path().join("brought-back");
    mgr.detach_release_audio_into(
        &release_id,
        dest.to_str().unwrap(),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    expect_storage_state(&values, &release_id, ReleaseStorageState::Local).await;
}

/// The Storage Manager's inspector shows, and its actions act on, a release
/// only while the list holds it: the list answers with the row of each
/// selected release it holds wherever it sits, in a window or not, and a
/// selected release whose upload finishes leaves the Uploading list and the
/// answer together.
#[tokio::test]
async fn selected_releases_leave_the_uploading_list_with_their_upload() {
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let (_a, first, _) =
        create_local_release(&mgr, &tmp.path().join("first"), &[("a.flac", b"first")]).await;
    let (_b, second, _) =
        create_local_release(&mgr, &tmp.path().join("second"), &[("b.flac", b"second")]).await;
    mgr.upload_release(&first, false).await.unwrap();
    mgr.upload_release(&second, false).await.unwrap();

    let services = AppServices::for_test(mgr.clone()).await.unwrap();
    let selected: std::collections::BTreeSet<String> =
        [first.clone(), second.clone()].into_iter().collect();
    let values = services.subscribe_storage_browse(StorageBrowseView {
        sort: StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        },
        filter: StorageFilter::Uploading,
        // One row on screen, so one selected release sits outside it.
        windows: [bae_core::library::LibraryPageWindow {
            offset: 0,
            limit: 1,
        }]
        .into_iter()
        .collect(),
        selected: selected.clone(),
    });
    let next = || async {
        tokio::time::timeout(std::time::Duration::from_secs(5), values.next())
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

    mgr.drain_uploads_expecting_work().await.unwrap();
    loop {
        let value = next().await;
        if value.total_count == 0 {
            assert!(
                value.selected.is_empty(),
                "an uploaded release is no longer in the Uploading list"
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
    tracing_init();
    let tmp = TempDir::new().unwrap();
    let mgr = setup_with_cloud(&tmp).await;
    let pinned =
        create_remote_release(&mgr, &tmp.path().join("pinned"), &[("a.flac", b"pinned")]).await;
    let unpinned = create_remote_release(
        &mgr,
        &tmp.path().join("unpinned"),
        &[("b.flac", b"unpinned")],
    )
    .await;
    TransferService::new(mgr.clone())
        .pin_release(pinned.clone(), |progress| async move {
            Ok(collect_progress(progress).await)
        })
        .finish()
        .await
        .expect("pin task completes");

    let services = AppServices::for_test(mgr.clone()).await.unwrap();
    let view = |selected: &[&String]| StorageBrowseView {
        sort: StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        },
        filter: StorageFilter::All,
        // One row on screen, so one selected release sits outside it.
        windows: [bae_core::library::LibraryPageWindow {
            offset: 0,
            limit: 1,
        }]
        .into_iter()
        .collect(),
        selected: selected.iter().map(|id| (*id).clone()).collect(),
    };
    let values = services.subscribe_storage_browse(view(&[&pinned, &unpinned]));
    let next = || async {
        tokio::time::timeout(std::time::Duration::from_secs(5), values.next())
            .await
            .expect("storage subscription delivers a value")
            .expect("storage projection resolves")
    };

    use bae_core::album_detail::ReleaseStorageAction::{DetachAudio, Pin};
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
