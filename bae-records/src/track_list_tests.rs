use super::*;

/// A picked list reads in the order the picks' release lists them, whatever
/// order the picks came in, with each pick once and a pick the library lacks
/// first; a given list reads as given.
#[tokio::test]
async fn picked_tracks_read_in_their_release_s_order() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let listed = crate::test_fixtures::chunked_tracks::track_ids(6);
    let seeded = listed.clone();
    db.write(move |sql| crate::test_fixtures::chunked_tracks::seed_release(&sql, &seeded))
        .await
        .unwrap();
    let picked = TrackList::Picked(BTreeSet::from([
        listed[4].clone(),
        "missing-track".to_string(),
        listed[1].clone(),
        listed[2].clone(),
    ]));
    let given = TrackList::Given(vec![listed[4].clone(), listed[1].clone(), listed[4].clone()]);

    let (picked, given) =
        db.read(move |sql| Ok((picked.ordered_on(&sql)?, given.ordered_on(&sql)?))).await.unwrap();

    assert_eq!(
        picked,
        ["missing-track".to_string(), listed[1].clone(), listed[2].clone(), listed[4].clone()]
    );
    assert_eq!(given, [listed[4].clone(), listed[1].clone(), listed[4].clone()]);
}
