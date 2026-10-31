use super::super::*;

const RELEASE_EARLY: &str = "d439555e-9883-4037-a580-48ea33c2c57b";
const RELEASE_LATE: &str = "af4421de-5174-45a9-81fc-74110bc5f85d";

/// Two releases on albums titled to sort apart.
const LIBRARY: &str = "
    INSERT INTO albums (id, title, is_compilation, created_at) VALUES
        ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z'),
        ('dab6efe2-9b5c-4034-b95e-73cf49f54b9a', 'Album Title B', 0, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0),
        ('af4421de-5174-45a9-81fc-74110bc5f85d', 'dab6efe2-9b5c-4034-b95e-73cf49f54b9a', '2026-01-01T00:00:00Z', 0);
";

/// Seed the release file `id` of `release`, named `name`, staged on this
/// device only from an original of `size` zero bytes written under `dir`.
async fn insert_file(
    db: &Database,
    dir: &std::path::Path,
    (id, release, name, size): (&str, &str, &str, u64),
) {
    let original = dir.join(id);
    std::fs::File::create_new(&original)
        .unwrap()
        .set_len(size)
        .unwrap();
    db.insert_file_for_test(
        &NewReleaseFile::new(
            release,
            name,
            bae_util::content_type::ContentType::Jpeg,
            id.to_string(),
            chrono::Utc::now(),
        ),
        &original,
    )
    .await
    .unwrap();
}

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
    let (db, tmp) = super::temp_db().await;
    exec(&db, LIBRARY).await;
    // The first release has two files, of 10 and 20 bytes; the second none.
    for file in [
        (
            "d3edb846-9b0b-4a64-b0f6-3f117b686bc0",
            RELEASE_EARLY,
            "cover.jpg",
            10,
        ),
        (
            "161dd6f3-3bdd-4312-b5d3-8b9d83b53694",
            RELEASE_EARLY,
            "back.jpg",
            20,
        ),
    ] {
        insert_file(&db, tmp.path(), file).await;
    }
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
        "DELETE FROM release_files WHERE id = '161dd6f3-3bdd-4312-b5d3-8b9d83b53694';",
    )
    .await;
    for file in [
        (
            "f5008044-8ffe-4227-bdc3-238f85a6a4a6",
            RELEASE_LATE,
            "one.jpg",
            1,
        ),
        (
            "cda4c2ec-c6fa-422c-a224-d4f56ca5f3f3",
            RELEASE_LATE,
            "two.jpg",
            2,
        ),
    ] {
        insert_file(&db, tmp.path(), file).await;
    }
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
            let order = super::super::query::storage_order(&sort);
            let steps = plan(
                &db,
                super::super::storage_window_query(
                    &order,
                    filter,
                    super::super::StorageRows::Listed,
                    &order.everything(),
                )
                .replace("LIMIT :limit OFFSET :offset", "LIMIT 50 OFFSET 0"),
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

/// A bare library connection that records every release whose listing row
/// a write asks to refresh, in `refreshed`.
fn counting_refreshes() -> rusqlite::Connection {
    let conn = crate::library::bare_library_connection();
    conn.execute_batch(
        "CREATE TEMP TABLE refreshed (release_id TEXT NOT NULL);
         CREATE TEMP TRIGGER count_refresh INSTEAD OF INSERT ON main.release_listing_refresh
         BEGIN
             INSERT INTO refreshed (release_id) VALUES (NEW.release_id);
         END;",
    )
    .unwrap();
    conn
}

/// How many refreshes of `release`'s listing row the writes since the count
/// was last cleared asked for.
fn refreshes(conn: &rusqlite::Connection, release: &str) -> i64 {
    conn.query_row(
        "SELECT count(*) FROM refreshed WHERE release_id = ?1",
        [release],
        |row| row.get(0),
    )
    .unwrap()
}

/// The album `release`'s listing row names, None while it has none.
fn listed_album(conn: &rusqlite::Connection, release: &str) -> Option<String> {
    conn.query_row(
        "SELECT (SELECT album_id FROM release_listing WHERE release_id = ?1)",
        [release],
        |row| row.get(0),
    )
    .unwrap()
}

/// Run `sql` as one write.
fn write(conn: &mut rusqlite::Connection, sql: &str) {
    let write = conn.transaction().unwrap();
    write.execute_batch(sql).unwrap();
    write.commit().unwrap();
}

const ALBUM_A: &str = "4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d";
const ALBUM_B: &str = "dab6efe2-9b5c-4034-b95e-73cf49f54b9a";

/// A release arriving writes its listing row with one refresh, in the write
/// that adds it: as a snapshot installs it, after its album, which lists that
/// album too, and as it joins an album already listed.
#[test]
fn a_release_arriving_refreshes_its_listing_once() {
    let mut conn = counting_refreshes();

    write(
        &mut conn,
        "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
             ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order) VALUES
             ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0);",
    );
    assert_eq!(listed_album(&conn, RELEASE_EARLY).as_deref(), Some(ALBUM_A));
    assert_eq!(refreshes(&conn, RELEASE_EARLY), 1);

    write(
        &mut conn,
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES
             ('af4421de-5174-45a9-81fc-74110bc5f85d', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-02T00:00:00Z', 1);",
    );
    assert_eq!(listed_album(&conn, RELEASE_LATE).as_deref(), Some(ALBUM_A));
    assert_eq!(refreshes(&conn, RELEASE_LATE), 1);
    assert_eq!(refreshes(&conn, RELEASE_EARLY), 1);
}

/// A release changing writes its listing row with one refresh: moving to an
/// album the same write adds, which lists that album and its releases;
/// moving to an album already listed; and changing what it states itself.
#[test]
fn a_release_changing_refreshes_its_listing_once() {
    let mut conn = counting_refreshes();
    write(
        &mut conn,
        "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
             ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order) VALUES
             ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0),
             ('af4421de-5174-45a9-81fc-74110bc5f85d', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-02T00:00:00Z', 1);
         DELETE FROM refreshed;",
    );

    write(
        &mut conn,
        "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
             ('dab6efe2-9b5c-4034-b95e-73cf49f54b9a', 'Album Title B', 0, '2026-01-03T00:00:00Z');
         UPDATE releases SET album_id = 'dab6efe2-9b5c-4034-b95e-73cf49f54b9a'
         WHERE id = 'af4421de-5174-45a9-81fc-74110bc5f85d';",
    );
    assert_eq!(listed_album(&conn, RELEASE_LATE).as_deref(), Some(ALBUM_B));
    assert_eq!(refreshes(&conn, RELEASE_LATE), 1);

    write(
        &mut conn,
        "DELETE FROM refreshed;
         UPDATE releases SET album_id = '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d'
         WHERE id = 'af4421de-5174-45a9-81fc-74110bc5f85d';",
    );
    assert_eq!(listed_album(&conn, RELEASE_LATE).as_deref(), Some(ALBUM_A));
    assert_eq!(refreshes(&conn, RELEASE_LATE), 1);

    write(
        &mut conn,
        "DELETE FROM refreshed;
         UPDATE releases SET media = '[{\"medium\": \"vinyl\", \"count\": 1}]'
         WHERE id = 'af4421de-5174-45a9-81fc-74110bc5f85d';",
    );
    assert_eq!(refreshes(&conn, RELEASE_LATE), 1);
    assert_eq!(refreshes(&conn, RELEASE_EARLY), 0);
}
