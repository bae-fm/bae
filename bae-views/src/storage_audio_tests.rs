//! The audio a Storage Manager row lists for a release.

use crate::*;
use bae_records::test_fixtures::storage_listing::*;
use bae_records::test_fixtures::*;

/// A release's Storage Manager row lists its audio held on this device as the
/// original here, which is read after the row's query returns.
#[tokio::test]
async fn a_storage_row_shows_the_audio_this_device_holds() {
    let (db, tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    write_library(&records).await;
    let id = "d3edb846-9b0b-4a64-b0f6-3f117b686bc0";
    let original = tmp.path().join(id);
    std::fs::File::create_new(&original).unwrap().set_len(10).unwrap();
    records
        .insert_file_for_test(
            &bae_records::NewReleaseFile::new(
                RELEASE_EARLY,
                "track.flac",
                bae_util::content_type::ContentType::Flac,
                bae_records::ReleaseFileRole::Other,
                id.to_string(),
                chrono::Utc::now(),
            ),
            &original,
        )
        .await
        .unwrap();
    let selected = std::collections::BTreeSet::from([RELEASE_EARLY.to_string()]);

    let rows = db
        .read(move |sql| {
            crate::storage::storage_selected_on(&sql, StorageFilter::All, None, &selected)
        })
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0.unstored_audio, vec![bae_records::DbUnstoredAudio::Original]);
}
