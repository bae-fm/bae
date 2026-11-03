//! A release's storage transition is its `release_storage_action` row: the
//! storage and detail reads show it from the row, and the row goes in the
//! write that finishes the transition's work, or after a failure.

use super::*;
use bae_transfers::StoredTransition;
use bae_types::release_storage::ReleaseStorageAction;

const WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// A transition's row is there while its work runs; a work that fails or
/// panics has it deleted in a write of its own. A work that succeeds deletes
/// it in the write that finishes it, as the unpin and make-local tests below
/// show.
#[tokio::test]
async fn a_failed_transition_ends_in_a_write_of_its_own() {
    let (manager, _temp_dir, _album, release) = manager_with_release().await;
    let shown = || async {
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id,
        )
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
    assert!(failure.ends_with("Some(Unpin)"), "the row is there while it runs: {failure}");
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
        || async { file_pinned(&manager, bae_db::synced::RELEASE_FILES_NAMESPACE, &file.id).await };
    assert!(pinned().await);

    let refused = bae_transfers::release_storage_action::unpin_release(
        &manager.database,
        &release.id,
        vec![file.file],
    )
    .await;
    assert!(refused.is_err(), "no unpin was admitted for the write to end");
    assert!(pinned().await, "a write that can't end the transition releases no pin");

    manager.unpin_release(&release.id).await.unwrap();
    assert!(!pinned().await, "the unpin released the pin");
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
        .await
        .unwrap(),
        None,
        "and ended the transition"
    );
}

/// Making a release local switches its files in the write that ends its
/// transition: a write that can't end it switches nothing, so the release
/// never shows the transition once its files are here.
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
            let path =
                bae_db::stored_name::file_path(&release.id, &file.id, &file.original_filename)
                    .unwrap();
            (file.file, path)
        })
        .collect();
    let (_stop, stopped) = tokio::sync::watch::channel(false);
    let prepared = manager
        .database
        .prepare_device_only(&refused_folder, paths, &stopped, |_| {})
        .await
        .unwrap();
    let refused = bae_transfers::release_storage_action::make_release_local(
        &manager.database,
        &release.id,
        prepared,
    )
    .await;
    assert!(refused.is_err(), "no make-local was admitted for the write to end");
    assert!(
        release_files(&manager, &release.id).await.iter().all(|file| file.stored),
        "a write that can't end the transition switches no file"
    );

    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    manager
        .detach_release_audio_into(&release.id, &folder, &crate::CancellationToken::new())
        .await
        .unwrap();
    assert!(
        release_files(&manager, &release.id).await.iter().all(|file| !file.stored),
        "making it local switched every file here"
    );
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
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
    assert!(bae_transfers::release_storage_action::begin_release_storage_action(
        &manager.database,
        &release.id,
        StoredTransition::Unpin
    )
    .await
    .unwrap());

    let ran = std::sync::atomic::AtomicBool::new(false);
    let refused = manager
        .run_storage_action(&release.id, StoredTransition::DetachAudio, async {
            ran.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await;
    assert!(matches!(refused, Err(LibraryError::Storage(_))), "{refused:?}");
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst), "the refused work never runs");
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
        .await
        .unwrap(),
        Some(ReleaseStorageAction::Unpin)
    );
}

/// A move to the cloud shows as a transition until the write that queues its
/// files, which ends it; one whose files can't be queued ends it in a write
/// of its own.
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
        bae_transfers::release_storage_action::release_transfer_action(
            &manager.database,
            &release.id
        )
        .await
        .unwrap(),
        None
    );

    let empty = insert_release_without_files(&manager).await;
    let refused = manager.upload_release(&empty, false).await;
    assert!(refused.is_err(), "a release with no files has none to queue");
    assert_eq!(
        bae_transfers::release_storage_action::release_transfer_action(&manager.database, &empty)
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

/// While a release's copies download into the person's folder, the release
/// is listed with the bytes of the file coven downloads and of the whole
/// release; once its files have switched it is listed no more.
#[tokio::test]
async fn a_release_made_local_lists_its_bytes_while_its_copies_download() {
    let (manager, temp_dir) = setup_test_manager().await;
    let home = connect_test_cloud(&manager).await;
    let release_id =
        uploaded_release(&manager, &temp_dir.path().join("made-local"), "Made Local", false).await;
    let file = release_files(&manager, &release_id).await.remove(0);
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
            manager.database.store_id_for_test(),
            location.uploader(),
            location.object(),
        ),
        12,
        reached,
        resumed,
    )
    .await;
    let mut transfers = manager.subscribe_make_local_transfers();
    let folder = temp_dir.path().join("local");
    std::fs::create_dir(&folder).unwrap();
    let making = tokio::spawn({
        let manager = manager.library.clone();
        let release_id = release_id.clone();
        async move {
            manager
                .detach_release_audio_into(&release_id, &folder, &crate::CancellationToken::new())
                .await
        }
    });
    observed.await.unwrap();
    let listed = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(progress) = transfers.borrow_and_update().get(&release_id).copied() {
                return progress;
            }
            transfers.changed().await.unwrap();
        }
    })
    .await
    .expect("the release is listed while its copy downloads");
    assert!(listed.file_bytes_total > 0, "{listed:?}");
    assert!(listed.file_bytes_done < listed.file_bytes_total, "{listed:?}");
    assert!(listed.bytes_done < listed.bytes_total, "{listed:?}");
    assert!(listed.bytes_total >= listed.file_bytes_total, "{listed:?}");

    resume.send(()).unwrap();
    making.await.unwrap().unwrap();
    assert!(transfers.borrow().is_empty(), "a release whose files switched is listed no more");
}
