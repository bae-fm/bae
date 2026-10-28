use super::super::*;

const RELEASE_EARLY: &str = "d439555e-9883-4037-a580-48ea33c2c57b";
const RELEASE_LATE: &str = "af4421de-5174-45a9-81fc-74110bc5f85d";

/// Two releases on albums titled to sort apart, the first with two files of
/// 10 and 20 bytes, the second with none.
const LIBRARY: &str = "
    INSERT INTO albums (id, title, is_compilation, created_at) VALUES
        ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z'),
        ('dab6efe2-9b5c-4034-b95e-73cf49f54b9a', 'Album Title B', 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0),
        ('af4421de-5174-45a9-81fc-74110bc5f85d', 'dab6efe2-9b5c-4034-b95e-73cf49f54b9a', '2026-01-01T00:00:00Z', 0);
    INSERT INTO release_files (id, release_id, original_filename, file_size, content_type, created_at) VALUES
        ('d3edb846-9b0b-4a64-b0f6-3f117b686bc0', 'd439555e-9883-4037-a580-48ea33c2c57b', 'cover.jpg', 10, 'image/jpeg', '2026-01-01T00:00:00Z'),
        ('161dd6f3-3bdd-4312-b5d3-8b9d83b53694', 'd439555e-9883-4037-a580-48ea33c2c57b', 'back.jpg', 20, 'image/jpeg', '2026-01-01T00:00:00Z');
";

async fn exec(db: &Database, sql: &'static str) {
    db.call(move |conn| conn.execute_batch(sql).map_err(DbError::from))
        .await
        .unwrap();
}

/// The Storage Manager list's release ids in `field` order, ascending.
async fn page(db: &Database, field: StorageSortField) -> Vec<String> {
    db.get_storage_page(
        &StorageSortCriterion {
            field,
            direction: SortDirection::Ascending,
        },
        StorageFilter::All,
        0,
        50,
    )
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.release.id)
    .collect()
}

#[tokio::test]
async fn the_storage_list_counts_and_sorts_by_files_as_they_change() {
    let (db, _tmp) = super::temp_db().await;
    exec(&db, LIBRARY).await;
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();

    assert_eq!(db.get_storage_count(StorageFilter::All).await.unwrap(), 2);
    assert_eq!(db.get_storage_count(StorageFilter::Local).await.unwrap(), 2);
    assert_eq!(
        db.get_storage_count(StorageFilter::Remote).await.unwrap(),
        0
    );
    assert_eq!(
        db.get_storage_total_size(StorageFilter::All).await.unwrap(),
        30
    );
    assert_eq!(
        page(&db, StorageSortField::FileCount).await,
        ids(&[RELEASE_LATE, RELEASE_EARLY])
    );

    exec(
        &db,
        "DELETE FROM release_files WHERE id = '161dd6f3-3bdd-4312-b5d3-8b9d83b53694';
         INSERT INTO release_files (id, release_id, original_filename, file_size, content_type, created_at) VALUES
            ('f5008044-8ffe-4227-bdc3-238f85a6a4a6', 'af4421de-5174-45a9-81fc-74110bc5f85d', 'one.jpg', 1, 'image/jpeg', '2026-01-01T00:00:00Z'),
            ('cda4c2ec-c6fa-422c-a224-d4f56ca5f3f3', 'af4421de-5174-45a9-81fc-74110bc5f85d', 'two.jpg', 2, 'image/jpeg', '2026-01-01T00:00:00Z');",
    )
    .await;
    assert_eq!(
        db.get_storage_total_size(StorageFilter::All).await.unwrap(),
        13
    );
    assert_eq!(
        page(&db, StorageSortField::FileCount).await,
        ids(&[RELEASE_EARLY, RELEASE_LATE])
    );
    assert_eq!(
        page(&db, StorageSortField::TotalSize).await,
        ids(&[RELEASE_LATE, RELEASE_EARLY])
    );

    // Retitling the first album sorts its release after the second.
    exec(
        &db,
        "UPDATE albums SET title = 'Album Title C' WHERE id = '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d';",
    )
    .await;
    assert_eq!(
        page(&db, StorageSortField::AlbumTitle).await,
        ids(&[RELEASE_LATE, RELEASE_EARLY])
    );

    exec(
        &db,
        "DELETE FROM releases WHERE id = 'd439555e-9883-4037-a580-48ea33c2c57b';",
    )
    .await;
    assert_eq!(db.get_storage_count(StorageFilter::All).await.unwrap(), 1);
    assert_eq!(
        db.get_storage_total_size(StorageFilter::All).await.unwrap(),
        3
    );
}

/// The plan SQLite chooses for `statement`, one detail line per step.
async fn plan(db: &Database, statement: String) -> Vec<String> {
    db.read(move |sql| {
        sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), [], |row| {
            row.get::<_, String>(3)
        })
        .map_err(DbError::from)
    })
    .await
    .unwrap()
}

/// The cloud-only count reads the releases that count from an index of the
/// listing, and asks nothing of their files or coven's pins.
#[tokio::test]
async fn the_cloud_only_count_reads_one_index_of_the_listing() {
    let (db, _tmp) = super::temp_db().await;
    assert_eq!(
        plan(
            &db,
            super::super::query::CLOUD_ONLY_RELEASE_COUNT.to_string()
        )
        .await,
        vec![
            "SEARCH listing USING COVERING INDEX idx_release_listing_cloud_only (stored=?)"
                .to_string()
        ]
    );
}

/// The releases uploading are read in queue order from an index of the
/// listing that holds every column read, not from coven's queue grouped by
/// release.
#[tokio::test]
async fn the_uploading_releases_read_the_listing_in_queue_order() {
    let (db, _tmp) = super::temp_db().await;
    assert_eq!(
        plan(
            &db,
            format!(
                "SELECT release_id FROM ({}) ORDER BY position",
                super::super::query::UPLOADING_RELEASES
            )
        )
        .await,
        vec!["SEARCH uploading USING COVERING INDEX idx_release_listing_uploading (first_queued_position>?)".to_string()]
    );
}

/// Sorting by storage reads a window from an index of the listing in either
/// direction, under every filter: SQLite never sorts the list itself.
#[tokio::test]
async fn the_storage_sort_reads_a_window_from_an_index() {
    let (db, _tmp) = super::temp_db().await;
    for filter in [
        StorageFilter::All,
        StorageFilter::Local,
        StorageFilter::Remote,
    ] {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let sort = StorageSortCriterion {
                field: StorageSortField::Storage,
                direction,
            };
            let steps = plan(
                &db,
                super::super::query::storage_page_query(&sort, filter)
                    .replace("LIMIT ? OFFSET ?", "LIMIT 50 OFFSET 0"),
            )
            .await;
            assert!(
                steps
                    .iter()
                    .any(|step| step.contains("USING INDEX idx_release_listing_")
                        && step.contains("storage")),
                "{filter:?} {direction:?}: {steps:?}"
            );
            assert!(
                !steps
                    .iter()
                    .any(|step| step.contains("TEMP B-TREE FOR ORDER BY")),
                "{filter:?} {direction:?}: {steps:?}"
            );
        }
    }
}
