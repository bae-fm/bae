use super::*;

/// A picked list reads in the order the picks' release lists them, whatever
/// order the picks came in, with each pick once and a pick the library lacks
/// first, or left out when the read asks only for held tracks; a given list
/// reads as given, or without the tracks the library lacks.
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
    let given = TrackList::Given(vec![
        listed[4].clone(),
        "missing-track".to_string(),
        listed[1].clone(),
        listed[4].clone(),
    ]);

    let reads = db
        .read(move |sql| {
            Ok([
                picked.ordered_on(&sql, TrackReading::Named)?,
                picked.ordered_on(&sql, TrackReading::Held)?,
                given.ordered_on(&sql, TrackReading::Named)?,
                given.ordered_on(&sql, TrackReading::Held)?,
            ])
        })
        .await
        .unwrap();

    assert_eq!(
        reads[0],
        ["missing-track".to_string(), listed[1].clone(), listed[2].clone(), listed[4].clone()]
    );
    assert_eq!(reads[1], [listed[1].clone(), listed[2].clone(), listed[4].clone()]);
    assert_eq!(
        reads[2],
        [listed[4].clone(), "missing-track".to_string(), listed[1].clone(), listed[4].clone()]
    );
    assert_eq!(reads[3], [listed[4].clone(), listed[1].clone(), listed[4].clone()]);
}
