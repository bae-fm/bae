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
