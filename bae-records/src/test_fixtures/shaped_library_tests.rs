use super::*;
use bae_store::test_fixtures::{row_count, temp_db};

async fn count(db: &bae_store::Store, sql: &'static str) -> i64 {
    db.read(move |read| read.query_row(sql, [], |row| row.get(0)).map_err(Into::into))
        .await
        .unwrap()
}

/// 2,000 albums hold the shape the module describes: its box sets, its
/// compilations, its works and its merges.
#[tokio::test]
async fn two_thousand_albums_have_the_shape() {
    let (db, _tmp) = temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    db.write(|sql| insert_on(&sql, 0, 2_000)).await.unwrap();

    assert_eq!(row_count(&db, "albums").await, 2_000);
    let (releases, tracks) = (row_count(&db, "releases").await, row_count(&db, "tracks").await);
    assert!((2_200..=2_450).contains(&releases), "{releases} releases");
    // 12 tracks a release, 40 box sets of 120 tracks and 4 of 600.
    assert!((30_000..=38_000).contains(&tracks), "{tracks} tracks");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM releases WHERE id IN (SELECT release_id FROM tracks GROUP BY release_id HAVING COUNT(*) = 600)").await,
        4
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM releases WHERE id IN (SELECT release_id FROM tracks GROUP BY release_id HAVING COUNT(*) = 120)").await,
        40
    );
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM (SELECT release_id FROM release_mediums GROUP BY release_id HAVING COUNT(*) = 30)").await,
        4
    );
    let compilations = count(&db, "SELECT COUNT(*) FROM albums WHERE is_compilation").await;
    assert!((60..=140).contains(&compilations), "{compilations} compilations");
    assert_eq!(row_count(&db, "artists").await, 500);
    assert!(row_count(&db, "works").await >= 600);
    assert!(row_count(&db, "track_works").await > 0);
    assert!(row_count(&db, "release_artist_roles").await > 0);
    assert!(row_count(&db, "track_artist_roles").await > 0);
    assert_eq!(row_count(&db, "artist_merges").await, 5);
    assert_eq!(row_count(&db, "artist_credits").await, 25);
    // A few artists hold most albums: the top one holds far more than the
    // average of four.
    let top = count(
        &db,
        "SELECT MAX(n) FROM (SELECT COUNT(*) AS n FROM album_artists GROUP BY artist_id)",
    )
    .await;
    assert!(top > 40, "the top artist holds {top} albums");
}
