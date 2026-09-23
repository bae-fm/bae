use super::*;
use coven::{FixedClock, SequentialIdProvider};
use serde_json::json;
use std::sync::Arc;

fn instant() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn discogs() -> String {
    json!({
        "id": 7811, "master_id": 7822, "title": "Selected Album", "year": 2005,
        "artists": [{"id": 7833, "name": "Selected Artist"}],
        "labels": [{"name": "Selected Label", "catno": "S-7811"}],
        "formats": [{"name": "Vinyl"}],
        "tracklist": [
            {"position": "A1", "title": "First Track", "type_": "track"},
            {"position": "B1", "title": "Second Track", "type_": "track"}
        ]
    })
    .to_string()
}

fn group() -> String {
    json!({
        "id": "album-group", "title": "Linked Album", "first-release-date": "1979",
        "artist-credit": [{"name": "Selected Artist", "artist": {"id": "artist-id", "name": "Selected Artist"}}],
        "relations": [
            {"url": {"resource": "https://www.discogs.com/master/7822"}},
            {"url": {"resource": "https://www.allmusic.com/album/mw0000007811"}}
        ]
    }).to_string()
}

fn master() -> String {
    json!({"id":7822,"title":"Master Album","year":1980,"artists":[{"id":7833,"name":"Selected Artist"}]}).to_string()
}

fn musicbrainz() -> String {
    json!({
        "id": "linked-release", "title": "Linked Album", "date": "1999", "country": "JP", "barcode": "1234567890123",
        "artist-credit": [{"name":"Selected Artist", "artist":{"id":"artist-id", "name":"Selected Artist"}}],
        "release-group":{"id":"album-group","first-release-date":"1979"},
        "label-info":[{"label":{"name":"Linked Label"},"catalog-number":"OTHER-1"}],
        "media":[{"format":"CD", "tracks":[{"number":"1", "title":"Different Track"}]}],
        "relations":[{"url":{"resource":"https://www.discogs.com/release/7811"}}],
        "cover-art-archive":{"front":false,"darkened":false}
    }).to_string()
}

async fn database() -> (Database, tempfile::TempDir) {
    let temp = tempfile::TempDir::new().unwrap();
    let db = Database::new_test(
        temp.path().join("library.db").to_str().unwrap(),
        Arc::new(FixedClock(instant())),
        Arc::new(SequentialIdProvider::new("enrichment")),
    )
    .await
    .unwrap();
    (db, temp)
}

#[tokio::test]
async fn master_backlink_enriches_an_archived_release_and_replays_offline() {
    let stored = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "7811"),
        anchor: discogs(),
        supporting: vec![SourcePayload::new(
            PayloadSource::DiscogsMaster,
            "7822",
            master(),
        )],
    };
    let providers = crate::providers::Providers::offline();
    providers.musicbrainz().seed_discogs_url_lookup("7811", None);
    providers.musicbrainz().seed_discogs_master_url_lookup("7822", Some("album-group".into()));
    providers.musicbrainz().seed_release_group_json_cache("album-group", group());
    let client = DiscogsClient::new(providers.discogs().clone(), "test-token".into());
    let enriched = providers.enrich_payloads(Some(&client), &stored, CallPriority::Interactive)
        .await
        .unwrap();
    let records = enriched.records().unwrap();
    assert!(records
        .iter()
        .any(|record| record.url() == "https://musicbrainz.org/release-group/album-group"));
    assert!(records
        .iter()
        .any(|record| record.url() == "https://www.allmusic.com/album/mw0000007811"));
    assert!(!records
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz && record.release_ref().is_some()));
    let parsed = enriched
        .parsed(
            &[],
            &FixedClock(instant()),
            &SequentialIdProvider::new("before"),
        )
        .unwrap();
    assert_eq!(parsed.album.title, "Selected Album");
    assert_eq!(parsed.album.year, Some(1980));
    assert_eq!(parsed.release.pressing.year, Some(2005));
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| track.side)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    assert_eq!(
        parsed.artists[0].musicbrainz_artist_id.as_deref(),
        Some("artist-id")
    );
    let (db, _temp) = database().await;
    store(&db, &enriched, instant()).await.unwrap();
    let replay = load(&db, &stored.release).await.unwrap().unwrap();
    assert_eq!(replay.records().unwrap(), records);
    let replayed = replay
        .parsed(
            &[],
            &FixedClock(instant()),
            &SequentialIdProvider::new("after"),
        )
        .unwrap();
    assert_eq!(replayed.album.year, parsed.album.year);
    assert_eq!(replayed.release.pressing, parsed.release.pressing);
    assert_eq!(
        replayed
            .tracks
            .iter()
            .map(|track| (&track.title, track.side))
            .collect::<Vec<_>>(),
        parsed
            .tracks
            .iter()
            .map(|track| (&track.title, track.side))
            .collect::<Vec<_>>()
    );
}

#[test]
fn selected_pressing_wins_and_linked_release_fills_only_absent_details() {
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "7811"),
        anchor: discogs(),
        supporting: vec![
            SourcePayload::new(PayloadSource::MusicBrainzDiscogsXref, "7811", musicbrainz()),
            SourcePayload::new(PayloadSource::MusicBrainz, "linked-release", musicbrainz()),
            SourcePayload::new(PayloadSource::DiscogsMaster, "7822", master()),
            SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "album-group",
                group(),
            ),
        ],
    };
    let parsed = payloads
        .parsed(
            &[],
            &FixedClock(instant()),
            &SequentialIdProvider::new("precedence"),
        )
        .unwrap();
    assert_eq!(parsed.album.title, "Selected Album");
    assert_eq!(parsed.album.year, Some(1980));
    assert_eq!(parsed.release.pressing.year, Some(2005));
    assert_eq!(
        parsed.release.pressing.label.as_deref(),
        Some("Selected Label")
    );
    assert_eq!(parsed.release.pressing.format.as_deref(), Some("Vinyl"));
    assert_eq!(parsed.release.pressing.country.as_deref(), Some("JP"));
    assert_eq!(
        parsed.release.pressing.barcode.as_deref(),
        Some("1234567890123")
    );
    assert_eq!(parsed.tracks.len(), 2);
    assert_eq!(parsed.tracks[1].title, "Second Track");
    assert_eq!(parsed.tracks[1].side, Some(2));
    let detail = payloads.detail_for_audio(&[], &[]).unwrap();
    assert_eq!(detail.label, parsed.release.pressing.label);
    assert_eq!(detail.country, parsed.release.pressing.country);
    assert_eq!(detail.year, parsed.release.pressing.year);
}

#[test]
fn linked_album_fills_missing_title_and_artists_without_setting_pressing_year() {
    let mut anchor: serde_json::Value = serde_json::from_str(&discogs()).unwrap();
    anchor["title"] = json!("");
    anchor["artists"] = json!([]);
    anchor["year"] = json!(0);
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "7811"),
        anchor: anchor.to_string(),
        supporting: vec![SourcePayload::new(
            PayloadSource::DiscogsMaster,
            "7822",
            master(),
        )],
    };
    let parsed = payloads
        .parsed(
            &[],
            &FixedClock(instant()),
            &SequentialIdProvider::new("missing"),
        )
        .unwrap();
    assert_eq!(parsed.album.title, "Master Album");
    assert_eq!(parsed.artists[0].name, "Selected Artist");
    assert_eq!(parsed.album.year, Some(1980));
    assert_eq!(parsed.release.pressing.year, None);
}

#[tokio::test]
async fn replacing_a_reverse_answer_removes_an_obsolete_pressing_alias_atomically() {
    let (db, _temp) = database().await;
    let mut payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "7811"),
        anchor: discogs(),
        supporting: vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            musicbrainz(),
        )],
    };
    store(&db, &payloads, instant()).await.unwrap();
    assert!(load(&db, &payloads.release)
        .await
        .unwrap()
        .unwrap()
        .records()
        .unwrap()
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz && record.release_ref().is_some()));
    payloads.supporting.clear();
    store(&db, &payloads, instant()).await.unwrap();
    assert!(!load(&db, &payloads.release)
        .await
        .unwrap()
        .unwrap()
        .records()
        .unwrap()
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz));
}

#[test]
fn linked_release_album_date_survives_an_unavailable_parent_document() {
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "7811"),
        anchor: discogs(),
        supporting: vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            musicbrainz(),
        )],
    };
    let parsed = payloads
        .parsed(
            &[],
            &FixedClock(instant()),
            &SequentialIdProvider::new("inline-date"),
        )
        .unwrap();
    assert_eq!(parsed.album.year, Some(1979));
    assert_eq!(parsed.release.pressing.year, Some(2005));
}

#[test]
fn selected_album_credits_keep_every_artist_in_detail_and_import() {
    let mut anchor: serde_json::Value = serde_json::from_str(&discogs()).unwrap();
    anchor["artists"] =
        json!([{ "id":7833, "name":"Selected Artist" }, { "id":7834, "name":"Second Artist" }]);
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "7811"),
        anchor: anchor.to_string(),
        supporting: vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            musicbrainz(),
        )],
    };
    assert_eq!(
        payloads.detail_for_audio(&[], &[]).unwrap().artist.as_deref(),
        Some("Selected Artist, Second Artist")
    );
    let parsed = payloads
        .parsed(
            &[],
            &FixedClock(instant()),
            &SequentialIdProvider::new("credits"),
        )
        .unwrap();
    assert_eq!(
        parsed
            .artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Selected Artist", "Second Artist"]
    );
    assert_eq!(parsed.album_artists.len(), 1);
}

#[tokio::test]
async fn unavailable_supporting_document_does_not_return_during_archive_replay() {
    let (db, _temp) = database().await;
    let mut payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "7811"),
        anchor: discogs(),
        supporting: vec![SourcePayload::new(
            PayloadSource::DiscogsMaster,
            "7822",
            master(),
        )],
    };
    store(&db, &payloads, instant()).await.unwrap();
    // A fresh lookup could not obtain the parent. Its replay must describe
    // the same metadata application, without a previously cached parent.
    payloads.supporting.clear();
    store(&db, &payloads, instant()).await.unwrap();
    let replay = load(&db, &payloads.release).await.unwrap().unwrap();
    assert_eq!(replay, payloads);
}
