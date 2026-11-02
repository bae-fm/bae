use super::*;
use crate::catalog_model::test_documents::{
    linked_discogs_master, linked_discogs_release, linked_musicbrainz_release, linked_release_group,
};
use serde_json::json;

/// A Discogs release's master names its MusicBrainz release group back, and
/// the group names the album on other catalogs: the fetch follows the whole
/// chain.
#[tokio::test]
async fn a_master_backlink_is_followed() {
    let fakes = crate::catalog_model::fake_catalogs::FakeCatalogs::offline();
    let providers = fakes.providers();
    fakes.discogs().seed_release_cache("7811", linked_discogs_release());
    fakes.discogs().seed_master_cache("7822", linked_discogs_master());
    fakes.musicbrainz().seed_discogs_page_lookup(&[
        (bae_catalogs::musicbrainz::DiscogsPage::Release("7811".into()), None),
        (bae_catalogs::musicbrainz::DiscogsPage::Master("7822".into()), Some("album-group")),
    ]);
    fakes.musicbrainz().seed_release_group_json_cache("album-group", linked_release_group());
    let client = DiscogsClient::new(fakes.discogs().clone(), "test-token".into());
    let fetched = providers
        .fetch_payloads(
            Some(&client),
            &LookupRef::new(LookupCatalog::Discogs, "7811"),
            ReadDepth::Complete,
            CallPriority::Interactive,
        )
        .await
        .unwrap()
        .extract()
        .unwrap();
    let records = fetched.records();
    assert!(records
        .iter()
        .any(|record| record.url() == "https://musicbrainz.org/release-group/album-group"));
    assert!(records
        .iter()
        .any(|record| record.url() == "https://www.allmusic.com/album/mw0000007811"));
    assert!(!records
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz && record.release_ref().is_some()));
    let parsed = fetched.every_medium_values().unwrap();
    assert_eq!(parsed.album_title, "Selected Album");
    assert_eq!(parsed.album_year, Some(1980));
    assert_eq!(parsed.pressing.year, Some(2005));
    assert_eq!(crate::catalog_model::release_values::value_labels(&parsed), ["A1", "B1"]);
    assert_eq!(parsed.album_credits[0].credit.musicbrainz_artist_id.as_deref(), Some("artist-id"));
}

#[test]
fn the_pressing_is_the_selected_record_s_alone() {
    let payloads = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        vec![
            SourcePayload::new(
                PayloadSource::MusicBrainzDiscogsXref,
                "7811",
                linked_musicbrainz_release(),
            ),
            SourcePayload::new(
                PayloadSource::MusicBrainz,
                "linked-release",
                linked_musicbrainz_release(),
            ),
            SourcePayload::new(PayloadSource::DiscogsMaster, "7822", linked_discogs_master()),
            SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "album-group",
                linked_release_group(),
            ),
        ],
    );
    let parsed = payloads.extract().unwrap().every_medium_values().unwrap();
    assert_eq!(parsed.album_title, "Selected Album");
    assert_eq!(parsed.album_year, Some(1980));
    assert_eq!(parsed.pressing.year, Some(2005));
    assert_eq!(parsed.pressing.labels[0].name(), Some("Selected Label"));
    assert_eq!(
        parsed.pressing.facts.media,
        bae_types::pressing::made_of(bae_types::pressing::Medium::Vinyl, 1).media,
        "the selected pressing's own media, not the linked release's CD"
    );
    assert_eq!(
        parsed.pressing.facts.area, None,
        "the linked release fills nothing the selected one leaves out"
    );
    assert!(parsed.pressing.barcodes.is_empty());
    assert_eq!(parsed.tracks.len(), 2);
    assert_eq!(parsed.tracks[1].title, "Second Track");
    assert_eq!(crate::catalog_model::release_values::value_labels(&parsed)[1], "B1");
    let detail = payloads.extract().unwrap().detail(&[]);
    assert_eq!(detail.labels, parsed.pressing.labels);
    assert_eq!(detail.facts, parsed.pressing.facts);
    assert_eq!(detail.year, parsed.pressing.year);
}

/// The year an album first came out is resolved as its year is: its own
/// album page's first, and the linked release's only where that states none.
#[test]
fn the_album_page_states_the_first_year_before_the_linked_release() {
    let with_master = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        vec![
            SourcePayload::new(
                PayloadSource::MusicBrainzDiscogsXref,
                "7811",
                linked_musicbrainz_release(),
            ),
            SourcePayload::new(PayloadSource::DiscogsMaster, "7822", linked_discogs_master()),
        ],
    )
    .extract()
    .unwrap();
    assert_eq!(with_master.metadata.album.year, Some(1980));
    assert_eq!(with_master.metadata.album.first_year, Some(1980));

    let without_master = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            linked_musicbrainz_release(),
        )],
    )
    .extract()
    .unwrap();
    assert_eq!(without_master.metadata.album.first_year, Some(1979));
}

#[test]
fn linked_album_fills_missing_title_and_artists_without_setting_pressing_year() {
    let mut anchor: serde_json::Value = serde_json::from_str(&linked_discogs_release()).unwrap();
    anchor["title"] = json!("");
    anchor["artists"] = json!([]);
    anchor["year"] = json!(0);
    let payloads = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        anchor.to_string(),
        vec![SourcePayload::new(PayloadSource::DiscogsMaster, "7822", linked_discogs_master())],
    );
    let parsed = payloads.extract().unwrap().every_medium_values().unwrap();
    assert_eq!(parsed.album_title, "Master Album");
    assert_eq!(parsed.album_credits[0].credit.name, "Selected Artist");
    assert_eq!(parsed.album_year, Some(1980));
    assert_eq!(parsed.pressing.year, None);
}

#[test]
fn linked_release_album_date_survives_an_unavailable_parent_document() {
    let payloads = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            linked_musicbrainz_release(),
        )],
    );
    let parsed = payloads.extract().unwrap().every_medium_values().unwrap();
    assert_eq!(parsed.album_year, Some(1979));
    assert_eq!(parsed.pressing.year, Some(2005));
}

#[test]
fn selected_album_credits_keep_every_artist_in_detail_and_import() {
    let mut anchor: serde_json::Value = serde_json::from_str(&linked_discogs_release()).unwrap();
    anchor["artists"] =
        json!([{ "id":7833, "name":"Selected Artist" }, { "id":7834, "name":"Second Artist" }]);
    let payloads = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        anchor.to_string(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            linked_musicbrainz_release(),
        )],
    );
    assert_eq!(
        payloads.extract().unwrap().detail(&[]).artist.as_deref(),
        Some("Selected Artist, Second Artist")
    );
    let parsed = payloads.extract().unwrap().every_medium_values().unwrap();
    assert_eq!(
        parsed.album_credits.iter().map(|artist| artist.credit.name.as_str()).collect::<Vec<_>>(),
        vec!["Selected Artist", "Second Artist"]
    );
}
