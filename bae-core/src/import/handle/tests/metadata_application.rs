//! Applying metadata replaces the candidate's editable values.

use super::*;
use crate::import::CandidateEditField;
use serial_test::serial;

/// A Discogs release with pressing metadata and no barcode.
fn seed_discogs_pressing(release_id: &str, label: &str, catalog_number: &str) {
    let raw_release = serde_json::json!({
        "id": release_id.parse::<u64>().expect("a numeric test Discogs release id"),
        "title": "Album Title",
        "year": 1996,
        "formats": [{ "name": "CD" }],
        "country": "US",
        "labels": [{ "name": label, "catno": catalog_number }],
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [{
            "position": "1",
            "title": "Track One",
            "duration": "0:01",
            "type_": "track",
            "artists": [],
        }, {
            "position": "2", "title": "Track Two", "duration": "0:01", "type_": "track", "artists": []
        }],
    })
    .to_string();
    crate::discogs::client::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    crate::discogs::client::seed_release_cache(release_id, raw_release);
    crate::discogs::client::seed_artist_image_response("1", None);
    crate::musicbrainz::seed_discogs_url_lookup(release_id, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn resetting_to_the_tags_drops_what_was_typed() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let from_tags = pane(&handle, &key).await.metadata_draft;

    handle
        .set_candidate_edit_field(
            &key,
            CandidateEditField::AlbumTitle,
            "Typed Title".to_string(),
        )
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(&key, CandidateEditField::Label, "Typed Label".to_string())
        .await
        .unwrap();
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::FileMetadata,
        )
        .await
        .unwrap();

    let reset = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(
        reset.metadata_draft.album_title, from_tags.album_title,
        "the title the tags state replaces the one that was typed over it"
    );
    assert_eq!(
        reset.metadata_draft.pressing.label, from_tags.pressing.label,
        "a field the tags leave blank goes back to blank"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[serial(musicbrainz)]
async fn a_pick_replaces_typed_fields_with_the_catalog_metadata() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    handle
        .library_manager
        .set_discogs_key(
            "test-discogs-token",
            crate::config::DiscogsValidation::Valid,
        )
        .unwrap();
    handle
        .set_candidate_edit_field(&key, CandidateEditField::Barcode, "5099749".to_string())
        .await
        .unwrap();

    let release_id = "70000101";
    seed_discogs_pressing(release_id, "Label Name", "CAT-1");
    handle
        .select_candidate_metadata_provenance(
            key.clone(),
            crate::import::MetadataProvenance::ExternalRelease {
                record: crate::import::MetadataRef::new(
                    crate::import::Catalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
        )
        .await
        .unwrap();
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(pane.metadata_draft.pressing.label, "Label Name");
    assert_eq!(
        pane.metadata_draft.pressing.barcode, "",
        "explicit application replaces typed values, including with absent metadata"
    );
}
