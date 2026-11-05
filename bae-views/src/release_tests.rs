//! The release detail: its track artists, its files and its index among its
//! album's releases, in order.

use crate::*;
use bae_db::test_fixtures::aggregate_ordering::*;
use bae_db::test_fixtures::*;

#[tokio::test]
async fn release_detail_orders_track_artists_and_keeps_tracks_without_artists() {
    let (db, _tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    let detail = crate::LibraryViews::new(db.clone())
        .find_db_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");

    let track_ids: Vec<&str> = detail.tracks.iter().map(|track| track.track.id.as_str()).collect();
    assert_eq!(track_ids, vec![TRACK_A, TRACK_B]);

    let artist_names: Vec<&str> =
        detail.tracks[0].artists.iter().map(|artist| artist.artist.name.as_str()).collect();
    assert_eq!(artist_names, vec!["Track Artist Name First", "Track Artist Name Second"]);
    assert!(detail.tracks[1].artists.is_empty());

    // Each track shows its own credits, and one with none its album's: the
    // detail and every other reader take that from the one SQL rule.
    let names = |credits: &[bae_db::credits::LibraryCredit]| {
        credits.iter().map(|credit| credit.artist.name.clone()).collect::<Vec<_>>()
    };
    let album_artists = db.get_artists_for_album(&detail.release.album_id).await.unwrap();
    assert!(!album_artists.is_empty(), "the fixture's album credits an artist");
    assert_eq!(names(&detail.tracks[0].shown_artists), artist_names);
    assert_eq!(names(&detail.tracks[1].shown_artists), names(&album_artists));
    for track in &detail.tracks {
        let shown = db.get_shown_artists_for_track(&track.track.id).await.unwrap();
        assert_eq!(names(&shown), names(&track.shown_artists), "{}", track.track.id);
    }
    assert!(db.get_shown_artists_for_track("no-such-track").await.unwrap().is_empty());
}

/// A release's detail lists its files in natural order of their names, case
/// aside, and the read under it walks the index of each release's files in
/// that order, with their pins and uploads.
#[tokio::test]
async fn release_detail_files_are_in_case_insensitive_natural_order() {
    let (db, tmp) = release_detail_db(crate::TEST_SCHEMA).await;
    insert_files_out_of_natural_order(&db, tmp.path()).await;

    let detail = crate::LibraryViews::new(db.clone())
        .find_db_release_detail(RELEASE_A)
        .await
        .unwrap()
        .expect("release detail");
    let names: Vec<&str> = detail.files.iter().map(|f| f.original_filename.as_str()).collect();
    assert_eq!(names, FILES_IN_NATURAL_ORDER, "the release's detail");

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::storage_inspector::INSPECTOR_FILES),
                [RELEASE_A],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("USING INDEX idx_release_files_release_order"))
            && !plan.iter().any(|step| step.contains("TEMP B-TREE")),
        "the detail's files walk the index with their pins and uploads: {plan:?}"
    );
}

/// A release's index among its album's releases is how many come before it
/// in the album's order, by place and then id, counted on the album's
/// releases index.
#[tokio::test]
async fn a_releases_index_counts_the_releases_before_it() {
    let (db, _tmp) = aggregate_db(crate::TEST_SCHEMA).await;
    // Places 0, 1, 1: the two at place 1 go by id.
    for (release_id, index) in [
        ("8aa66d48-65a0-42e4-8c1d-e7481e8c1861", 0),
        ("0252dedb-ee39-4547-8803-438dbeb57a64", 1),
        ("64e79a1f-404a-4c34-809a-a3cb44bf1942", 2),
    ] {
        let context = crate::LibraryViews::new(db.clone())
            .find_release_detail_context(release_id)
            .await
            .unwrap()
            .expect("the release exists");
        assert_eq!(context.release_index, index, "{release_id}");
    }

    let plan = db
        .read(|sql| {
            sql.query(
                &format!("EXPLAIN QUERY PLAN {}", crate::release::RELEASE_INDEX_IN_ALBUM),
                ["8aa66d48-65a0-42e4-8c1d-e7481e8c1861"],
                |row| row.get::<_, String>(3),
            )
            .map_err(DbError::from)
        })
        .await
        .unwrap();
    assert!(
        plan.iter().any(|step| step.contains("idx_releases_album_id")),
        "the count reads the album's releases index: {plan:?}"
    );
}
