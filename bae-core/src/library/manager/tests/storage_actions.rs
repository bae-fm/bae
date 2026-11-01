//! A release's storage transition is its `release_storage_action` row: the
//! storage and detail reads show it from the row, and the row goes however
//! the transition's work ends.

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

/// A transition's row is there while its work runs and gone once the work
/// ends, whether it succeeds, fails or panics.
#[tokio::test]
async fn a_transition_ends_however_its_work_ends() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let shown = || async {
        manager
            .database
            .release_transfer_action(&release.id)
            .await
            .unwrap()
    };

    let ran = manager
        .run_storage_action(&release.id, StoredTransition::DetachAudio, async {
            Ok(shown().await)
        })
        .await
        .unwrap();
    assert_eq!(
        ran,
        Some(ReleaseStorageAction::DetachAudio),
        "the row is there while it runs"
    );
    assert_eq!(shown().await, None, "a success ends it");

    let failed = manager
        .run_storage_action(&release.id, StoredTransition::Unpin, async {
            Err::<(), _>(LibraryError::Storage("the work failed".into()))
        })
        .await;
    assert!(
        matches!(failed, Err(LibraryError::Storage(_))),
        "{failed:?}"
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
