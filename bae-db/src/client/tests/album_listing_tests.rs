use super::super::*;
use crate::test_fixtures::album_listing::*;

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
