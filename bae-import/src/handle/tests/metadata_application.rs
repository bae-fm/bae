//! Applying metadata replaces the candidate's editable values.

use super::*;
use crate::CandidateEditField;

/// A Discogs release with pressing metadata and no barcode.
pub(super) fn seed_discogs_pressing(
    fakes: &FakeCatalogs,
    release_id: &str,
    label: &str,
    catalog_number: &str,
) {
    seed_discogs_release(fakes, release_id, label, catalog_number, &[]);
}

/// [`seed_discogs_pressing`], printed with `barcodes`.
fn seed_discogs_release(
    fakes: &FakeCatalogs,
    release_id: &str,
    label: &str,
    catalog_number: &str,
    barcodes: &[&str],
) {
    let raw_release = serde_json::json!({
        "identifiers": barcodes
            .iter()
            .map(|value| serde_json::json!({ "type": "Barcode", "value": value }))
            .collect::<Vec<_>>(),
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
    bae_catalogs::discogs::documents::parse_discogs_release_json(&raw_release)
        .expect("the rendered Discogs release parses");
    fakes.seed_discogs_release(release_id, raw_release);
    fakes.seed_discogs_artist_image("1", None);
    fakes.seed_discogs_page_lookup(&[(
        bae_catalogs::musicbrainz::DiscogsPage::Release(release_id.to_string()),
        None,
    )]);
}

#[tokio::test(flavor = "multi_thread")]
async fn resetting_to_the_tags_drops_what_was_typed() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let from_tags = pane(&handle, &key).await.draft().unwrap().metadata_draft.clone();

    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Text {
                field: CandidateEditField::AlbumTitle,
                value: "Typed Title".to_string(),
            },
        )
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Labels(vec![crate::RawLabelEdit {
                name: "Typed Label".to_string(),
                catalog_number: String::new(),
                ..Default::default()
            }]),
        )
        .await
        .unwrap();
    handle.select_candidate_file_tags(key.clone()).await.unwrap();

    let reset = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(
        reset.draft().unwrap().metadata_draft.album_title,
        from_tags.album_title,
        "the title the tags state replaces the one that was typed over it"
    );
    assert_eq!(
        reset.draft().unwrap().metadata_draft.pressing.labels,
        from_tags.pressing.labels,
        "a field the tags leave blank goes back to blank"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pick_replaces_typed_fields_with_the_catalog_metadata() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    handle
        .set_candidate_edit_field(
            &key,
            crate::DraftFieldEdit::Barcodes(vec!["5099749".to_string()]),
        )
        .await
        .unwrap();

    let release_id = "70000101";
    seed_discogs_pressing(&fakes, release_id, "Label Name", "CAT-1");
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    assert_eq!(
        pane.draft().unwrap().metadata_draft.pressing.labels,
        vec![crate::RawLabelEdit {
            name: "Label Name".to_string(),
            catalog_number: "CAT-1".to_string(),
            ..Default::default()
        }]
    );
    assert_eq!(
        pane.draft().unwrap().metadata_draft.pressing.barcodes,
        Vec::<String>::new(),
        "explicit application replaces typed values, including with absent metadata"
    );
}

/// A Discogs release printed with an EAN and a UPC keeps both: applying it
/// writes both barcode rows into the draft, and the edit the import writes
/// states both.
#[tokio::test(flavor = "multi_thread")]
async fn applying_a_release_with_two_barcodes_keeps_both() {
    let fakes = FakeCatalogs::offline();
    let (handle, _tmp, key, _hash) = pane_fixture_with(fakes.providers()).await;
    handle
        .catalogs
        .store_discogs_key("test-discogs-token", bae_preferences::DiscogsValidation::Valid)
        .await
        .unwrap();
    let release_id = "70000102";
    seed_discogs_release(
        &fakes,
        release_id,
        "Label Name",
        "CAT-1",
        &["5 012345 678900", "0 12345 67890 5"],
    );
    handle
        .select_candidate_release(
            key.clone(),
            bae_identify::PressingLink {
                record: bae_catalogs::records::LookupRef::new(
                    bae_catalogs::catalog::LookupCatalog::Discogs,
                    release_id.to_string(),
                ),
                partners: vec![],
            },
            crate::CatalogApplySteps::unread(),
        )
        .await
        .unwrap();
    let pane = pane(&handle, &key).await;
    shut_down(handle).await;

    let both = ["5 012345 678900", "0 12345 67890 5"];
    assert_eq!(pane.draft().unwrap().metadata_draft.pressing.barcodes, both);
    let shaped = pane.draft().unwrap().metadata_draft.shape().expect("the applied draft shapes");
    assert_eq!(shaped.pressing.barcodes, both);
    assert_eq!(
        crate::RawPressingEdit::from_pressing(&shaped.pressing).barcodes,
        both,
        "the editor's rows read both back"
    );
}
