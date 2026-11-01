//! A release's storage transition is its `release_storage_action` row: the
//! storage and detail reads show it from the row, and the row goes in the
//! write that finishes the transition's work, or after a failure.

use super::*;
use crate::album_detail::{ReleaseStorageAction, StorageRowStatus};
use crate::db::StoredTransition;

const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Pull `next` until `wanted` holds of a value, failing after [`WAIT`].
async fn until<Value, Error: std::fmt::Debug>(
    what: &str,
    mut next: impl AsyncFnMut() -> Result<Value, Error>,
    wanted: impl Fn(&Value) -> bool,
) {
    tokio::time::timeout(WAIT, async {
        loop {
            if wanted(&next().await.unwrap()) {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what}"));
}

/// Beginning and ending a release's transition reaches the album detail,
/// the release detail and the Storage Manager, each read from the row: while
/// it runs the release shows it and offers no storage action.
#[cfg(feature = "test-utils")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transition_reaches_the_reads_from_its_row() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    let services = crate::library::AppServices::for_test(manager.library.clone())
        .await
        .unwrap();
    let album_detail = services.subscribe_album_detail();
    album_detail.set(Some(album.id.clone())).unwrap();
    let release_detail = services.subscribe_release_detail();
    release_detail.set(Some(release.id.clone())).unwrap();
    let storage = services.subscribe_storage_browse(crate::library::StorageBrowseView {
        sort: crate::db::StorageSortCriterion {
            field: crate::db::StorageSortField::AlbumTitle,
            direction: crate::db::SortDirection::Ascending,
        },
        filter: crate::db::StorageFilter::All,
        search: String::new(),
        windows: [crate::library::LibraryPageWindow {
            offset: 0,
            limit: 50,
        }]
        .into_iter()
        .collect(),
        selected: Default::default(),
    });
    let album_shows = |action: Option<ReleaseStorageAction>| {
        move |snapshot: &crate::library::DetailSnapshot<crate::album_detail::AlbumDetail>| {
            snapshot.value.as_ref().is_some_and(|detail| {
                detail.releases[0].summary.transfer_action == action
                    && detail.releases[0].summary.transitioning == action.is_some()
            })
        }
    };
    let release_shows = |action: Option<ReleaseStorageAction>| {
        move |snapshot: &crate::library::DetailSnapshot<crate::album_detail::ReleaseDetail>| {
            snapshot
                .value
                .as_ref()
                .is_some_and(|detail| detail.summary.transfer_action == action)
        }
    };
    let row_shows = |status: StorageRowStatus| {
        move |snapshot: &crate::library::StorageBrowseSnapshot| {
            snapshot
                .windows
                .iter()
                .flat_map(|window| &window.rows)
                .any(|row| row.status == status)
        }
    };

    until(
        "the album shows no transition",
        || album_detail.next(),
        album_shows(None),
    )
    .await;
    until(
        "the release shows no transition",
        || release_detail.next(),
        release_shows(None),
    )
    .await;
    until(
        "the row rests",
        || storage.next(),
        row_shows(StorageRowStatus::Local),
    )
    .await;

    assert!(manager
        .database
        .begin_release_storage_action(&release.id, StoredTransition::Unpin)
        .await
        .unwrap());
    let unpinning = Some(ReleaseStorageAction::Unpin);
    until(
        "the album shows the transition",
        || album_detail.next(),
        album_shows(unpinning),
    )
    .await;
    until(
        "the release shows the transition",
        || release_detail.next(),
        release_shows(unpinning),
    )
    .await;
    until(
        "the row shows the transition",
        || storage.next(),
        row_shows(StorageRowStatus::Transferring(ReleaseStorageAction::Unpin)),
    )
    .await;

    manager
        .database
        .end_release_storage_action(&release.id)
        .await
        .unwrap();
    until(
        "the album's transition ends",
        || album_detail.next(),
        album_shows(None),
    )
    .await;
    until(
        "the release's transition ends",
        || release_detail.next(),
        release_shows(None),
    )
    .await;
    until(
        "the row rests again",
        || storage.next(),
        row_shows(StorageRowStatus::Local),
    )
    .await;

    drop((album_detail, release_detail, storage));
    tokio::task::spawn_blocking(move || drop(services))
        .await
        .expect("tear the application down");
}

/// A transition's row is there while its work runs; a work that fails or
/// panics has it deleted in a write of its own. A work that succeeds deletes
/// it in the write that finishes it, as the unpin and make-local tests below
/// show.
#[tokio::test]
async fn a_failed_transition_ends_in_a_write_of_its_own() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let shown = || async {
        manager
            .database
            .release_transfer_action(&release.id)
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
    assert!(
        failure.ends_with("Some(Unpin)"),
        "the row is there while it runs: {failure}"
    );
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
#[cfg(feature = "test-utils")]
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
        || async { file_pinned(&manager, crate::sync::RELEASE_FILES_NAMESPACE, &file.id).await };
    assert!(pinned().await);

    let refused = manager
        .database
        .unpin_release(&release.id, vec![file.file])
        .await;
    assert!(
        refused.is_err(),
        "no unpin was admitted for the write to end"
    );
    assert!(
        pinned().await,
        "a write that can't end the transition releases no pin"
    );

    manager.unpin_release(&release.id).await.unwrap();
    assert!(!pinned().await, "the unpin released the pin");
    assert_eq!(
        manager
            .database
            .release_transfer_action(&release.id)
            .await
            .unwrap(),
        None,
        "and ended the transition"
    );
}

/// Making a release local switches its files in the write that ends its
/// transition: a write that can't end it switches nothing, so the release
/// never shows the transition once its files are here.
#[cfg(feature = "test-utils")]
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
            let path = crate::storage::stored_name::file_path(
                &release.id,
                &file.id,
                &file.original_filename,
            )
            .unwrap();
            (file.file, path)
        })
        .collect();
    let (_stop, stopped) = tokio::sync::watch::channel(false);
    let prepared = manager
        .database
        .prepare_device_only(&refused_folder, paths, &stopped)
        .await
        .unwrap();
    let refused = manager
        .database
        .make_release_local(&release.id, prepared)
        .await;
    assert!(
        refused.is_err(),
        "no make-local was admitted for the write to end"
    );
    assert!(
        release_files(&manager, &release.id)
            .await
            .iter()
            .all(|file| file.stored),
        "a write that can't end the transition switches no file"
    );

    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    manager
        .detach_release_audio_into(
            &release.id,
            &folder,
            &crate::library::CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(
        release_files(&manager, &release.id)
            .await
            .iter()
            .all(|file| !file.stored),
        "making it local switched every file here"
    );
    assert_eq!(
        manager
            .database
            .release_transfer_action(&release.id)
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
    assert!(manager
        .database
        .begin_release_storage_action(&release.id, StoredTransition::Unpin)
        .await
        .unwrap());

    let ran = std::sync::atomic::AtomicBool::new(false);
    let refused = manager
        .run_storage_action(&release.id, StoredTransition::DetachAudio, async {
            ran.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await;
    assert!(
        matches!(refused, Err(LibraryError::Storage(_))),
        "{refused:?}"
    );
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "the refused work never runs"
    );
    assert_eq!(
        manager
            .database
            .release_transfer_action(&release.id)
            .await
            .unwrap(),
        Some(ReleaseStorageAction::Unpin)
    );
}

/// A move to the cloud shows as a transition until the write that queues its
/// files, which ends it; one whose files can't be queued ends it in a write
/// of its own.
#[cfg(feature = "test-utils")]
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
        manager
            .database
            .release_transfer_action(&release.id)
            .await
            .unwrap(),
        None
    );

    let empty = insert_release_without_files(&manager).await;
    let refused = manager.upload_release(&empty, false).await;
    assert!(
        refused.is_err(),
        "a release with no files has none to queue"
    );
    assert_eq!(
        manager
            .database
            .release_transfer_action(&empty)
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
