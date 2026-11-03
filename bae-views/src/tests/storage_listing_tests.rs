//! The Storage Manager list: its counts, sizes and sorts as release files
//! change, and every storage sort read from an index.

use crate::*;
use bae_db::test_fixtures::exec_batch;
use bae_db::test_fixtures::storage_listing::*;

/// The Storage Manager list's release ids in `field` order, ascending.
async fn page(db: &Database, field: StorageSortField) -> Vec<String> {
    crate::LibraryViews::new(db.clone())
        .get_storage_page(
            &StorageSortCriterion { field, direction: SortDirection::Ascending },
            StorageFilter::All,
            0,
            50,
        )
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|row| row.release.id)
        .collect()
}

#[tokio::test]
async fn the_storage_list_counts_and_sorts_by_files_as_they_change() {
    let (db, tmp) = bae_db::test_fixtures::temp_db().await;
    exec_batch(&db, LIBRARY).await;
    // The first release has two files, of 10 and 20 bytes; the second none.
    for file in [
        ("d3edb846-9b0b-4a64-b0f6-3f117b686bc0", RELEASE_EARLY, "cover.jpg", 10),
        ("161dd6f3-3bdd-4312-b5d3-8b9d83b53694", RELEASE_EARLY, "back.jpg", 20),
    ] {
        insert_file(&db, tmp.path(), file).await;
    }
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();

    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::All).await.unwrap(),
        2
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::Local).await.unwrap(),
        2
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_count(StorageFilter::Remote)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        30
    );
    assert_eq!(page(&db, StorageSortField::FileCount).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    exec_batch(&db, "DELETE FROM release_files WHERE id = '161dd6f3-3bdd-4312-b5d3-8b9d83b53694';")
        .await;
    for file in [
        ("f5008044-8ffe-4227-bdc3-238f85a6a4a6", RELEASE_LATE, "one.jpg", 1),
        ("cda4c2ec-c6fa-422c-a224-d4f56ca5f3f3", RELEASE_LATE, "two.jpg", 2),
    ] {
        insert_file(&db, tmp.path(), file).await;
    }
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        13
    );
    assert_eq!(page(&db, StorageSortField::FileCount).await, ids(&[RELEASE_EARLY, RELEASE_LATE]));
    assert_eq!(page(&db, StorageSortField::TotalSize).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    // Retitling the first album sorts its release after the second.
    exec_batch(
        &db,
        "UPDATE albums SET title = 'Album Title C' WHERE id = '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d';",
    )
    .await;
    assert_eq!(page(&db, StorageSortField::AlbumTitle).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    exec_batch(&db, "DELETE FROM releases WHERE id = 'd439555e-9883-4037-a580-48ea33c2c57b';")
        .await;
    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::All).await.unwrap(),
        1
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        3
    );
}

/// Sorting by storage reads a window from an index of the listing in either
/// direction, under every filter: SQLite never sorts the list itself.
#[tokio::test]
async fn the_storage_sort_reads_a_window_from_an_index() {
    let (db, _tmp) = bae_db::test_fixtures::temp_db().await;
    for filter in [StorageFilter::All, StorageFilter::Local, StorageFilter::Remote] {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let sort = StorageSortCriterion { field: StorageSortField::Storage, direction };
            let order = crate::storage::storage_order(&sort);
            let steps = plan(
                &db,
                crate::storage::storage_window_query(
                    &order,
                    filter,
                    crate::storage::StorageRows::Listed,
                    &order.everything(),
                )
                .replace("LIMIT :limit OFFSET :offset", "LIMIT 50 OFFSET 0"),
            )
            .await;
            assert!(
                steps.iter().any(|step| step.contains("USING INDEX idx_release_listing_")
                    && step.contains("storage")),
                "{filter:?} {direction:?}: {steps:?}"
            );
            assert!(
                !steps.iter().any(|step| step.contains("TEMP B-TREE FOR ORDER BY")),
                "{filter:?} {direction:?}: {steps:?}"
            );
        }
    }
}
