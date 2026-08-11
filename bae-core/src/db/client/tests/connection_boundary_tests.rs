use super::super::*;
use super::*;
use coven::SystemClock;
use std::sync::Arc;

#[tokio::test]
async fn coven_connection_enforces_foreign_keys_for_bae_schema() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();

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
