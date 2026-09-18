use super::*;
use crate::db::DbSourceReleasePayload;
use coven::{FixedClock, SequentialIdProvider};
use std::sync::Arc;

#[test]
fn discogs_cover_choices_keep_every_image() {
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "123"),
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

#[test]
fn library_check_matches_detail_keys_with_and_without_source_groups() {
    for group in [None, Some(789)] {
        for source in [Catalog::Discogs, Catalog::MusicBrainz] {
            let anchor = match source {
                Catalog::Discogs => serde_json::json!({
                    "id": 123, "title": "Album Title", "master_id": group
                }),
                Catalog::MusicBrainz => serde_json::json!({
                    "id": "123", "title": "Album Title", "artist-credit": [],
                    "label-info": [], "media": [], "relations": [],
                    "cover-art-archive": { "front": false, "darkened": false },
                    "release-group": group.map(|id| serde_json::json!({ "id": id.to_string() }))
                }),
                other => unreachable!("nothing fetches documents from {}", other.as_str()),
            };
            let payloads = ReleasePayloads {
                release: MetadataRef::new(source, "123"),
                anchor: anchor.to_string(),
                supporting: Vec::new(),
            };
            let check = payloads.library_check().unwrap();
            let detail = payloads.detail_for_audio(&[]).unwrap();
            assert_eq!(check.source, detail.source);
            assert_eq!(check.release_id, detail.release_id);
            assert_eq!(check.source_group_id, detail.source_group_id);
            assert_eq!(check.source_group_id, group.map(|id| id.to_string()));
        }
    }
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
            Catalog::Discogs,
            discogs.clone(),
            SourcePayload::new(
                PayloadSource::MusicBrainzDiscogsXref,
                "123",
                musicbrainz.clone(),
            ),
        ),
        (
            Catalog::MusicBrainz,
            musicbrainz,
            SourcePayload::new(PayloadSource::Discogs, "123", discogs),
        ),
    ] {
        let covers = ReleasePayloads {
            release: MetadataRef::new(source, "source-release"),
            anchor,
            supporting: vec![supporting, master.clone()],
        }
        .covers()
        .expect("all archived artwork parses");
        assert_eq!(covers.len(), 4);
        assert_eq!(
            covers
                .iter()
                .filter(|cover| cover.source == Catalog::Discogs)
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

fn mb_release_with_relations(
    id: &str,
    group_id: &str,
    urls: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "title": "Album Title",
        "artist-credit": [],
        "label-info": [],
        "media": [],
        "release-group": { "id": group_id },
        "cover-art-archive": { "front": false, "darkened": false },
        "relations": urls
            .iter()
            .map(|url| serde_json::json!({
                "target-type": "url",
                "type": "other databases",
                "url": { "resource": url }
            }))
            .collect::<Vec<_>>(),
    })
}

/// Every URL relation an editor filed on the release or on its release
/// group is another catalog's description of the same object. An address
/// no catalog bae knows publishes at is not one.
#[test]
fn a_musicbrainz_release_records_every_catalog_it_links_out_to() {
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        anchor: mb_release_with_relations(
            "mb-release",
            "mb-group",
            &[
                "https://www.discogs.com/release/4242-Album-Title",
                "https://www.wikidata.org/wiki/Q424242",
                "https://example.net/not-a-catalog/4242",
            ],
        )
        .to_string(),
        supporting: vec![SourcePayload::new(
            PayloadSource::MusicBrainzReleaseGroup,
            "mb-group",
            serde_json::json!({
                "relations": [
                    { "url": { "resource": "https://www.allmusic.com/album/mw0000424242" } },
                    { "url": { "resource": "https://rateyourmusic.com/release/album/artist-name/album-title/" } },
                    { "url": { "resource": "https://artist-name.bandcamp.com/album/album-title" } },
                ]
            })
            .to_string(),
        )],
    };

    let records = payloads.records().expect("the stored documents read");
    let described: Vec<(Catalog, &str, &str, &str, bool)> = records
        .iter()
        .map(|record| {
            (
                record.catalog,
                record.key.as_str(),
                record.group_key.as_str(),
                record.url.as_str(),
                record.reads_draft,
            )
        })
        .collect();
    assert_eq!(
        described,
        vec![
            (
                Catalog::MusicBrainz,
                "mb-release",
                "mb-group",
                "https://musicbrainz.org/release/mb-release",
                true,
            ),
            (
                Catalog::Discogs,
                "4242",
                "4242",
                "https://www.discogs.com/release/4242",
                false,
            ),
            (
                Catalog::AllMusic,
                "mw0000424242",
                "mw0000424242",
                "https://www.allmusic.com/album/mw0000424242",
                false,
            ),
            (
                Catalog::Bandcamp,
                "artist-name.bandcamp.com/album/album-title",
                "artist-name.bandcamp.com/album/album-title",
                "https://artist-name.bandcamp.com/album/album-title",
                false,
            ),
            (
                Catalog::RateYourMusic,
                "album/artist-name/album-title",
                "album/artist-name/album-title",
                "https://rateyourmusic.com/release/album/artist-name/album-title",
                false,
            ),
            (
                Catalog::Wikidata,
                "Q424242",
                "Q424242",
                "https://www.wikidata.org/wiki/Q424242",
                false,
            ),
        ],
        "a catalog that groups nothing files every release as its own group"
    );
}

/// A MusicBrainz release that names no release group is one the catalog did
/// not group, which is what the rest of the catalogs say about every release
/// they list: it stands as its own group.
#[test]
fn a_musicbrainz_release_with_no_release_group_stands_as_its_own_group() {
    let mut anchor = mb_release_with_relations("mb-release", "mb-group", &[]);
    anchor["release-group"] = serde_json::Value::Null;
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        anchor: anchor.to_string(),
        supporting: Vec::new(),
    };

    let records = payloads.records().expect("the stored document reads");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].group_key, "mb-release");
}

/// A master URL on the release names the group of a catalog that already
/// has a record, not a record of its own: a record names a pressing.
#[test]
fn a_linked_group_page_fills_in_its_catalogs_group() {
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        anchor: mb_release_with_relations(
            "mb-release",
            "mb-group",
            &[
                "https://www.discogs.com/master/909090",
                "https://www.discogs.com/release/4242",
            ],
        )
        .to_string(),
        supporting: Vec::new(),
    };

    let records = payloads.records().expect("the stored document reads");
    let discogs = records
        .iter()
        .find(|record| record.catalog == Catalog::Discogs)
        .expect("the linked Discogs release is a record");
    assert_eq!(discogs.key, "4242");
    assert_eq!(discogs.group_key, "909090");
}

/// A url-rel names a release page and nothing above it. The cross-linked
/// Discogs release was fetched along with the anchor, and only that document
/// says which master the release belongs to.
#[test]
fn the_archived_cross_reference_names_the_discogs_master() {
    let payloads = ReleasePayloads {
        release: MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        anchor: mb_release_with_relations(
            "mb-release",
            "mb-group",
            &["https://www.discogs.com/release/4242"],
        )
        .to_string(),
        supporting: vec![SourcePayload::new(
            PayloadSource::Discogs,
            "4242",
            serde_json::json!({
                "id": 4242, "title": "Album Title", "master_id": 909090
            })
            .to_string(),
        )],
    };

    let records = payloads.records().expect("the stored documents read");
    let discogs = records
        .iter()
        .find(|record| record.catalog == Catalog::Discogs)
        .expect("the linked Discogs release is a record");
    assert_eq!(discogs.key, "4242");
    assert_eq!(discogs.group_key, "909090");
}

/// A pick claims one release per catalog. What the primary's document says
/// about another catalog stands, unless the person claimed that catalog
/// themselves — then their release outranks the cross-link. Only the
/// primary's own record reads the draft.
#[test]
fn a_partner_outranks_what_the_primary_says_about_its_catalog() {
    let primary = ReleasePayloads {
        release: MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        anchor: mb_release_with_relations(
            "mb-release",
            "mb-group",
            &["https://www.discogs.com/release/1111"],
        )
        .to_string(),
        supporting: Vec::new(),
    };
    let partner = ReleasePayloads {
        release: MetadataRef::new(Catalog::Discogs, "2222"),
        anchor: serde_json::json!({
            "id": 2222, "title": "Album Title", "master_id": 909090
        })
        .to_string(),
        supporting: Vec::new(),
    };

    let records = claimed_records(&[
        (primary.release().clone(), Some(primary)),
        (partner.release().clone(), Some(partner)),
    ])
    .expect("the claimed documents read");

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].catalog, Catalog::MusicBrainz);
    assert!(records[0].reads_draft);
    assert_eq!(records[1].catalog, Catalog::Discogs);
    assert_eq!(
        records[1].key, "2222",
        "the picked Discogs release outranks the cross-linked one"
    );
    assert_eq!(records[1].group_key, "909090");
    assert!(!records[1].reads_draft);
}

/// A claimed release nothing archived documents for still contributes its
/// own record: the pick claims it either way.
#[test]
fn a_claimed_release_with_no_documents_still_has_a_record() {
    let records = claimed_records(&[(MetadataRef::new(Catalog::Discogs, "4242"), None)])
        .expect("a claim with no documents reads");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].catalog, Catalog::Discogs);
    assert_eq!(records[0].url, "https://www.discogs.com/release/4242");
    assert!(records[0].reads_draft);
    assert_eq!(
        records[0].group_key, "4242",
        "a release its catalog did not group is its own group"
    );
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
        &MetadataRef::new(Catalog::Discogs, "12345"),
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
        &MetadataRef::new(Catalog::MusicBrainz, "never-fetched"),
    )
    .await
    .expect("the read succeeds");
    assert!(payloads.is_none());
}
