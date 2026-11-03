use super::super::*;

pub(crate) const BRAVO: &str = "f392a38b-f264-41a0-a225-c6935b8a8262";

pub(crate) const ALPHA: &str = "4309b580-a6bf-455e-92ea-ec58ee7129e7";

pub(crate) const ALBUM_ONE: &str = "07d22f7d-3980-4c81-9d27-8fcd50ee3464";

pub(crate) const ALBUM_TWO: &str = "7cc40503-11b4-4ba3-8cec-45ba0498232b";

pub(crate) const ALBUM_THREE: &str = "c217fb46-e6a3-4eaf-bc0a-ac9036440dc7";

/// Two artists and three albums: the first by Bravo (2001), the second by
/// Alpha (year unknown), the third by Bravo and Alpha in that order (1999),
/// each on one release.
pub(crate) const LIBRARY: &str = "
    INSERT INTO artists (id, name, name_key, created_at) VALUES
        ('f392a38b-f264-41a0-a225-c6935b8a8262', 'Artist Name Bravo', 'artist name bravo', '2026-01-01T00:00:00Z'),
        ('4309b580-a6bf-455e-92ea-ec58ee7129e7', 'Artist Name Alpha', 'artist name alpha', '2026-01-01T00:00:00Z');
    INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES
        ('07d22f7d-3980-4c81-9d27-8fcd50ee3464', 'Album Title One', 2001, 0, '2026-01-01T00:00:01Z'),
        ('7cc40503-11b4-4ba3-8cec-45ba0498232b', 'Album Title Two', NULL, 0, '2026-01-01T00:00:02Z'),
        ('c217fb46-e6a3-4eaf-bc0a-ac9036440dc7', 'Album Title Three', 1999, 0, '2026-01-01T00:00:03Z');
    INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES
        ('b55e9fa2-fc5e-4963-bb58-2bb34d532ce6', '07d22f7d-3980-4c81-9d27-8fcd50ee3464', 'f392a38b-f264-41a0-a225-c6935b8a8262', 0, '2026-01-01T00:00:00Z'),
        ('817dd37c-8326-48de-9da1-3ea2879a6a74', '7cc40503-11b4-4ba3-8cec-45ba0498232b', '4309b580-a6bf-455e-92ea-ec58ee7129e7', 0, '2026-01-01T00:00:00Z'),
        ('cce3b935-9acd-4639-a03a-2c039a765772', 'c217fb46-e6a3-4eaf-bc0a-ac9036440dc7', 'f392a38b-f264-41a0-a225-c6935b8a8262', 0, '2026-01-01T00:00:00Z'),
        ('7cb353ba-0c4d-4167-b18d-cbf6681548bb', 'c217fb46-e6a3-4eaf-bc0a-ac9036440dc7', '4309b580-a6bf-455e-92ea-ec58ee7129e7', 1, '2026-01-01T00:00:00Z');
    INSERT INTO releases (id, album_id, created_at, album_order) VALUES
        ('5a905d5d-d400-42f3-a9a8-b858f18dcbc3', '07d22f7d-3980-4c81-9d27-8fcd50ee3464', '2026-01-01T00:00:00Z', 0),
        ('3a629e97-79c5-42da-99ac-09cfb3733dd7', '7cc40503-11b4-4ba3-8cec-45ba0498232b', '2026-01-01T00:00:00Z', 0),
        ('7cc7aeee-e43c-469d-aa7a-990ea258bd80', 'c217fb46-e6a3-4eaf-bc0a-ac9036440dc7', '2026-01-01T00:00:00Z', 0);
";

pub(crate) async fn listed_db() -> (Database, tempfile::TempDir) {
    let (db, tmp) = super::temp_db().await;
    exec(&db, LIBRARY).await;
    (db, tmp)
}

pub(crate) async fn exec(db: &Database, sql: &'static str) {
    db.call(move |conn| conn.execute_batch(sql).map_err(DbError::from)).await.unwrap();
}

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
    exec(
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
