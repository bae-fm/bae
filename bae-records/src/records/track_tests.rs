use crate::records::*;
use crate::test_fixtures::chunked_tracks::*;

#[tokio::test]
async fn filter_existing_track_ids_merges_chunks() {
    let (db, _tmp, track_ids) = chunked_track_db(crate::test_fixtures::TEST_SCHEMA).await;
    let records = crate::test_fixtures::test_records(&db);
    let mut requested = track_ids.clone();
    requested.insert(SQL_MAX_IN_VARS / 2, "missing-track".to_string());

    let mut existing = records.filter_existing_track_ids(&requested).await.unwrap();
    existing.sort();
    let mut expected_existing = track_ids;
    expected_existing.sort();
    assert_eq!(existing, expected_existing);
}

/// `track_ids_between` returns the tracks from the anchor to the target in
/// their release's order, whichever comes first; the target alone when the
/// library lacks the anchor, and nothing when it lacks the target.
#[tokio::test]
async fn track_ids_between_run_from_the_anchor_to_the_target_in_release_order() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let listed = crate::test_fixtures::chunked_tracks::track_ids(6);
    let seeded = listed.clone();
    db.write(move |sql| crate::test_fixtures::chunked_tracks::seed_release(&sql, &seeded))
        .await
        .unwrap();
    let records = crate::test_fixtures::test_records(&db);
    let between = |anchor: &str, target: &str| {
        records.track_ids_between(anchor.to_string(), target.to_string())
    };

    assert_eq!(between(&listed[1], &listed[4]).await.unwrap(), listed[1..=4]);
    assert_eq!(between(&listed[4], &listed[1]).await.unwrap(), listed[1..=4]);
    assert_eq!(between(&listed[2], &listed[2]).await.unwrap(), [listed[2].clone()]);
    assert_eq!(between("missing-track", &listed[3]).await.unwrap(), [listed[3].clone()]);
    assert!(between(&listed[3], "missing-track").await.unwrap().is_empty());
}

/// `track_ids_in_release_order` returns the picked ids in the order their
/// release lists them, whatever order the picks came in, keeps one copy of a
/// repeated id and drops an id the library does not have.
#[tokio::test]
async fn track_ids_in_release_order_follow_the_release_not_the_picks() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let listed = crate::test_fixtures::chunked_tracks::track_ids(6);
    let seeded = listed.clone();
    db.write(move |sql| crate::test_fixtures::chunked_tracks::seed_release(&sql, &seeded))
        .await
        .unwrap();
    let records = crate::test_fixtures::test_records(&db);

    let picked = vec![
        listed[4].clone(),
        "missing-track".to_string(),
        listed[1].clone(),
        listed[4].clone(),
        listed[2].clone(),
    ];
    let ordered = records.track_ids_in_release_order(picked).await.unwrap();

    assert_eq!(ordered, vec![listed[1].clone(), listed[2].clone(), listed[4].clone()]);
}
