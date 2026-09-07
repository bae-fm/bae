use super::super::*;
use super::*;

#[tokio::test]
async fn coven_connection_enforces_foreign_keys_for_bae_schema() {
    let (db, _tmp) = super::temp_db().await;

    let track = DbTrack::new_test("missing-release", TRACK_A, "Track Title A", Some(1));
    let error = db
        .insert_track(&track)
        .await
        .expect_err("track insert without a release must violate the foreign key");

    assert!(
        error.to_string().contains("FOREIGN KEY constraint failed"),
        "expected a foreign-key violation, got {error}"
    );
}
