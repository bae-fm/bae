use crate::test_fixtures::storage_listing::*;

/// The releases uploading are read in queue order from an index of their
/// storage rows that holds every column read, not from coven's queue grouped
/// by release.
#[tokio::test]
async fn the_uploading_releases_read_their_storage_in_queue_order() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    assert_eq!(
        plan(
            &db,
            format!(
                "SELECT release_id FROM ({}) ORDER BY position",
                crate::records::query::UPLOADING_RELEASES
            )
        )
        .await,
        vec!["SEARCH uploading USING COVERING INDEX idx_release_storage_uploading (first_queued_position>?)".to_string()]
    );
}
