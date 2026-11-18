//! These tests check what the release's storage row shows when a move fails
//! because an original file changed, and what Cancel does then.

use super::*;

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

/// Moving a selection moves each release in a write of its own. A release
/// whose earlier move failed is still in that move, so coven refuses to move
/// it again: the other releases move, and the outcome names that release with
/// coven's reason.
#[tokio::test]
async fn a_release_in_a_failed_move_is_named_and_the_rest_of_the_selection_moves() {
    let (manager, temp, failed, _file_id) = queued_upload_fixture("failed").await;
    let library = manager.library.manager();
    std::fs::write(temp.path().join("failed/a.flac"), b"changed after the move").unwrap();
    library.drain_uploads_expecting_work().await.unwrap();
    let first = insert_local_release_with_files(
        library,
        &temp.path().join("first"),
        "First Album",
        &[("b.flac", b"b")],
    )
    .await;
    let last = insert_local_release_with_files(
        library,
        &temp.path().join("last"),
        "Last Album",
        &[("c.flac", b"c")],
    )
    .await;

    let outcome = library
        .upload_releases(&[first.id.clone(), failed.id.clone(), last.id.clone()])
        .await
        .expect("the selection returns each release's outcome");

    let UploadReleasesOutcome::Partial { receipt: Some(receipt), failure } = outcome else {
        panic!("the other two releases move and the one in a failed move does not");
    };
    assert_eq!(receipt.release_ids, vec![first.id.clone(), last.id.clone()]);
    assert_eq!(failure.release_ids, vec![failed.id.clone()]);
    let bae_diagnostics::UiError::Diagnostic { detail, .. } = failure.error;
    assert!(
        detail.starts_with(&format!("release {}: ", failed.id)) && detail.contains(" is in move "),
        "the failure gives coven's reason: {detail}"
    );
    for release in [&first, &last] {
        assert_eq!(moving_file_count(library, &release.id).await, 1);
    }
}
