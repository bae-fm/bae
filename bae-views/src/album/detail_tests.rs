//! These tests check that the album page's detail read reads every release in
//! one statement, shows the release its request chose or the primary one, and
//! returns no album after the test deletes the album.

use super::*;
use bae_records::test_fixtures::live_query::*;
use bae_records::test_fixtures::*;
use bae_util::test_channels::HANG_GUARD;

/// An album's page reads its releases with their track ratings, its album
/// with its ratings and release choices, and its artist credits. The three
/// statements hold for any number of releases.
#[tokio::test]
async fn album_detail_reads_every_release_in_one_statement() {
    let (db, _temp) =
        bae_records::test_fixtures::aggregate_ordering::aggregate_db(crate::TEST_SCHEMA).await;
    let log = db.record_statements_for_test().await.unwrap();
    let detail = crate::LibraryViews::new(db.clone())
        .find_album_detail(bae_records::test_fixtures::ALBUM_A)
        .await
        .unwrap()
        .expect("the album has releases");
    assert_eq!(detail.releases.len(), 3, "the album has three releases");
    assert!(detail.shows_release_choices, "an album of three releases offers a choice");
    let statements = log.take();
    assert_eq!(statements.len(), 3, "{} statements", statements.len());
}

/// An album of one release offers no choice between releases.
#[tokio::test]
async fn an_album_of_one_release_shows_no_release_choices() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let detail = crate::LibraryViews::new(db.clone())
        .find_album_detail(ALBUM_ID)
        .await
        .unwrap()
        .expect("the album has a release");
    assert_eq!(detail.releases.len(), 1);
    assert!(!detail.shows_release_choices);
}

/// The album page shows the release its request chose while the album holds
/// it, and the primary release otherwise, with the release's place in the
/// album's order and whether it can become the primary release.
#[tokio::test]
async fn album_detail_shows_the_chosen_release_or_the_primary_one() {
    let (db, _temp) =
        bae_records::test_fixtures::aggregate_ordering::aggregate_db(crate::TEST_SCHEMA).await;
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_detail(Some(
        crate::album_detail::AlbumDetailRequest {
            album_id: bae_records::test_fixtures::ALBUM_A.to_string(),
            chosen_release_id: Some(bae_records::test_fixtures::RELEASE_B.to_string()),
        },
    ));
    let requests = live.requests();
    let chosen =
        live.next_inline().await.expect("the query stays open").result.unwrap().detail.unwrap();
    assert_eq!(chosen.shown_release_index, 2, "the album orders its releases Z, A, B");
    assert!(chosen.shown_release_can_become_primary);

    requests
        .set(Some(crate::album_detail::AlbumDetailRequest {
            album_id: bae_records::test_fixtures::ALBUM_A.to_string(),
            chosen_release_id: Some("release-of-no-album".to_string()),
        }))
        .unwrap();
    let primary = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("a new request reruns the query")
        .expect("the query stays open")
        .result
        .unwrap()
        .detail
        .unwrap();
    assert_eq!(primary.shown_release_index, 0);
    assert_eq!(primary.primary_release_id, bae_records::test_fixtures::RELEASE_Z);
    assert!(!primary.shown_release_can_become_primary);

    let resolved =
        resolve_album_detail_projection(AlbumDetailProjection { detail: Some(chosen) }).unwrap();
    assert_eq!(resolved.shown_release.summary.id, bae_records::test_fixtures::RELEASE_B);
}

#[tokio::test]
async fn album_detail_subscription_delivers_absence_after_deletion() {
    let (db, _temp) = live_db(crate::TEST_SCHEMA).await;
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_detail(Some(
        crate::album_detail::AlbumDetailRequest::album(ALBUM_ID.to_string()),
    ));
    assert!(live
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .detail
        .is_some());

    delete_release(&test_records(&db), RELEASE_ID).await;

    let deleted = tokio::time::timeout(HANG_GUARD, live.next_inline())
        .await
        .expect("deleting the album's one release reruns the album detail query")
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(deleted.detail.is_none());
}
