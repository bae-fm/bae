use super::*;
use crate::db::DbSourceReleasePayload;
use coven::{FixedClock, SequentialIdProvider};
use std::sync::Arc;

#[test]
fn discogs_cover_choices_keep_every_image() {
    let payloads = ReleasePayloads {
        release: MetadataRef::new("123", MetadataSource::Discogs),
        source_group_id: None,
        document_release_id: "123".to_string(),
        anchor: serde_json::json!({
            "id": 123,
            "title": "Album Title",
            "images": [
                { "type": "secondary", "uri": "https://images.example/back.jpg", "uri150": "https://images.example/back-small.jpg" },
                { "type": "primary", "uri": "https://images.example/front.jpg", "uri150": "https://images.example/front-small.jpg" }
            ]
        }).to_string(),
        supporting: vec![],
    };
    let covers = payloads.covers().expect("cover choices parse");
    assert_eq!(covers.len(), 2);
    assert_eq!(covers[0].url, "https://images.example/front.jpg");
    assert_eq!(covers[1].url, "https://images.example/back.jpg");
    assert_eq!(
        covers[1].thumbnail_url,
        "https://images.example/back-small.jpg"
    );
}

fn now() -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .expect("a valid test instant")
        .with_timezone(&Utc)
}

#[test]
fn cover_choices_include_cross_references_and_deduplicate_master_images() {
    let discogs = serde_json::json!({
        "id": 123, "title": "Album Title", "master_id": 456,
        "images": [{ "type": "primary", "uri": "https://images.example/front.jpg" }]
    })
    .to_string();
    let musicbrainz = serde_json::json!({
        "id": "mb-release", "title": "Album Title",
        "artist-credit": [], "label-info": [], "media": [], "relations": [],
        "release-group": { "id": "mb-group" },
        "cover-art-archive": { "front": true, "darkened": false }
    })
    .to_string();
    let master = SourcePayload::new(
        PayloadSource::DiscogsMaster,
        "456",
        serde_json::json!({
            "id": 456, "images": [
                { "type": "primary", "uri": "https://images.example/front.jpg" },
                { "type": "secondary", "uri": "https://images.example/booklet.jpg" }
            ]
        })
        .to_string(),
    );
    for (source, anchor, supporting) in [
        (
            MetadataSource::Discogs,
            discogs.clone(),
            SourcePayload::new(
                PayloadSource::MusicBrainzDiscogsXref,
                "123",
                musicbrainz.clone(),
            ),
        ),
        (
            MetadataSource::MusicBrainz,
            musicbrainz,
            SourcePayload::new(PayloadSource::Discogs, "123", discogs),
        ),
    ] {
        let covers = ReleasePayloads {
            release: MetadataRef::new("source-release", source),
            source_group_id: None,
            document_release_id: "source-release".to_string(),
            anchor,
            supporting: vec![supporting, master.clone()],
        }
        .covers()
        .expect("all archived artwork parses");
        assert_eq!(covers.len(), 4);
        assert_eq!(
            covers
                .iter()
                .filter(|cover| cover.source == MetadataSource::Discogs)
                .count(),
            2
        );
        assert_eq!(
            covers
                .iter()
                .filter(|cover| cover.url == "https://images.example/front.jpg")
                .count(),
            1
        );
        assert!(covers
            .iter()
            .any(|cover| cover.url == "https://images.example/booklet.jpg"));
    }
}

async fn test_database() -> (Database, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().expect("a temp library dir");
    let path = dir.path().join("test.db");
    let database = Database::new_test(
        path.to_str().expect("a UTF-8 temp path"),
        Arc::new(FixedClock(now())),
        Arc::new(SequentialIdProvider::new("payload")),
    )
    .await
    .expect("the test database opens");
    (database, dir)
}

async fn archive(database: &Database, rows: &[(PayloadSource, &str, serde_json::Value)]) {
    let rows: Vec<DbSourceReleasePayload> = rows
        .iter()
        .map(|(source, id, json)| DbSourceReleasePayload {
            source: *source,
            source_release_id: (*id).to_string(),
            json: json.to_string(),
            fetched_at: now(),
        })
        .collect();
    database
        .save_source_release_payloads(&rows)
        .await
        .expect("the documents archive");
}

fn discogs_release(id: u64, master_id: u64, year: u32) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": "Album Title",
        "year": year,
        "master_id": master_id,
        "artists": [{ "id": 1, "name": "Artist Name" }],
        "tracklist": [
            { "position": "1", "title": "Track Title", "type_": "track", "artists": [] }
        ],
    })
}

/// A Discogs release names its master, and the master states the year the
/// album first came out — 1967 for a 1985 reissue. Reading the set back has
/// to follow that name out of the *anchor*, which is where a
/// Discogs-seeded release's own document lives; a reader that only looked
/// at the supporting documents would find no Discogs release there and
/// silently fall back to the pressing's own year.
#[tokio::test]
async fn a_discogs_release_reaches_its_master_through_the_anchor() {
    let (database, _dir) = test_database().await;
    archive(
        &database,
        &[
            (
                PayloadSource::Discogs,
                "12345",
                discogs_release(12345, 99, 1985),
            ),
            (
                PayloadSource::DiscogsMaster,
                "99",
                serde_json::json!({ "id": 99, "year": 1967 }),
            ),
        ],
    )
    .await;

    let payloads = load(
        &database,
        &MetadataRef::new("12345", MetadataSource::Discogs),
    )
    .await
    .expect("the stored set reads back")
    .expect("the anchor is archived");

    let parsed = payloads
        .parsed(&[], &FixedClock(now()), &SequentialIdProvider::new("album"))
        .expect("the stored documents map");
    assert_eq!(
        parsed.album.year,
        Some(1967),
        "the album year is the master's, not the pressing's"
    );
    assert_eq!(parsed.release.pressing.year, Some(1985));
}

/// Nothing archived is not a half-read set: the anchor's absence is the
/// whole answer, and no supporting key is guessed from a release nobody
/// fetched.
#[tokio::test]
async fn an_unfetched_release_reads_back_as_nothing() {
    let (database, _dir) = test_database().await;
    let payloads = load(
        &database,
        &MetadataRef::new("never-fetched", MetadataSource::MusicBrainz),
    )
    .await
    .expect("the read succeeds");
    assert!(payloads.is_none());
}
#[tokio::test]
async fn loading_documents_does_not_parse_editor_metadata() {
    let (database, _dir) = test_database().await;
    archive(
        &database,
        &[(
            PayloadSource::MusicBrainz,
            "release",
            serde_json::json!({
                "id": "release", "title": "Album Title",
                "release-group": {"id": "group"},
                "artist-credit": "unreadable editor metadata",
                "cover-art-archive": {"front": false, "darkened": false}
            }),
        )],
    )
    .await;
    let raw = load(
        &database,
        &MetadataRef::new("release", MetadataSource::MusicBrainz),
    )
    .await
    .expect("loading owned documents must not parse editor metadata")
    .expect("stored anchor");
    assert!(
        raw.detail_for_audio(&[]).is_err(),
        "processing still reports malformed provider data"
    );
}

fn musicbrainz_release(group: &str, discogs_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "mb-release", "title": "Album Title",
        "release-group": {"id": group},
        "relations": [{"url": {"resource": format!("https://www.discogs.com/release/{discogs_id}-Album-Title")}}],
        "artist-credit": [{"name": "Artist Name"}],
        "cover-art-archive": {"front": false, "darkened": false}
    })
}

fn supporting_keys(payloads: &ReleasePayloads) -> Vec<(PayloadSource, &str)> {
    payloads
        .supporting
        .iter()
        .map(|document| (document.source, document.source_release_id.as_str()))
        .collect()
}

#[tokio::test]
async fn references_observe_support_added_and_updated_after_the_anchor() {
    let (database, _dir) = test_database().await;
    archive(
        &database,
        &[(
            PayloadSource::MusicBrainz,
            "mb-release",
            musicbrainz_release("group", "123"),
        )],
    )
    .await;
    let release = MetadataRef::new("mb-release", MetadataSource::MusicBrainz);
    let raw = load(&database, &release)
        .await
        .expect("load")
        .expect("anchor");
    assert!(raw.supporting.is_empty(), "unfetched support is optional");
    assert_eq!(raw.source_group_id(), Some("group"));
    assert_eq!(
        raw.covers().expect("anchor artwork"),
        vec![RemoteCover::musicbrainz_release_group("group")]
    );

    archive(&database, &[
        (PayloadSource::MusicBrainzReleaseGroup, "group", serde_json::json!({"id":"group"})),
        (PayloadSource::Discogs, "123", discogs_release(123, 456, 2000)),
        (PayloadSource::DiscogsMaster, "456", serde_json::json!({"id":456,"year":1990,"images":[{"type":"primary","uri":"https://images.example/master.jpg"}]})),
        // A Discogs back-reference must not make the MusicBrainz traversal recursive.
        (PayloadSource::MusicBrainzDiscogsXref, "123", serde_json::json!({"artist-credit": "unreadable unused document"})),
    ]).await;
    let raw = load(&database, &release)
        .await
        .expect("load")
        .expect("anchor");
    assert_eq!(
        supporting_keys(&raw),
        vec![
            (PayloadSource::Discogs, "123"),
            (PayloadSource::MusicBrainzReleaseGroup, "group"),
            (PayloadSource::DiscogsMaster, "456")
        ]
    );
    assert_eq!(
        raw.covers().expect("late master artwork")[1].url,
        "https://images.example/master.jpg"
    );

    archive(&database, &[(PayloadSource::DiscogsMaster, "456", serde_json::json!({"id":456,"year":1990,"images":[{"type":"primary","uri":"https://images.example/revised.jpg"}]}))]).await;
    let raw = load(&database, &release)
        .await
        .expect("load")
        .expect("anchor");
    assert_eq!(
        raw.covers().expect("updated shared master")[1].url,
        "https://images.example/revised.jpg"
    );
}

#[tokio::test]
async fn replacement_documents_replace_their_relationships() {
    let (database, _dir) = test_database().await;
    archive(
        &database,
        &[
            (
                PayloadSource::MusicBrainz,
                "mb-release",
                musicbrainz_release("old-group", "123"),
            ),
            (
                PayloadSource::Discogs,
                "123",
                discogs_release(123, 456, 2000),
            ),
            (
                PayloadSource::DiscogsMaster,
                "456",
                serde_json::json!({"id":456}),
            ),
            (
                PayloadSource::MusicBrainzReleaseGroup,
                "old-group",
                serde_json::json!({"id":"old-group"}),
            ),
            (
                PayloadSource::Discogs,
                "789",
                discogs_release(789, 999, 2000),
            ),
            (
                PayloadSource::DiscogsMaster,
                "999",
                serde_json::json!({"id":999}),
            ),
        ],
    )
    .await;
    archive(
        &database,
        &[(
            PayloadSource::MusicBrainz,
            "mb-release",
            musicbrainz_release("new-group", "789"),
        )],
    )
    .await;
    let release = MetadataRef::new("mb-release", MetadataSource::MusicBrainz);
    let raw = load(&database, &release)
        .await
        .expect("load")
        .expect("anchor");
    assert_eq!(raw.source_group_id(), Some("new-group"));
    assert_eq!(
        supporting_keys(&raw),
        vec![
            (PayloadSource::Discogs, "789"),
            (PayloadSource::DiscogsMaster, "999")
        ]
    );
    archive(
        &database,
        &[(
            PayloadSource::Discogs,
            "789",
            serde_json::json!({"id":789,"title":"Album Title"}),
        )],
    )
    .await;
    let raw = load(&database, &release)
        .await
        .expect("load")
        .expect("anchor");
    assert_eq!(
        supporting_keys(&raw),
        vec![(PayloadSource::Discogs, "789")],
        "replacing the supporting release retires its old master reference"
    );
}

#[tokio::test]
async fn document_batch_failure_preserves_payloads_and_references() {
    let (database, _dir) = test_database().await;
    archive(
        &database,
        &[(
            PayloadSource::MusicBrainz,
            "mb-release",
            musicbrainz_release("original", "123"),
        )],
    )
    .await;
    let rows = [
        DbSourceReleasePayload {
            source: PayloadSource::MusicBrainz,
            source_release_id: "mb-release".to_string(),
            json: musicbrainz_release("replacement", "789").to_string(),
            fetched_at: now(),
        },
        DbSourceReleasePayload {
            source: PayloadSource::Discogs,
            source_release_id: "789".to_string(),
            json: "{\"master_id\":\"unreadable\"}".to_string(),
            fetched_at: now(),
        },
    ];
    assert!(database.save_source_release_payloads(&rows).await.is_err());
    let raw = load(
        &database,
        &MetadataRef::new("mb-release", MetadataSource::MusicBrainz),
    )
    .await
    .expect("load")
    .expect("anchor");
    assert_eq!(raw.source_group_id(), Some("original"));
    assert_eq!(
        raw.anchor,
        musicbrainz_release("original", "123").to_string()
    );
}

#[tokio::test]
async fn stored_group_id_matches_the_processed_detail() {
    let (database, _dir) = test_database().await;
    for (source, id, json, group) in [
        (
            PayloadSource::MusicBrainz,
            "mb-release",
            musicbrainz_release("group", "123"),
            Some("group"),
        ),
        (
            PayloadSource::Discogs,
            "123",
            discogs_release(123, 456, 2000),
            Some("456"),
        ),
        (
            PayloadSource::Discogs,
            "789",
            serde_json::json!({"id":789,"title":"Album Title"}),
            None,
        ),
    ] {
        archive(&database, &[(source, id, json)]).await;
        let source = match source {
            PayloadSource::MusicBrainz => MetadataSource::MusicBrainz,
            PayloadSource::Discogs => MetadataSource::Discogs,
            _ => unreachable!(),
        };
        let raw = load(&database, &MetadataRef::new(id, source))
            .await
            .expect("load")
            .expect("anchor");
        assert_eq!(raw.source_group_id(), group);
        assert_eq!(raw.source_release_id(), id);
        assert_eq!(
            raw.parse()
                .expect("parse once")
                .detail_for_audio(&[])
                .expect("detail")
                .source_group_id
                .as_deref(),
            group
        );
    }
}

#[tokio::test]
async fn stored_document_identity_and_owned_projections_preserve_the_provider_id() {
    let (database, _dir) = test_database().await;
    archive(
        &database,
        &[(
            PayloadSource::MusicBrainz,
            "archive-key",
            musicbrainz_release("group", "123"),
        )],
    )
    .await;
    let raw = load(
        &database,
        &MetadataRef::new("archive-key", MetadataSource::MusicBrainz),
    )
    .await
    .expect("load")
    .expect("anchor");
    assert_eq!(raw.source_release_id(), "mb-release");
    let parsed = raw.parse().expect("parse the processing result once");
    drop(raw);
    assert_eq!(
        parsed.identity().expect("identity").source_release_id,
        "mb-release"
    );
    let detail = parsed
        .detail_for_audio(&[])
        .expect("detail from the same models");
    assert_eq!(detail.release_id, "mb-release");
    assert_eq!(
        detail.cover_art,
        parsed.covers().expect("artwork from the same models")
    );
}
