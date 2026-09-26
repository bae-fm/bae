use super::*;
use crate::db::Database;
use crate::import::source_release::{UnfetchedDocument, UnfetchedReason};
use chrono::{DateTime, Utc};
use coven::{FixedClock, SequentialIdProvider};
use std::sync::Arc;

#[test]
fn discogs_cover_choices_keep_every_image() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "123"),
        serde_json::json!({
            "id": 123,
            "title": "Album Title",
            "images": [
                { "type": "secondary", "uri": "https://images.example/back.jpg", "uri150": "https://images.example/back-small.jpg" },
                { "type": "primary", "uri": "https://images.example/front.jpg", "uri150": "https://images.example/front-small.jpg" }
            ]
        }).to_string(),
        vec![],
    );
    let covers = payloads.extract().expect("cover choices parse").covers();
    assert_eq!(covers.len(), 2);
    assert_eq!(covers[0].image.url, "https://images.example/front.jpg");
    assert_eq!(covers[1].image.url, "https://images.example/back.jpg");
    assert_eq!(
        covers[1].image.downscaled,
        vec![crate::import::cover_art::DownscaledCopy {
            url: "https://images.example/back-small.jpg".to_string(),
            max_edge: 150,
        }]
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
            let payloads = ReleasePayloads::for_test(
        MetadataRef::new(source, "123"),
        anchor.to_string(),
        Vec::new(),
    );
            let check = payloads.extract().unwrap().library_check();
            let detail = payloads.extract().unwrap().detail_for_audio(&[], &[]).unwrap();
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
        "artist-credit": [], "label-info": [], "media": [],
        "relations": [{"url":{"resource":"https://www.discogs.com/release/123"}}],
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
        let covers = ReleasePayloads::for_test(
        MetadataRef::new(
                source,
                if source == Catalog::Discogs {
                    "123"
                } else {
                    "mb-release"
                },
            ),
        anchor,
        vec![supporting, master.clone()],
    )
        .extract().expect("all archived artwork parses")
        .covers();
        // The archive's front the release document states, Discogs's front,
        // and the master's booklet; the release group's address, which
        // nothing states, is not offered beside them.
        assert_eq!(covers.len(), 3, "{covers:?}");
        assert!(covers
            .iter()
            .all(|cover| cover.standing == crate::import::cover_art::CoverStanding::Stated));
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
                .filter(|cover| cover.image.url == "https://images.example/front.jpg")
                .count(),
            1
        );
        assert!(covers
            .iter()
            .any(|cover| cover.image.url == "https://images.example/booklet.jpg"));
    }
}

/// A MusicBrainz release the archive holds no front image for.
fn unillustrated_musicbrainz_release(relations: serde_json::Value) -> String {
    serde_json::json!({
        "id": "mb-release", "title": "Album Title",
        "artist-credit": [], "label-info": [], "media": [],
        "relations": relations,
        "release-group": { "id": "mb-group" },
        "cover-art-archive": { "front": false, "darkened": false }
    })
    .to_string()
}

/// A Discogs release with one image.
fn illustrated_discogs_release() -> String {
    serde_json::json!({
        "id": 123, "title": "Album Title",
        "images": [{ "type": "primary", "uri": "https://images.example/front.jpg" }]
    })
    .to_string()
}

/// A pick's covers are the images its claimed releases' documents state, the
/// primary's first. A MusicBrainz primary whose document states the archive
/// holds no front offers nothing of its own, and the album address nothing
/// states is not offered while a partner states an image: the pick's default
/// cover is the partner's.
#[test]
fn a_picks_covers_are_the_images_its_documents_state() {
    let primary = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        unillustrated_musicbrainz_release(serde_json::json!([])),
        Vec::new(),
    );
    let partner = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "123"),
        illustrated_discogs_release(),
        Vec::new(),
    );
    let covers = crate::import::source_release::pick_covers(
        &primary.extract().expect("the primary extracts"),
        &[partner.extract().expect("the partner extracts")],
    );
    assert_eq!(covers.len(), 1, "{covers:?}");
    assert_eq!(covers[0].image.url, "https://images.example/front.jpg");
    assert_eq!(covers[0].source, Catalog::Discogs);
    let alone = primary.extract().expect("the primary's own artwork parses").covers();
    assert_eq!(alone.len(), 1, "{alone:?}");
    assert!(
        alone[0].image.url.ends_with("/release-group/mb-group/front"),
        "with nothing stated, the album's unstated address is what is offered: {alone:?}"
    );
    assert_eq!(
        alone[0].standing,
        crate::import::cover_art::CoverStanding::Unstated
    );
}

/// A Discogs release reachable twice — cross-linked by the primary's own
/// document and claimed as a partner — offers its images once.
#[test]
fn a_release_reachable_twice_offers_its_images_once() {
    let primary = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        unillustrated_musicbrainz_release(
            serde_json::json!([{ "url": { "resource": "https://www.discogs.com/release/123" } }]),
        ),
        vec![SourcePayload::new(
            PayloadSource::Discogs,
            "123",
            illustrated_discogs_release(),
        )],
    );
    let partner = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "123"),
        illustrated_discogs_release(),
        Vec::new(),
    );
    let covers = crate::import::source_release::pick_covers(
        &primary.extract().expect("the primary extracts"),
        &[partner.extract().expect("the partner extracts")],
    );
    assert_eq!(covers.len(), 1, "the image reachable twice is offered once: {covers:?}");
    assert_eq!(covers[0].image.url, "https://images.example/front.jpg");
}

fn mb_release_with_relations(id: &str, group_id: &str, urls: &[&str]) -> serde_json::Value {
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

/// A release-group document whose url-rels name a Wikidata item, which is
/// where MusicBrainz editors file the link.
fn wikidata_linked_release_group(group: &str, item: &str) -> String {
    serde_json::json!({
        "id": group,
        "relations": [
            { "url": { "resource": format!("https://www.wikidata.org/wiki/{item}") } }
        ]
    })
    .to_string()
}

/// One item's entity document, stating the album's key in three catalogs: the
/// Discogs master nothing else names, plus two catalogs MusicBrainz links to
/// neither of.
fn wikidata_item(item: &str) -> String {
    serde_json::json!({
        "entities": {
            item: {
                "claims": {
                    "P1954": [{ "mainsnak": { "datavalue": { "value": "909090" } } }],
                    "P1729": [{ "mainsnak": { "datavalue": { "value": "mw0000424242" } } }],
                    "P2205": [{
                        "mainsnak": { "datavalue": { "value": "4242424242424242424242" } }
                    }]
                }
            }
        }
    })
    .to_string()
}

/// Every URL relation an editor filed on the release or on its release
/// group is another catalog's description of the same object. An address
/// no catalog bae knows publishes at is not one.
#[test]
fn a_musicbrainz_release_records_every_catalog_it_links_out_to() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        mb_release_with_relations(
            "mb-release",
            "mb-group",
            &[
                "https://www.discogs.com/release/4242-Album-Title",
                "https://www.wikidata.org/wiki/Q424242",
                "https://example.net/not-a-catalog/4242",
            ],
        )
        .to_string(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzReleaseGroup,
            "mb-group",
            serde_json::json!({
                "id": "mb-group",
                "relations": [
                    { "url": { "resource": "https://www.allmusic.com/album/mw0000424242" } },
                    { "url": { "resource": "https://rateyourmusic.com/release/album/artist-name/album-title/" } },
                    { "url": { "resource": "https://artist-name.bandcamp.com/album/album-title" } },
                ]
            })
            .to_string(),
        )],
    );

    let records = payloads.extract().expect("the stored documents read").records();
    let described: Vec<(Catalog, &str, Option<String>, String, bool)> = records
        .iter()
        .map(|record| {
            (
                record.catalog(),
                record.key(),
                record.album_ref().map(|album| album.key),
                record.url(),
                record.reads_draft(),
            )
        })
        .collect();
    assert_eq!(
        described,
        vec![
            (
                Catalog::MusicBrainz,
                "mb-release",
                Some("mb-group".to_owned()),
                "https://musicbrainz.org/release/mb-release".to_owned(),
                true,
            ),
            (
                Catalog::Discogs,
                "4242",
                None,
                "https://www.discogs.com/release/4242".to_owned(),
                false,
            ),
            (
                Catalog::AllMusic,
                "mw0000424242",
                Some("mw0000424242".to_owned()),
                "https://www.allmusic.com/album/mw0000424242".to_owned(),
                false,
            ),
            (
                Catalog::Bandcamp,
                "artist-name.bandcamp.com/album/album-title",
                Some("artist-name.bandcamp.com/album/album-title".to_owned()),
                "https://artist-name.bandcamp.com/album/album-title".to_owned(),
                false,
            ),
            (
                Catalog::RateYourMusic,
                "album/artist-name/album-title",
                Some("album/artist-name/album-title".to_owned()),
                "https://rateyourmusic.com/release/album/artist-name/album-title".to_owned(),
                false,
            ),
            (
                Catalog::Wikidata,
                "Q424242",
                Some("Q424242".to_owned()),
                "https://www.wikidata.org/wiki/Q424242".to_owned(),
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
fn a_musicbrainz_release_without_a_parent_keeps_it_unknown() {
    let mut anchor = mb_release_with_relations("mb-release", "mb-group", &[]);
    anchor["release-group"] = serde_json::Value::Null;
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        anchor.to_string(),
        Vec::new(),
    );

    let records = payloads.extract().expect("the stored document reads").records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].album_ref(), None);
}

/// A master URL on the release names the group of a catalog that already
/// has a record, not a record of its own: a record names a pressing.
#[test]
fn a_group_link_does_not_invent_a_pressings_parent() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        mb_release_with_relations(
            "mb-release",
            "mb-group",
            &[
                "https://www.discogs.com/master/909090",
                "https://www.discogs.com/release/4242",
            ],
        )
        .to_string(),
        Vec::new(),
    );

    let records = payloads.extract().expect("the stored document reads").records();
    let discogs = records
        .iter()
        .find(|record| record.catalog() == Catalog::Discogs)
        .expect("the linked Discogs release is a record");
    assert_eq!(discogs.key(), "4242");
    assert_eq!(discogs.album_ref(), None);
}

/// A url-rel names a release page and nothing above it. The cross-linked
/// Discogs release was fetched along with the anchor, and only that document
/// says which master the release belongs to.
#[test]
fn the_archived_cross_reference_names_the_discogs_master() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        mb_release_with_relations(
            "mb-release",
            "mb-group",
            &["https://www.discogs.com/release/4242"],
        )
        .to_string(),
        vec![SourcePayload::new(
            PayloadSource::Discogs,
            "4242",
            serde_json::json!({
                "id": 4242, "title": "Album Title", "master_id": 909090
            })
            .to_string(),
        )],
    );

    let records = payloads.extract().expect("the stored documents read").records();
    let discogs = records
        .iter()
        .find(|record| record.catalog() == Catalog::Discogs)
        .expect("the linked Discogs release is a record");
    assert_eq!(discogs.key(), "4242");
    assert_eq!(
        discogs.album_ref().map(|album| album.key),
        Some("909090".to_owned())
    );
}

/// Wikidata's item for the album is the hub the catalogs MusicBrainz editors
/// leave unlinked are reached through: one archived item adds a record per
/// identifier it states, and fills in the Discogs master nothing else named.
#[test]
fn an_archived_wikidata_item_records_the_catalogs_it_identifies() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        mb_release_with_relations(
            "mb-release",
            "mb-group",
            &["https://www.discogs.com/release/4242"],
        )
        .to_string(),
        vec![
            SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "mb-group",
                wikidata_linked_release_group("mb-group", "Q424242"),
            ),
            SourcePayload::new(PayloadSource::Wikidata, "Q424242", wikidata_item("Q424242")),
        ],
    );

    let records = payloads.extract().expect("the stored documents read").records();
    let described: Vec<(Catalog, &str, Option<String>, String)> = records
        .iter()
        .map(|record| {
            (
                record.catalog(),
                record.key(),
                record.album_ref().map(|album| album.key),
                record.url(),
            )
        })
        .collect();
    assert_eq!(
        described,
        vec![
            (
                Catalog::MusicBrainz,
                "mb-release",
                Some("mb-group".to_owned()),
                "https://musicbrainz.org/release/mb-release".to_owned(),
            ),
            (
                Catalog::Discogs,
                "4242",
                None,
                "https://www.discogs.com/release/4242".to_owned(),
            ),
            (
                Catalog::AllMusic,
                "mw0000424242",
                Some("mw0000424242".to_owned()),
                "https://www.allmusic.com/album/mw0000424242".to_owned(),
            ),
            (
                Catalog::Spotify,
                "4242424242424242424242",
                Some("4242424242424242424242".to_owned()),
                "https://open.spotify.com/album/4242424242424242424242".to_owned(),
            ),
            (
                Catalog::Wikidata,
                "Q424242",
                Some("Q424242".to_owned()),
                "https://www.wikidata.org/wiki/Q424242".to_owned(),
            ),
        ]
    );
}

/// A document bae fetched itself outranks what the item says about the same
/// catalog: MusicBrainz's own link to a Spotify album is an editor looking at
/// this pressing, and the item describes the album.
#[test]
fn a_musicbrainz_link_outranks_the_item_on_the_same_catalog() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        mb_release_with_relations(
            "mb-release",
            "mb-group",
            &["https://open.spotify.com/album/9090909090909090909090"],
        )
        .to_string(),
        vec![
            SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "mb-group",
                wikidata_linked_release_group("mb-group", "Q424242"),
            ),
            SourcePayload::new(PayloadSource::Wikidata, "Q424242", wikidata_item("Q424242")),
        ],
    );

    let records = payloads.extract().expect("the stored documents read").records();
    let spotify = records
        .iter()
        .find(|record| record.catalog() == Catalog::Spotify)
        .expect("the linked Spotify album is a record");
    assert_eq!(spotify.key(), "9090909090909090909090");
}

/// A pick claims one release per catalog. What the primary's document says
/// about another catalog stands, unless the person claimed that catalog
/// themselves — then their release outranks the cross-link. Only the
/// primary's own record reads the draft.
#[test]
fn a_partner_outranks_what_the_primary_says_about_its_catalog() {
    let primary = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
        mb_release_with_relations(
            "mb-release",
            "mb-group",
            &["https://www.discogs.com/release/1111"],
        )
        .to_string(),
        Vec::new(),
    );
    let partner = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "2222"),
        serde_json::json!({
            "id": 2222, "title": "Album Title", "master_id": 909090
        })
        .to_string(),
        Vec::new(),
    );

    let primary = primary.extract().expect("the primary extracts");
    let partner = partner.extract().expect("the partner extracts");
    let records = crate::import::source_release::claimed_records(&[&primary, &partner]);

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].catalog(), Catalog::MusicBrainz);
    assert!(records[0].reads_draft());
    assert_eq!(records[1].catalog(), Catalog::Discogs);
    assert_eq!(
        records[1].key(),
        "2222",
        "the picked Discogs release outranks the cross-linked one"
    );
    assert_eq!(
        records[1].album_ref().map(|album| album.key),
        Some("909090".to_owned())
    );
    assert!(!records[1].reads_draft());
}

async fn test_database() -> (Database, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().expect("a temp library dir");
    let path = dir.path().join("test.db");
    let database = Database::new_test(
        path.to_str().expect("a UTF-8 temp path"),
        Arc::new(FixedClock(now())),
    )
    .await
    .expect("the test database opens");
    (database, dir)
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
/// album first came out — 1967 for a 1985 reissue. Extraction has to follow
/// that name out of the *anchor*, which is where a Discogs-seeded release's
/// own document lives; one that only looked at the supporting documents
/// would find no Discogs release there and silently fall back to the
/// pressing's own year.
#[test]
fn a_discogs_release_reaches_its_master_through_the_anchor() {
    let payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "12345"),
        discogs_release(12345, 99, 1985).to_string(),
        vec![SourcePayload::new(
            PayloadSource::DiscogsMaster,
            "99",
            serde_json::json!({ "id": 99, "year": 1967 }).to_string(),
        )],
    );

    let parsed = payloads
        .extract()
        .unwrap()
        .parsed(&[], &FixedClock(now()), &SequentialIdProvider::new("album"))
        .expect("the documents map");
    assert_eq!(
        parsed.album.year,
        Some(1967),
        "the album year is the master's, not the pressing's"
    );
    assert_eq!(parsed.release.pressing.year, Some(1985));
}

/// What Wikidata's item added when the release was fetched is stored with
/// it, so a commit, a re-identify or a reset reads the same records with
/// nothing asked of the network.
#[tokio::test]
async fn an_items_records_are_stored_with_the_release() {
    let (database, _dir) = test_database().await;
    let fetched = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "offline-mb-release"),
        mb_release_with_relations("offline-mb-release", "offline-mb-group", &[]).to_string(),
        vec![
            SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "offline-mb-group",
                wikidata_linked_release_group("offline-mb-group", "Q424242"),
            ),
            SourcePayload::new(PayloadSource::Wikidata, "Q424242", wikidata_item("Q424242")),
        ],
    )
    .extract()
    .expect("the documents extract");
    database.save_source_release(&fetched).await.unwrap();

    let stored = database
        .load_source_release(fetched.release())
        .await
        .expect("the stored release reads back")
        .expect("the release is stored");

    let catalogs: Vec<Catalog> = stored
        .records()
        .iter()
        .map(|record| record.catalog())
        .collect();
    assert_eq!(
        catalogs,
        vec![
            Catalog::MusicBrainz,
            Catalog::Discogs,
            Catalog::AllMusic,
            Catalog::Spotify,
            Catalog::Wikidata,
        ]
    );
}

/// Fetching a release follows the Wikidata item its MusicBrainz documents
/// name, which is what lets the records it adds be stored with the release.
#[tokio::test]
async fn a_fetch_follows_the_item_musicbrainz_names() {
    let providers = crate::providers::Providers::offline();
    let release_id = "archives-item-mb-release";
    let group_id = "archives-item-mb-group";
    providers.musicbrainz().seed_release_cache(
        release_id,
        mb_release_with_relations(release_id, group_id, &[]).to_string(),
    );
    providers.musicbrainz().seed_release_group_json_cache(
        group_id,
        wikidata_linked_release_group(group_id, "Q424242"),
    );
    providers.wikidata().seed_entity_cache("Q424242", Some(wikidata_item("Q424242")));

    let payloads = providers
        .fetch_payloads(
            None,
            &MetadataRef::new(Catalog::MusicBrainz, release_id),
            CallPriority::Interactive,
        )
    .await
    .expect("the release's documents fetch");

    let followed: Vec<(PayloadSource, String)> = payloads
        .supporting
        .iter()
        .map(|document| (document.source, document.source_release_id.clone()))
        .collect();
    assert_eq!(
        followed,
        vec![
            (PayloadSource::MusicBrainzReleaseGroup, group_id.to_string()),
            (PayloadSource::Wikidata, "Q424242".to_string()),
        ]
    );
    assert!(payloads
        .extract().expect("the fetched documents read")
        .records()
        .iter()
        .any(|record| record.catalog() == Catalog::Spotify));
}

/// Wikidata not answering is not an identification failure: the release keeps
/// every record its own catalogs' documents state, including the item the
/// url-rel named, and says the item is missing so the next preparation asks
/// for it again.
#[tokio::test]
async fn an_item_that_will_not_fetch_leaves_the_other_records_standing() {
    let providers = crate::providers::Providers::offline();
    let release_id = "missing-item-mb-release";
    let group_id = "missing-item-mb-group";
    providers.musicbrainz().seed_release_cache(
        release_id,
        mb_release_with_relations(release_id, group_id, &[]).to_string(),
    );
    providers.musicbrainz().seed_release_group_json_cache(
        group_id,
        wikidata_linked_release_group(group_id, "Q909090"),
    );
    providers.wikidata().seed_entity_cache("Q909090", None);

    let payloads = providers
        .fetch_payloads(
            None,
            &MetadataRef::new(Catalog::MusicBrainz, release_id),
            CallPriority::Interactive,
        )
    .await
    .expect("a release whose item will not fetch still identifies");

    assert!(
        !payloads
            .supporting
            .iter()
            .any(|document| document.source == PayloadSource::Wikidata),
        "no item stands in for the one Wikidata did not return"
    );
    assert_eq!(
        payloads.unfetched,
        vec![UnfetchedDocument {
            document: PayloadSource::Wikidata,
            key: "Q909090".to_string(),
            reason: UnfetchedReason::Failed,
        }]
    );
    let catalogs: Vec<Catalog> = payloads
        .extract().expect("the fetched documents read")
        .records()
        .iter()
        .map(|record| record.catalog())
        .collect();
    assert_eq!(catalogs, vec![Catalog::MusicBrainz, Catalog::Wikidata]);
}

#[test]
fn discogs_master_cross_reference_retains_album_links_without_claiming_a_pressing() {
    let mut payloads = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::Discogs, "7711"),
        serde_json::json!({
            "id": 7711, "master_id": 7722, "title": "Album Title",
            "artists": [{"id": 7733, "name": "Artist Name"}],
            "formats": [{"name": "Vinyl"}],
            "tracklist": [
                {"position": "A1", "title": "First Track", "type_": "track"},
                {"position": "B1", "title": "Second Track", "type_": "track"}
            ]
        })
        .to_string(),
        vec![
            SourcePayload::new(
                PayloadSource::DiscogsMaster,
                "7722",
                serde_json::json!({
                    "id": 7722, "year": 1966
                })
                .to_string(),
            ),
            SourcePayload::new(
                PayloadSource::MusicBrainzReleaseGroup,
                "linked-group",
                serde_json::json!({
                    "id": "linked-group", "first-release-date": "1965",
                    "relations": [
                        {"url": {"resource": "https://www.discogs.com/master/7722"}},
                        {"url": {"resource": "https://www.allmusic.com/album/mw0000007711"}},
                        {"url": {"resource": "https://www.wikidata.org/wiki/Q7711"}}
                    ]
                })
                .to_string(),
            ),
        ],
    );
    payloads.supporting.push(SourcePayload::new(
        PayloadSource::MusicBrainzDiscogsMasterXref,
        "7722",
        payloads.supporting[1].json.clone(),
    ));
    let records = payloads.extract().expect("linked album documents project").records();
    assert!(records
        .iter()
        .any(|record| record.url() == "https://musicbrainz.org/release-group/linked-group"));
    assert!(records
        .iter()
        .any(|record| record.url() == "https://www.allmusic.com/album/mw0000007711"));
    assert!(!records
        .iter()
        .any(|record| record.url().starts_with("https://musicbrainz.org/release/")));
    let parsed = payloads
        .extract()
        .unwrap()
        .parsed(
            &[],
            &FixedClock(now()),
            &SequentialIdProvider::new("linked-album"),
        )
        .unwrap();
    assert_eq!(parsed.album.year, Some(1966));
    assert_eq!(parsed.release.pressing.year, None);
    assert_eq!(
        parsed
            .tracks
            .iter()
            .map(|track| (track.title.as_str(), track.side))
            .collect::<Vec<_>>(),
        vec![("First Track", Some(1)), ("Second Track", Some(2))]
    );
}
