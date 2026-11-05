use crate::client::*;
use crate::test_fixtures::album_listing::*;
use crate::test_fixtures::storage_listing::*;

/// How many rows the library's tables, the listings' triggers included,
/// change when `release` is added to Album Title One.
async fn rows_changed_adding_release(db: &Database, release: &str) -> i64 {
    let release = release.to_string();
    db.call(move |conn| {
        let before: i64 = conn.query_row("SELECT total_changes()", [], |row| row.get(0))?;
        conn.execute(
            "INSERT INTO releases (id, album_id, created_at, album_order) \
             VALUES (?1, '07d22f7d-3980-4c81-9d27-8fcd50ee3464', '2026-01-01T00:00:00Z', 1)",
            [&release],
        )?;
        let after: i64 = conn.query_row("SELECT total_changes()", [], |row| row.get(0))?;
        Ok(after - before)
    })
    .await
    .unwrap()
}

/// Adding a release to an album lists that release and leaves the album's
/// other releases' listing rows alone: what it costs doesn't grow with the
/// releases the album already holds, as a box set's would.
#[tokio::test]
async fn adding_a_release_rewrites_no_other_release_of_its_album() {
    let (db, _tmp) = listed_db().await;
    let second =
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-1")).await;
    for i in 2..22 {
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid(&format!("box-set-{i}")))
            .await;
    }
    let twenty_second =
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-22")).await;
    assert_eq!(twenty_second, second, "the 22nd release changes as many rows as the second");
}

/// Renaming an album renames every one of its releases' listing rows, so
/// the Storage Manager sorts them by the new title.
#[tokio::test]
async fn renaming_an_album_renames_its_releases_listing_rows() {
    let (db, _tmp) = listed_db().await;
    rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-1")).await;
    crate::test_fixtures::exec_batch(
        &db,
        "UPDATE albums SET title = 'Album Title Renamed' \
         WHERE id = '07d22f7d-3980-4c81-9d27-8fcd50ee3464'",
    )
    .await;
    let titles: Vec<String> = db
        .call(|conn| {
            Ok(conn.query(
                "SELECT album_title FROM release_listing \
                 WHERE album_id = '07d22f7d-3980-4c81-9d27-8fcd50ee3464'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(titles, vec!["Album Title Renamed".to_string(); 2]);
}

/// The releases uploading are read in queue order from an index of the
/// listing that holds every column read, not from coven's queue grouped by
/// release.
#[tokio::test]
async fn the_uploading_releases_read_the_listing_in_queue_order() {
    let (db, _tmp) = crate::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    assert_eq!(
        plan(
            &db,
            format!(
                "SELECT release_id FROM ({}) ORDER BY position",
                crate::client::query::UPLOADING_RELEASES
            )
        )
        .await,
        vec!["SEARCH uploading USING COVERING INDEX idx_release_listing_uploading (first_queued_position>?)".to_string()]
    );
}

/// A bare library connection that records every release whose listing row
/// a write asks to refresh, in `refreshed`.
fn counting_refreshes() -> rusqlite::Connection {
    let conn = crate::schema::bare_library_connection(crate::test_fixtures::TEST_SCHEMA);
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
    conn.query_row("SELECT count(*) FROM refreshed WHERE release_id = ?1", [release], |row| {
        row.get(0)
    })
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
         UPDATE releases SET created_at = '2026-01-04T00:00:00Z'
         WHERE id = 'af4421de-5174-45a9-81fc-74110bc5f85d';",
    );
    assert_eq!(refreshes(&conn, RELEASE_LATE), 1);
    assert_eq!(refreshes(&conn, RELEASE_EARLY), 0);
}

/// `release`'s listing row's media columns.
fn listed_media(conn: &rusqlite::Connection, release: &str) -> (bool, Option<String>) {
    conn.query_row(
        "SELECT media_empty, media_key FROM release_listing WHERE release_id = ?1",
        [release],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

/// A release's media rows arriving, changing and leaving set its listing
/// row's media, in order, without refreshing the rest of the row.
#[test]
fn a_release_s_media_rows_set_its_listing_s_media() {
    let mut conn = counting_refreshes();
    write(
        &mut conn,
        "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
             ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
         INSERT INTO releases (id, album_id, created_at, album_order) VALUES
             ('af4421de-5174-45a9-81fc-74110bc5f85d', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-02T00:00:00Z', 0);
         INSERT INTO release_media (id, release_id, position, medium, count) VALUES
             ('media-1', 'af4421de-5174-45a9-81fc-74110bc5f85d', 1, 'cd', 1),
             ('media-0', 'af4421de-5174-45a9-81fc-74110bc5f85d', 0, 'vinyl', 2);",
    );
    assert_eq!(refreshes(&conn, RELEASE_LATE), 1);
    assert_eq!(
        listed_media(&conn, RELEASE_LATE),
        (false, Some("vinyl:00002,cd:00001".to_string()))
    );

    write(
        &mut conn,
        "DELETE FROM refreshed;
         UPDATE release_media SET count = 3 WHERE id = 'media-0';",
    );
    assert_eq!(
        listed_media(&conn, RELEASE_LATE),
        (false, Some("vinyl:00003,cd:00001".to_string()))
    );

    write(&mut conn, "DELETE FROM release_media;");
    assert_eq!(listed_media(&conn, RELEASE_LATE), (true, None));
    assert_eq!(refreshes(&conn, RELEASE_LATE), 0);
}
