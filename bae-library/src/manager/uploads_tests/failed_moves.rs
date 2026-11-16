//! These tests check what the pending list and the release's storage row
//! show when a move fails because an original file changed, and what Cancel
//! does then.

use super::*;

/// An original that changes before its upload fails its release's move: the
/// pending list names the file and offers Cancel upload, which ends the move,
/// and the release stays on this device only.
#[tokio::test]
async fn a_changed_original_fails_the_move_until_its_upload_is_cancelled() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("changed").await;
    std::fs::write(temp.path().join("changed/a.flac"), b"changed after the move").unwrap();
    manager.library.manager().drain_uploads_expecting_work().await.unwrap();
    manager.library.manager().start().unwrap();
    let mut values = manager.library.manager().subscribe_sync_status();
    let listed = pending_list_until(&mut values, |items| {
        items.iter().any(|item| {
            matches!(item.wait.action, bae_sync_status::PendingAction::CancelUpload { .. })
        })
    })
    .await;
    let item = listed
        .iter()
        .find(|item| {
            matches!(item.wait.action, bae_sync_status::PendingAction::CancelUpload { .. })
        })
        .unwrap();
    assert_eq!(
        item.subject,
        bae_sync_status::PendingSubjectName::ReleaseFile {
            file_name: "a.flac".into(),
            album_title: "Test Album".into(),
        }
    );
    let bae_sync_status::PendingAction::CancelUpload { id } = item.wait.action.clone() else {
        unreachable!("found by its action");
    };
    manager.library.manager().cancel_failed_release_move(id).await.unwrap();
    for file in release_files(manager.library.manager(), &release.id).await {
        assert!(!file.stored, "the release stays on this device");
    }
    assert_eq!(
        manager.library.manager().count_pending_uploads_for_release(&release.id).await.unwrap(),
        0
    );
    pending_list_until(&mut values, |items| {
        !items.iter().any(|item| {
            matches!(item.wait.action, bae_sync_status::PendingAction::CancelUpload { .. })
        })
    })
    .await;
}

/// When a move fails because an original changed, the release's storage row
/// shows Move failed with the file's name, and the row sorts first. The
/// row's Cancel (the release's transition cancel) ends the move, and the row
/// then shows the release as stored on this device only.
#[tokio::test]
async fn a_failed_move_shows_on_the_storage_row_until_cancelled() {
    use bae_views::album_detail::StorageRowStatus;
    let (manager, temp, release, _file_id) = queued_upload_fixture("failed-row").await;
    let other = insert_local_release_with_files(
        manager.library.manager(),
        &temp.path().join("other"),
        "Another Album",
        &[("b.flac", b"b")],
    )
    .await;
    std::fs::write(temp.path().join("failed-row/a.flac"), b"changed after the move").unwrap();
    manager.library.manager().drain_uploads_expecting_work().await.unwrap();
    let statuses = || async {
        let sort = bae_views::StorageSortCriterion {
            field: bae_views::StorageSortField::Storage,
            direction: bae_store::SortDirection::Ascending,
        };
        views(manager.library.manager())
            .get_storage_page(&sort, bae_views::StorageFilter::All, 0, 10)
            .await
            .unwrap()
            .rows
            .into_iter()
            .map(|row| (row.release.id, row.status))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        statuses().await,
        vec![
            (release.id.clone(), StorageRowStatus::MoveFailed { file_name: "a.flac".into() }),
            (other.id.clone(), StorageRowStatus::OnOneDevice),
        ]
    );
    manager.library.manager().cancel_release_transition(&release.id).await.unwrap();
    let rested = statuses().await;
    assert_eq!(rested.len(), 2);
    assert!(
        rested.iter().all(|(_, status)| *status == StorageRowStatus::OnOneDevice),
        "both releases are stored on this device only: {rested:?}"
    );
}

/// bae reads the failed move and the changed file's name from the move's
/// pending record in one query. The storage row names the file while the
/// record exists, and names no file after Cancel deletes the record.
#[tokio::test]
async fn a_failed_moves_storage_row_names_the_file_its_pending_record_names() {
    let (manager, temp, release, _file_id) = queued_upload_fixture("failed-file").await;
    let failed_file = || async {
        let release_id = release.id.clone();
        manager
            .library
            .manager()
            .store
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT failed_move_file FROM release_storage WHERE release_id = ?1",
                    [release_id],
                    |row| row.get::<_, Option<String>>(0),
                )?)
            })
            .await
            .unwrap()
    };
    assert_eq!(failed_file().await, None);
    std::fs::write(temp.path().join("failed-file/a.flac"), b"changed after the move").unwrap();
    manager.library.manager().drain_uploads_expecting_work().await.unwrap();
    assert_eq!(failed_file().await.as_deref(), Some("a.flac"));
    manager.library.manager().cancel_release_transition(&release.id).await.unwrap();
    assert_eq!(failed_file().await, None);
}
