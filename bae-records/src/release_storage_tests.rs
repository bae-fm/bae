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

/// The device id, as coven stores it, that `release`'s storage row on
/// `store` names as alone holding one of its audio files, or `None`.
async fn audio_elsewhere(store: &bae_store::Store, release: &'static str) -> Option<Vec<u8>> {
    store
        .read(move |sql| {
            Ok(sql.query_row(
                "SELECT audio_elsewhere_device FROM release_storage WHERE release_id = ?1",
                [release],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap()
}

/// A release's storage row names the device that alone holds one of its
/// audio files in the write that applies that file's record here, and names
/// none on that device itself or for a release whose only file elsewhere is
/// not audio.
#[test]
fn the_storage_row_names_the_device_that_alone_holds_its_audio() {
    use bae_util::content_type::ContentType::{Flac, Jpeg};
    bae_store::test_devices::run_two_store_test(
        crate::test_fixtures::TEST_SCHEMA,
        async |devices| {
            let dir = tempfile::tempdir().unwrap();
            let (a, b) = (devices.a(), devices.b());
            write_library(&crate::test_fixtures::test_records(a)).await;
            devices.converge("SELECT id FROM releases").await;

            import_file(a, dir.path(), RELEASE_EARLY, "track.flac", Flac).await;
            import_file(a, dir.path(), RELEASE_EARLY, "cover.jpg", Jpeg).await;
            import_file(a, dir.path(), RELEASE_LATE, "front.jpg", Jpeg).await;
            devices.converge("SELECT id, release_id, file FROM release_files").await;
            devices.converge("SELECT id, coordinates FROM coven_files").await;

            let device_a: Vec<u8> = b
                .read(|sql| {
                    Ok(sql.query_row(
                        "SELECT device FROM coven_devices WHERE NOT is_self",
                        [],
                        |row| row.get(0),
                    )?)
                })
                .await
                .unwrap();
            assert_eq!(audio_elsewhere(b, RELEASE_EARLY).await, Some(device_a));
            assert_eq!(audio_elsewhere(a, RELEASE_EARLY).await, None, "the device that holds it");
            assert_eq!(audio_elsewhere(b, RELEASE_LATE).await, None, "artwork is not audio");
        },
    );
}
