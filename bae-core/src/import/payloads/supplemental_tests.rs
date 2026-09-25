use super::*;
use crate::import::ParsedAlbum;
use chrono::{DateTime, Utc};
use coven::{FixedClock, SequentialIdProvider};
use serde_json::json;

fn parse(payloads: &ReleasePayloads) -> Result<ParsedAlbum, ImportError> {
    let clock = FixedClock(
        DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    );
    payloads
        .extract()
        .and_then(|release| release.parsed(&[], &clock, &SequentialIdProvider::new("supplemental")))
}

fn selected_release() -> ReleasePayloads {
    ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7811"),
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
    assert_eq!(parsed.album.title, "Selected Album");
    assert_eq!(parsed.album.year, Some(1979));
    assert_eq!(parsed.release.pressing.year, None);
    assert_eq!(parsed.artists[0].name, "Selected Artist");
    assert_eq!(
        parsed.artists[0].musicbrainz_artist_id.as_deref(),
        Some("linked-artist")
    );
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| (track.title.as_str(), track.side))
            .collect::<Vec<_>>(),
        vec![("First Track", Some(1)), ("Second Track", Some(2))]
    );
}

#[test]
fn malformed_linked_release_credit_keeps_its_pressing_fields_and_valid_credit() {
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
    assert_eq!(parsed.album.title, "Selected Album");
    assert_eq!(parsed.album.year, Some(1979));
    assert_eq!(parsed.release.pressing.year, Some(1985));
    assert_eq!(parsed.release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(
        parsed.release.pressing.barcode.as_deref(),
        Some("1234567890123")
    );
    assert_eq!(
        parsed.artists[0].musicbrainz_artist_id.as_deref(),
        Some("linked-artist")
    );
    assert_eq!(parsed.tracks[1].title, "Second Track");
    assert_eq!(parsed.tracks[1].side, Some(2));
}

#[test]
fn malformed_selected_release_credit_is_still_rejected() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "selected-release"),
        json!({"id":"selected-release","title":"Selected Album",
            "artist-credit":supplemental_credits(),
            "media":[{"format":"CD","tracks":[{"number":"1","title":"First Track"}]}],
            "cover-art-archive":{"front":false,"darkened":false}})
        .to_string(),
        vec![],
    );
    assert!(matches!(
        parse(&payloads),
        Err(ImportError::SourceData {
            catalog: Catalog::MusicBrainz,
            ..
        })
    ));
}

#[test]
fn malformed_optional_documents_do_not_block_extraction() {
    for (source, malformed) in [
        (PayloadSource::MusicBrainzDiscogsXref, "{}"),
        (PayloadSource::MusicBrainzReleaseGroup, "{}"),
        (PayloadSource::DiscogsMaster, "{}"),
        (
            PayloadSource::DiscogsMaster,
            r#"{"id":7811,"images":[{"uri":23}]}"#,
        ),
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
        assert_eq!(parsed.album.title, "Selected Album");
        assert_eq!(parsed.album.year, Some(1979));
        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(payloads.extract().unwrap().records().len(), 1);
        assert!(payloads.extract().unwrap().covers().is_empty());
        assert_eq!(
            payloads.extract().unwrap().detail_for_audio(&[], &[]).unwrap().title,
            "Selected Album"
        );
        assert_eq!(
            payloads.supporting.len(),
            1,
            "only usable documents are admitted"
        );
    }
}
