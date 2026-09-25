use super::*;
use crate::db::Database;
use chrono::{DateTime, Utc};
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
    )
    .await
    .unwrap();
    (db, temp)
}

/// A Discogs release's master names its MusicBrainz release group back, and
/// the group names the album on other catalogs: the fetch follows the whole
/// chain, and the release it stores reads back as it was extracted.
#[tokio::test]
async fn a_master_backlink_is_followed_and_the_stored_release_reads_back() {
    let providers = crate::providers::Providers::offline();
    providers.discogs().seed_release_cache("7811", discogs());
    providers.discogs().seed_master_cache("7822", master());
    providers.musicbrainz().seed_discogs_url_lookup("7811", None);
    providers.musicbrainz().seed_discogs_master_url_lookup("7822", Some("album-group".into()));
    providers.musicbrainz().seed_release_group_json_cache("album-group", group());
    let client = DiscogsClient::new(providers.discogs().clone(), "test-token".into());
    let fetched = providers
        .fetch_payloads(
            Some(&client),
            &MetadataRef::new(Catalog::Discogs, "7811"),
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
    let parsed = fetched
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
    db.save_source_release(&fetched).await.unwrap();
    let stored = db.load_source_release(fetched.release()).await.unwrap();
    assert_eq!(stored, Some(fetched));
}

#[test]
fn selected_pressing_wins_and_linked_release_fills_only_absent_details() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7811"),
        discogs(),
        vec![
            SourcePayload::new(PayloadSource::MusicBrainzDiscogsXref, "7811", musicbrainz()),
            SourcePayload::new(PayloadSource::MusicBrainz, "linked-release", musicbrainz()),
            SourcePayload::new(PayloadSource::DiscogsMaster, "7822", master()),
            SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "album-group",
                group(),
            ),
        ],
    );
    let parsed = payloads
        .extract()
        .unwrap()
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
    let detail = payloads.extract().unwrap().detail_for_audio(&[], &[]).unwrap();
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
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7811"),
        anchor.to_string(),
        vec![SourcePayload::new(
            PayloadSource::DiscogsMaster,
            "7822",
            master(),
        )],
    );
    let parsed = payloads
        .extract()
        .unwrap()
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

/// A cross-reference the catalog no longer answers is gone from the release
/// once it is fetched again: the new extraction replaces the old one whole.
#[tokio::test]
async fn refetching_without_a_reverse_answer_removes_its_pressing_record() {
    let (db, _temp) = database().await;
    let with_alias = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7811"),
        discogs(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            musicbrainz(),
        )],
    );
    db.save_source_release(&with_alias.extract().unwrap())
        .await
        .unwrap();
    assert!(db
        .load_source_release(&with_alias.release)
        .await
        .unwrap()
        .unwrap()
        .records()
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz && record.release_ref().is_some()));
    let without_alias =
        ReleasePayloads::for_test(MetadataRef::new(Catalog::Discogs, "7811"), discogs(), Vec::new());
    db.save_source_release(&without_alias.extract().unwrap())
        .await
        .unwrap();
    assert!(!db
        .load_source_release(&without_alias.release)
        .await
        .unwrap()
        .unwrap()
        .records()
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz));
}

#[test]
fn linked_release_album_date_survives_an_unavailable_parent_document() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7811"),
        discogs(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            musicbrainz(),
        )],
    );
    let parsed = payloads
        .extract()
        .unwrap()
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
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7811"),
        anchor.to_string(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            musicbrainz(),
        )],
    );
    assert_eq!(
        payloads.extract().unwrap().detail_for_audio(&[], &[]).unwrap().artist.as_deref(),
        Some("Selected Artist, Second Artist")
    );
    let parsed = payloads
        .extract()
        .unwrap()
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

/// A parent a later fetch could not obtain does not come back from the
/// earlier fetch: the stored release is the later fetch's, whole.
#[tokio::test]
async fn an_unavailable_supporting_document_does_not_return_from_an_earlier_fetch() {
    let (db, _temp) = database().await;
    let earlier = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7811"),
        discogs(),
        vec![SourcePayload::new(
            PayloadSource::DiscogsMaster,
            "7822",
            master(),
        )],
    );
    db.save_source_release(&earlier.extract().unwrap())
        .await
        .unwrap();
    let later =
        ReleasePayloads::for_test(MetadataRef::new(Catalog::Discogs, "7811"), discogs(), Vec::new())
            .extract()
            .unwrap();
    db.save_source_release(&later).await.unwrap();
    assert_eq!(
        db.load_source_release(later.release()).await.unwrap(),
        Some(later)
    );
}
