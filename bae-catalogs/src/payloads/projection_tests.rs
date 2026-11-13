use super::*;
use crate::test_documents::{
    linked_discogs_master, linked_discogs_release, linked_musicbrainz_release, linked_release_group,
};
use serde_json::json;

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
    assert_eq!(crate::release_values::value_labels(&parsed)[1], "B1");
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
        payloads
            .extract()
            .unwrap()
            .detail(&[])
            .artist
            .as_ref()
            .map(bae_types::credits::CreditLine::plain_text)
            .as_deref(),
        Some("Selected Artist, Second Artist")
    );
    let parsed = payloads.extract().unwrap().every_medium_values().unwrap();
    assert_eq!(
        parsed.album_credits.iter().map(|artist| artist.credit.name.as_str()).collect::<Vec<_>>(),
        vec!["Selected Artist", "Second Artist"]
    );
}

fn parse(payloads: &ReleasePayloads) -> Result<crate::release_values::ReleaseValues, CatalogError> {
    payloads.extract().and_then(|release| release.every_medium_values())
}

fn selected_release() -> ReleasePayloads {
    ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        json!({
            "id":7811, "master_id":7822, "title":"Selected Album", "year":0,
            "artists":[{"id":7833,"name":"Selected Artist"}],
            "formats":[{"name":"Vinyl"}],
            "tracklist":[
                {"position":"A1","title":"First Track","type_":"track"},
                {"position":"B1","title":"Second Track","type_":"track"}
            ]
        })
        .to_string(),
        vec![],
    )
}

fn supplemental_credits() -> serde_json::Value {
    json!([
        {"name":"", "artist":{"id":"unresolved-artist","name":null}},
        {"name":"Selected Artist", "artist":{"id":"linked-artist","name":"Selected Artist"}}
    ])
}

#[test]
fn malformed_group_credit_preserves_selected_tracks_and_valid_supplemental_fields() {
    let mut payloads = selected_release();
    payloads.supporting.push(SourcePayload::new(
        PayloadSource::MusicBrainzReleaseGroup,
        "linked-group",
        json!({"id":"linked-group","title":"Other Album","first-release-date":"1979",
            "artist-credit":supplemental_credits()})
        .to_string(),
    ));
    payloads.supporting.push(SourcePayload::new(
        PayloadSource::MusicBrainzDiscogsMasterXref,
        "7822",
        payloads.supporting[0].json.clone(),
    ));
    let parsed = parse(&payloads).unwrap();
    assert_eq!(parsed.album_title, "Selected Album");
    assert_eq!(parsed.album_year, Some(1979));
    assert_eq!(parsed.pressing.year, None);
    assert_eq!(parsed.album_credits[0].credit.name, "Selected Artist");
    assert_eq!(
        parsed.album_credits[0].credit.musicbrainz_artist_id.as_deref(),
        Some("linked-artist")
    );
    assert_eq!(
        parsed.tracks.iter().map(|track| track.title.as_str()).collect::<Vec<_>>(),
        ["First Track", "Second Track"]
    );
    assert_eq!(crate::release_values::value_labels(&parsed), ["A1", "B1"]);
}

#[test]
fn malformed_linked_release_credit_keeps_its_valid_credit_and_fills_no_pressing_field() {
    let mut payloads = selected_release();
    payloads.supporting.push(SourcePayload::new(
        PayloadSource::MusicBrainzDiscogsXref,
        "7811",
        json!({"id":"linked-release","title":"Other Album","date":"1985","country":"JP",
            "barcode":"1234567890123","artist-credit":supplemental_credits(),
            "release-group":{"id":"linked-group","first-release-date":"1979"},
            "cover-art-archive":{"front":false,"darkened":false}})
        .to_string(),
    ));
    let parsed = parse(&payloads).unwrap();
    assert_eq!(parsed.album_title, "Selected Album");
    assert_eq!(parsed.album_year, Some(1979));
    // The linked record is the same pressing on the other catalog; the
    // selected record states none of these, and the linked one fills none.
    assert_eq!(parsed.pressing.year, None);
    assert_eq!(parsed.pressing.facts.area, None);
    assert!(parsed.pressing.barcodes.is_empty());
    assert_eq!(
        parsed.album_credits[0].credit.musicbrainz_artist_id.as_deref(),
        Some("linked-artist")
    );
    assert_eq!(parsed.tracks[1].title, "Second Track");
    assert_eq!(crate::release_values::value_labels(&parsed)[1], "B1");
}

#[test]
fn malformed_selected_release_credit_is_still_rejected() {
    let payloads = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::MusicBrainz, "selected-release"),
        json!({"id":"selected-release","title":"Selected Album",
            "artist-credit":supplemental_credits(),
            "media":[{"format":"CD","tracks":[{"number":"1","title":"First Track"}]}],
            "cover-art-archive":{"front":false,"darkened":false}})
        .to_string(),
        vec![],
    );
    assert!(matches!(
        parse(&payloads),
        Err(CatalogError::SourceData { catalog: Catalog::MusicBrainz, .. })
    ));
}

#[test]
fn malformed_optional_documents_do_not_block_extraction() {
    for (source, malformed) in [
        (PayloadSource::MusicBrainzDiscogsXref, "{}"),
        (PayloadSource::MusicBrainzReleaseGroup, "{}"),
        (PayloadSource::DiscogsMaster, "{}"),
        (PayloadSource::DiscogsMaster, r#"{"id":7811,"images":[{"uri":23}]}"#),
        (PayloadSource::Wikidata, "{}"),
    ] {
        let selected = selected_release();
        let mut supporting = selected.supporting.clone();
        supporting.push(SourcePayload::new(source, "7811", malformed.into()));
        supporting.push(SourcePayload::new(
            PayloadSource::DiscogsMaster,
            "7822",
            json!({"id":7822,"title":"Parent Album","year":1979}).to_string(),
        ));
        // A fetch admits each supporting document as it arrives.
        let payloads = ReleasePayloads::for_test(
            selected.release.clone(),
            selected.anchor.clone(),
            supporting,
        );
        let parsed = parse(&payloads).unwrap_or_else(|error| panic!("{source:?}: {error}"));
        assert_eq!(parsed.album_title, "Selected Album");
        assert_eq!(parsed.album_year, Some(1979));
        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(payloads.extract().unwrap().records().len(), 1);
        assert!(payloads.extract().unwrap().covers().is_empty());
        assert_eq!(payloads.extract().unwrap().detail(&[]).title, "Selected Album");
        assert_eq!(payloads.supporting.len(), 1, "only usable documents are admitted");
    }
}
