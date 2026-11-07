//! The source-release tables hold every fact an extraction reads, and hold it
//! for exactly one extraction of a release at a time; a catalog release is
//! stored only while a row names it: stored in the write that writes the first
//! such row, and deleted in the write that drops the last.

use crate::tables::import_list::tests::*;
use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use crate::IdentifyRunId;
use crate::MetadataProvenance;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::payloads::ReleasePayloads;
use bae_catalogs::records::LookupRef;
use bae_catalogs::records::{MetadataRef, PayloadSource, SourcePayload};
use bae_catalogs::search::MetadataResult;
use bae_catalogs::source_release::{SourceRelease, UnfetchedDocument, UnfetchedReason};
use bae_catalogs::test_documents::{
    linked_discogs_master, linked_discogs_release, linked_musicbrainz_release,
    linked_release_group, musicbrainz_release_with_relations, wikidata_item,
    wikidata_linked_release_group,
};
use bae_identify::{RunOutcome, TerminalVerdict};
use bae_store::test_fixtures::empty_db;
use bae_store::Store;

/// A MusicBrainz release with everything its tracklist can state: a vinyl
/// medium and a CD medium, a credit that names no artist, composer
/// relations, and a performed work with parts in both directions.
fn musicbrainz_documents() -> ReleasePayloads {
    let composer = serde_json::json!({
        "target-type": "artist",
        "type": "composer",
        "artist": { "id": "mb-artist-composer", "name": "Artist Composer", "sort-name": "Composer, Artist" }
    });
    let work = serde_json::json!({
        "id": "mb-work-movement",
        "title": "Work Movement",
        "type": "Song",
        "relations": [
            composer,
            {
                "target-type": "work",
                "type": "parts",
                "direction": "backward",
                "work": {
                    "id": "mb-work-parent",
                    "title": "Work Parent",
                    "disambiguation": "Work Disambiguation",
                    "relations": [
                        {
                            "target-type": "work",
                            "type": "parts",
                            "direction": "forward",
                            "work": { "id": "mb-work-sibling", "title": "Work Sibling" }
                        }
                    ]
                }
            }
        ]
    });
    let anchor = serde_json::json!({
        "id": "mb-release",
        "title": "Album Title",
        "disambiguation": "made in Nordland",
        "annotation": "Matrix: NORD-1\n\n  Pressed by Plant Name  ",
        "date": "1999-04-01",
        "country": "GB",
        "barcode": "0123456789012",
        "artist-credit": [
            { "name": "Artist Name", "artist": { "id": "mb-artist", "name": "Artist Name", "sort-name": "Name, Artist" } }
        ],
        "release-group": { "id": "mb-group", "first-release-date": "1998" },
        "label-info": [
            { "label": { "name": "Label Name" }, "catalog-number": "CAT-1" },
            { "label": { "name": "Second Label" } },
            { "catalog-number": "CAT-2" }
        ],
        "media": [
            {
                "format": "12\" Vinyl",
                "discs": [{ "id": "disc-id-one" }, { "id": "disc-id-two" }, { "id": "disc-id-one" }],
                "tracks": [
                    {
                        "position": 1,
                        "number": "A1",
                        "length": 180000,
                        "recording": {
                            "title": "Track One",
                            "relations": [
                                composer,
                                { "target-type": "work", "type": "performance", "work": work }
                            ]
                        },
                        "artist-credit": [
                            { "name": "Artist Name", "artist": { "id": "mb-artist", "name": "Artist Name", "sort-name": "Name, Artist" } },
                            { "name": "Guest Artist" }
                        ]
                    },
                    { "position": 2, "number": "B1", "recording": { "title": "Track Two" } }
                ]
            },
            {
                "format": "CD",
                "tracks": [
                    { "position": 1, "title": "Track Three", "length": 200000 },
                    { "position": 2, "number": "2" }
                ]
            }
        ],
        "relations": [
            { "url": { "resource": "https://www.discogs.com/release/4242" } }
        ],
        "cover-art-archive": { "front": true, "darkened": false }
    });
    ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
        anchor.to_string(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzReleaseGroup,
            "mb-group",
            serde_json::json!({
                "id": "mb-group",
                "title": "Album Title",
                "first-release-date": "1998",
                "relations": [
                    { "url": { "resource": "https://www.discogs.com/master/909" } }
                ]
            })
            .to_string(),
        )],
    )
}

/// A Discogs release with every row shape: a heading over sub-track rows, an
/// index holding sub-tracks, a second disc, composer credits on the release
/// and on a row, and images.
fn discogs_documents() -> ReleasePayloads {
    let anchor = serde_json::json!({
        "id": 4242,
        "title": "Album Title",
        "year": 1999,
        "master_id": 909,
        "formats": [{ "name": "CD", "text": "Small label" }, { "name": "Album" }],
        "notes": "Made in Nordland.\r\n\r\nSleeve printed by Printer Name.",
        "labels": [{ "name": "Label Name", "catno": "CAT-1" }],
        "companies": [
            { "name": "Plant Name", "entity_type_name": "Pressed By" },
            { "name": "Maker Name", "entity_type_name": "Made By" }
        ],
        "identifiers": [
            { "type": "Barcode", "value": "0123456789012" },
            { "type": "Matrix / Runout", "value": "MADE IN NORDLAND BY PLANT", "description": "Variant 1" },
            { "type": "Rights Society", "value": "BIEM/NORD" },
            { "type": "Barcode", "value": "0 12345 67890 1", "description": "Text" },
            { "type": "Label Code", "value": "LC 0000" }
        ],
        "images": [
            { "type": "primary", "uri": "https://images.example/front.jpg", "uri150": "https://images.example/front-150.jpg" }
        ],
        "artists": [{ "id": 11, "name": "Artist Name" }],
        "extraartists": [
            { "id": 12, "name": "Artist Composer", "role": "Written-By", "anv": "A. Composer" },
            { "id": 13, "name": "Artist Producer", "role": "Producer" }
        ],
        "tracklist": [
            { "position": "", "type_": "heading", "title": "Heading Title" },
            { "position": "1-1a", "type_": "track", "title": "Part One", "duration": "1:00" },
            { "position": "1-1b", "type_": "track", "title": "Part Two", "duration": "2:00",
              "extraartists": [{ "id": 12, "name": "Artist Composer", "role": "Composed By" }] },
            {
                "position": "", "type_": "index", "title": "Index Title", "duration": "5:00",
                "sub_tracks": [
                    { "position": "2-1a", "type_": "track", "title": "Part Three", "duration": "2:00",
                      "artists": [{ "id": 14, "name": "Guest Artist" }] },
                    { "position": "2-1b", "type_": "track", "title": "Part Four", "duration": "3:00" }
                ]
            },
            { "position": "2-2", "type_": "track", "title": "Track Title", "duration": "4:00" }
        ]
    });
    ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "4242"),
        anchor.to_string(),
        Vec::new(),
    )
}

/// What a release extracts to is what reading it back returns: nothing an
/// import surface reads is lost between the extraction and the tables.
#[tokio::test]
async fn a_stored_release_reads_back_as_it_was_extracted() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let mut musicbrainz = musicbrainz_documents().extract().unwrap();
    musicbrainz.unfetched.push(bae_catalogs::source_release::UnfetchedDocument {
        document: PayloadSource::Discogs,
        key: "4242".to_string(),
        reason: bae_catalogs::source_release::UnfetchedReason::NotAsked,
    });
    let performed = &musicbrainz.mediums[0].entries[0].works[0].work;
    let bae_catalogs::source_release::SourceWorkEvent::Part { work: parent, .. } =
        &performed.events[1]
    else {
        panic!("the movement names its parent work: {performed:?}");
    };
    assert_eq!(parent.events.len(), 1, "the parent names its other part");
    assert_eq!(musicbrainz.mediums[0].entries[0].credits.len(), 2);
    assert_eq!(
        musicbrainz.mediums[0].disc_ids,
        ["disc-id-one", "disc-id-two"],
        "every disc ID registered against the medium, each once"
    );
    assert_eq!(
        musicbrainz.notes,
        vec![
            "made in Nordland".to_string(),
            "Matrix: NORD-1".to_string(),
            "Pressed by Plant Name".to_string(),
        ],
        "the disambiguation, then each line of the annotation"
    );
    let discogs = discogs_documents().extract().unwrap();
    assert_eq!(
        discogs.notes,
        vec![
            "Small label".to_string(),
            "Made in Nordland.".to_string(),
            "Sleeve printed by Printer Name.".to_string(),
            "Plant Name".to_string(),
            "Maker Name".to_string(),
            "MADE IN NORDLAND BY PLANT".to_string(),
            "BIEM/NORD".to_string(),
            "LC 0000".to_string(),
        ],
        "the format text, each line of the notes, the companies, then every identifier but a barcode"
    );
    assert_eq!(discogs.mediums.len(), 2, "the positions number two discs");
    assert_eq!(discogs.mediums[1].entries[0].children.len(), 2);
    for extracted in [&musicbrainz, &discogs] {
        assert!(
            extracted.covers.release.iter().all(|cover| !cover.image.downscaled.is_empty()),
            "every offered image carries the copies its catalog serves"
        );
    }
    for extracted in [musicbrainz, discogs] {
        tables.save_source_release(&extracted).await.unwrap();
        let stored = tables
            .load_source_release(extracted.release())
            .await
            .unwrap()
            .expect("the saved release reads back");
        assert_eq!(stored, extracted);
    }
}

/// A MusicBrainz release's album links read back as its documents stated
/// them: each album named, in order, or that one could not be read.
#[tokio::test]
async fn a_stored_release_reads_back_its_album_links() {
    use bae_catalogs::album_links::AlbumLinks;
    use bae_catalogs::source_release::CatalogFacts;
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let extracted = musicbrainz_documents().extract().unwrap();
    let CatalogFacts::MusicBrainz { album_links, .. } = &extracted.catalog else {
        panic!("a MusicBrainz release states MusicBrainz facts");
    };
    assert_eq!(
        *album_links,
        AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, "909")]),
        "the group's page links the master"
    );
    for stated in [
        AlbumLinks::Read(vec![
            MetadataRef::new(Catalog::Discogs, "910"),
            MetadataRef::new(Catalog::Discogs, "909"),
        ]),
        AlbumLinks::Read(Vec::new()),
        AlbumLinks::Unread,
    ] {
        let mut release = extracted.clone();
        let CatalogFacts::MusicBrainz { album_links, .. } = &mut release.catalog else {
            unreachable!("checked above");
        };
        *album_links = stated;
        tables.save_source_release(&release).await.unwrap();
        assert_eq!(tables.load_source_release(release.release()).await.unwrap(), Some(release));
    }
}

/// Fetching a release again replaces everything stored under it: no row of
/// the earlier extraction outlives it.
#[tokio::test]
async fn saving_a_release_again_replaces_its_rows() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let earlier = musicbrainz_documents().extract().unwrap();
    tables.save_source_release(&earlier).await.unwrap();

    let later = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
        serde_json::json!({
            "id": "mb-release",
            "title": "Album Title Later",
            "artist-credit": [{ "name": "Artist Name" }],
            "media": [{ "format": "CD", "tracks": [{ "number": "1", "title": "Track One" }] }],
            "cover-art-archive": { "front": false, "darkened": false }
        })
        .to_string(),
        Vec::new(),
    )
    .extract()
    .unwrap();
    tables.save_source_release(&later).await.unwrap();

    assert_eq!(tables.load_source_release(later.release()).await.unwrap(), Some(later));
}

/// A release's album year and media counts read back from what the store
/// keeps of it: its album's first year, else its pressing's year, and the
/// carriers its mediums (MusicBrainz) or format entries (Discogs) name.
#[tokio::test]
async fn a_stored_release_reads_back_its_album_year_and_media_counts() {
    use bae_types::pressing::{MediaCount, Medium};
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let musicbrainz = |media: serde_json::Value, group: Option<&str>| {
        let mut anchor = serde_json::json!({
            "id": "mb-release",
            "title": "Album Title",
            "date": "2003",
            "artist-credit": [{ "name": "Artist Name" }],
            "media": media,
            "cover-art-archive": { "front": false, "darkened": false }
        });
        if let Some(year) = group {
            anchor["release-group"] =
                serde_json::json!({ "id": "mb-group", "first-release-date": year });
        }
        ReleasePayloads::for_test(
            LookupRef::new(LookupCatalog::MusicBrainz, "mb-release"),
            anchor.to_string(),
            Vec::new(),
        )
    };
    let discogs = |formats: serde_json::Value, master: Option<i64>| {
        let anchor = serde_json::json!({
            "id": 4242,
            "title": "Album Title",
            "year": 2003,
            "master_id": master.map(|_| 909),
            "formats": formats,
            "artists": [{ "id": 11, "name": "Artist Name" }],
            "tracklist": [{ "position": "1", "type_": "track", "title": "Track Title" }]
        });
        let documents = master
            .map(|year| {
                vec![SourcePayload::new(
                    PayloadSource::DiscogsMaster,
                    "909",
                    serde_json::json!({ "id": 909, "title": "Album Title", "year": year })
                        .to_string(),
                )]
            })
            .unwrap_or_default();
        ReleasePayloads::for_test(
            LookupRef::new(LookupCatalog::Discogs, "4242"),
            anchor.to_string(),
            documents,
        )
    };
    let count = |medium, count| MediaCount { medium, count };
    for (payloads, album_year, media) in [
        (
            musicbrainz(
                serde_json::json!([
                    { "format": "CD", "tracks": [{ "number": "1", "title": "Track One" }] },
                    { "format": "CD", "tracks": [{ "number": "1", "title": "Track Two" }] }
                ]),
                Some("1998"),
            ),
            Some(1998),
            vec![count(Medium::Cd, 2)],
        ),
        (musicbrainz(serde_json::json!([]), None), Some(2003), Vec::new()),
        (
            discogs(
                serde_json::json!([{ "name": "Vinyl", "qty": "2" }, { "name": "CD", "qty": "1" }]),
                Some(1997),
            ),
            Some(1997),
            vec![count(Medium::Vinyl, 2), count(Medium::Cd, 1)],
        ),
        (discogs(serde_json::json!([]), None), Some(2003), Vec::new()),
    ] {
        let extracted = payloads.extract().unwrap();
        tables.save_source_release(&extracted).await.unwrap();
        let stored = tables
            .load_source_release(extracted.release())
            .await
            .unwrap()
            .expect("the saved release reads back");
        assert_eq!(stored.metadata.album.year, album_year);
        assert_eq!(stored.metadata.pressing.facts.media, media);
        assert_eq!(stored, extracted);
    }
}

/// A release whose album year or media counts are not what its other facts
/// give is one no extraction builds, since the store keeps only those other
/// facts: its write fails with the bug the store reports.
#[tokio::test]
async fn a_release_whose_derived_facts_disagree_is_a_bug() {
    let extracted = musicbrainz_documents().extract().unwrap();
    let mut year = extracted.clone();
    year.metadata.album.year = Some(1970);
    let mut media = extracted.clone();
    media.metadata.pressing.facts.media.clear();
    for (release, said) in [(year, "states album year"), (media, "counts media")] {
        let (db, _tmp, heard) =
            bae_store::test_fixtures::temp_db_hearing_bugs(crate::TEST_SCHEMA).await;
        let tables = crate::tables::ImportTables::new(db.clone());

        let failed = tables.save_source_release(&release).await.unwrap_err();

        let bug = crate::tables::tests::the_reported_bug(&heard, &failed);
        assert!(bug.contains(said), "{bug}");
    }
}

/// A release nothing fetched reads back as nothing.
#[tokio::test]
async fn an_unfetched_release_reads_as_nothing() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    assert_eq!(
        tables.load_source_release(&LookupRef::new(LookupCatalog::Discogs, "4242")).await.unwrap(),
        None
    );
}

/// A Discogs release whose documents never reach a MusicBrainz album reads
/// back naming the release group a reading found its master to be — the
/// statement a list read through another pressing's link — with the
/// archive's images of that group, and nothing a statement about another
/// master names. It reads the same whether it was stored before the statement
/// was kept or after, and a later reading of the group that names nothing
/// takes the album away again.
#[tokio::test]
async fn a_stored_release_carries_what_its_album_s_group_was_read_to_be() {
    use bae_catalogs::records::ReleaseRecord;
    use bae_catalogs::remote_image::RemoteCover;

    let master = |key: &str| MetadataRef::new(Catalog::Discogs, key);
    let statements = || {
        vec![
            ("mb-group-same".to_string(), vec![master("909")]),
            ("mb-group-other".to_string(), vec![master("910")]),
        ]
    };
    let discogs = discogs_documents().extract().unwrap();
    let mut expected = discogs.clone();
    expected
        .other_records
        .push(ReleaseRecord::album(&MetadataRef::new(Catalog::MusicBrainz, "mb-group-same")));
    expected.covers.album.push(RemoteCover::musicbrainz_release_group("mb-group-same"));
    expected.archive_groups.push("mb-group-same".to_string());

    // Stored first, then the statement read.
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    tables.save_source_release(&discogs).await.unwrap();
    tables.replace_group_album_links(statements()).await.unwrap();
    let before = tables.load_source_release(discogs.release()).await.unwrap();
    assert_eq!(before.as_ref(), Some(&expected));

    // The statement read first, then stored.
    let (db_after, _tmp_after) = empty_db(crate::TEST_SCHEMA).await;
    let tables_after = crate::tables::ImportTables::new(db_after);
    tables_after.replace_group_album_links(statements()).await.unwrap();
    tables_after.save_source_release(&discogs).await.unwrap();
    assert_eq!(tables_after.load_source_release(discogs.release()).await.unwrap(), before);

    tables
        .replace_group_album_links(vec![("mb-group-same".to_string(), Vec::new())])
        .await
        .unwrap();
    assert_eq!(tables.load_source_release(discogs.release()).await.unwrap(), Some(discogs));
}

/// A Discogs release stored with its master and the release group the
/// master names back reads back as it was extracted.
#[tokio::test]
async fn a_release_with_its_master_and_group_reads_back_as_extracted() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let fetched = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        vec![
            SourcePayload::new(PayloadSource::DiscogsMaster, "7822", linked_discogs_master()),
            SourcePayload::new(
                PayloadSource::MusicBrainzDiscogsMasterXref,
                "7822",
                linked_release_group(),
            ),
        ],
    )
    .extract()
    .unwrap();
    tables.save_source_release(&fetched).await.unwrap();
    assert_eq!(tables.load_source_release(fetched.release()).await.unwrap(), Some(fetched));
}

/// What Wikidata's item added when the release was fetched is stored with
/// it, so a commit, a re-identify or a reset reads the same records with
/// nothing asked of the network.
#[tokio::test]
async fn an_items_records_are_stored_with_the_release() {
    let (db, _dir) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let fetched = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::MusicBrainz, "offline-mb-release"),
        musicbrainz_release_with_relations("offline-mb-release", "offline-mb-group", &[])
            .to_string(),
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
    tables.save_source_release(&fetched).await.unwrap();

    let stored = tables
        .load_source_release(fetched.release())
        .await
        .expect("the stored release reads back")
        .expect("the release is stored");

    let catalogs: Vec<Catalog> = stored.records().iter().map(|record| record.catalog()).collect();
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

/// A cross-reference the catalog no longer answers is gone from the release
/// once it is fetched again: the new extraction replaces the old one whole.
#[tokio::test]
async fn refetching_without_a_reverse_answer_removes_its_pressing_record() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let with_alias = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        vec![SourcePayload::new(
            PayloadSource::MusicBrainzDiscogsXref,
            "7811",
            linked_musicbrainz_release(),
        )],
    );
    tables.save_source_release(&with_alias.extract().unwrap()).await.unwrap();
    assert!(tables
        .load_source_release(&LookupRef::new(LookupCatalog::Discogs, "7811"))
        .await
        .unwrap()
        .unwrap()
        .records()
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz && record.release_ref().is_some()));
    let without_alias = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        Vec::new(),
    );
    tables.save_source_release(&without_alias.extract().unwrap()).await.unwrap();
    assert!(!tables
        .load_source_release(&LookupRef::new(LookupCatalog::Discogs, "7811"))
        .await
        .unwrap()
        .unwrap()
        .records()
        .iter()
        .any(|record| record.catalog() == Catalog::MusicBrainz));
}

/// A parent a later fetch could not obtain does not come back from the
/// earlier fetch: the stored release is the later fetch's, whole.
#[tokio::test]
async fn an_unavailable_supporting_document_does_not_return_from_an_earlier_fetch() {
    let (db, _temp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let earlier = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        vec![SourcePayload::new(PayloadSource::DiscogsMaster, "7822", linked_discogs_master())],
    );
    tables.save_source_release(&earlier.extract().unwrap()).await.unwrap();
    let later = ReleasePayloads::for_test(
        LookupRef::new(LookupCatalog::Discogs, "7811"),
        linked_discogs_release(),
        Vec::new(),
    )
    .extract()
    .unwrap();
    tables.save_source_release(&later).await.unwrap();
    assert_eq!(tables.load_source_release(later.release()).await.unwrap(), Some(later));
}

/// `key` of `catalog` as a fetch hands it over, printed with `barcode`: one
/// row hangs off it once stored.
fn barcoded(catalog: LookupCatalog, key: &str, barcode: &str) -> SourceRelease {
    let mut release = SourceRelease::for_test(LookupRef::new(catalog, key));
    release.metadata.pressing.barcodes = vec![barcode.to_string()];
    release
}

/// The releases the writes below carry: MusicBrainz `rel-1` and `rel-2`, and
/// Discogs `dg-1`, which pairs with `rel-2` as one pressing.
fn carried() -> Vec<SourceRelease> {
    vec![
        barcoded(LookupCatalog::MusicBrainz, "rel-1", "0123456789012"),
        barcoded(LookupCatalog::MusicBrainz, "rel-2", "0123456789029"),
        barcoded(LookupCatalog::Discogs, "dg-1", "0123456789036"),
    ]
}

/// MusicBrainz release `key`.
fn musicbrainz(key: &str) -> LookupRef {
    LookupRef::new(LookupCatalog::MusicBrainz, key)
}

impl Library {
    /// A lookup of the release at `display_path` stored, its one match
    /// naming MusicBrainz release `release`.
    fn match_naming(&self, display_path: &str, release: &str) -> Write {
        let found = MetadataResult { release_id: release.to_string(), ..found_release(0, FITS) };
        self.concluded_carrying(display_path, found_verdict(11, [(found, 0)]), carried())
    }

    /// The release's draft applied from `provenance`.
    fn sourced(&self, display_path: &str, provenance: Option<MetadataProvenance>) -> Write {
        self.metadata_carrying(
            display_path,
            metadata_draft("Album Title", "Artist"),
            provenance,
            carried(),
        )
    }

    /// The release linked to MusicBrainz pressing `release`, with Discogs
    /// `partners`, or to nothing.
    fn pressing(&self, display_path: &str, release: Option<&str>, partners: &[&str]) -> Write {
        let link = release.map(|release| {
            crate::ReleaseLink::Pressing(bae_identify::PressingLink {
                record: musicbrainz(release),
                partners: partners
                    .iter()
                    .map(|partner| LookupRef::new(LookupCatalog::Discogs, *partner))
                    .collect(),
            })
        });
        self.link_carrying(display_path, link, carried())
    }
}

/// A draft read from MusicBrainz release `key`.
fn read_from(key: &str) -> Option<MetadataProvenance> {
    Some(MetadataProvenance::ExternalRelease { record: musicbrainz(key) })
}

/// Whether `release` is stored, and how many rows hang off it.
async fn stored(db: &Store, release: &LookupRef) -> (bool, i64) {
    let (catalog, key) = (release.catalog.as_str(), release.key.clone());
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT EXISTS (SELECT 1 FROM source_release \
                            WHERE catalog = ?1 AND release_id = ?2), \
                    (SELECT COUNT(*) FROM source_release_barcode \
                     WHERE catalog = ?1 AND release_id = ?2)",
            [catalog, key.as_str()],
            |row| Ok((row.get::<_, bool>(0)?, row.get::<_, i64>(1)?)),
        )?)
    })
    .await
    .unwrap()
}

/// A write a case makes of the release at `a`.
type CaseWrite = fn(&Library) -> Write;

/// Each kind of row that names a release keeps it, and the write that drops
/// that row, or changes it to name another release or none, deletes the
/// release and the rows under it in the same write. A candidate's whole state
/// going — as a removed folder takes it — takes every release only it named.
#[tokio::test]
async fn the_last_row_naming_a_catalog_release_takes_it_with_it() {
    let rel_1 = musicbrainz("rel-1");
    let dg_1 = LookupRef::new(LookupCatalog::Discogs, "dg-1");
    let cases: [(&str, CaseWrite, CaseWrite, &LookupRef); 9] = [
        (
            "a match whose verdict goes",
            |lib| lib.match_naming("a", "rel-1"),
            |lib| lib.file_decision("a"),
            &rel_1,
        ),
        (
            "a match that names another release",
            |lib| lib.match_naming("a", "rel-1"),
            |lib| lib.match_naming("a", "rel-2"),
            &rel_1,
        ),
        (
            "a draft's source that goes",
            |lib| lib.sourced("a", read_from("rel-1")),
            |lib| lib.sourced("a", None),
            &rel_1,
        ),
        (
            "a draft's source that turns to the files' tags",
            |lib| lib.sourced("a", read_from("rel-1")),
            |lib| lib.sourced("a", Some(MetadataProvenance::FileMetadata)),
            &rel_1,
        ),
        (
            "a link that goes",
            |lib| lib.pressing("a", Some("rel-1"), &[]),
            |lib| lib.pressing("a", None, &[]),
            &rel_1,
        ),
        (
            "a link that names another release",
            |lib| lib.pressing("a", Some("rel-1"), &[]),
            |lib| lib.pressing("a", Some("rel-2"), &[]),
            &rel_1,
        ),
        (
            "a partner that goes",
            |lib| lib.pressing("a", Some("rel-2"), &["dg-1"]),
            |lib| lib.pressing("a", Some("rel-2"), &[]),
            &dg_1,
        ),
        (
            "a partner whose link goes",
            |lib| lib.pressing("a", Some("rel-2"), &["dg-1"]),
            |lib| lib.pressing("a", None, &[]),
            &dg_1,
        ),
        (
            "every row of a candidate whose state goes",
            |lib| {
                lib.sourced("a", read_from("rel-1"))
                    + lib.pressing("a", Some("rel-1"), &[])
                    + lib.match_naming("a", "rel-1")
            },
            |lib| lib.unwatch(),
            &rel_1,
        ),
    ];
    for (case, naming, dropping, release) in cases {
        let lib = Library::new().await;
        lib.run(lib.scan("a") + naming(&lib)).await;
        assert_eq!(stored(&lib.db, release).await, (true, 1), "{case}: named, so kept");
        lib.run(dropping(&lib)).await;
        assert_eq!(
            stored(&lib.db, release).await,
            (false, 0),
            "{case}: no row names it, so it goes"
        );
    }
}

/// A release two candidates name stays while either names it, and goes with
/// the second.
#[tokio::test]
async fn a_release_two_candidates_name_stays_until_both_drop_it() {
    let lib = Library::new().await;
    let rel_1 = musicbrainz("rel-1");
    lib.run(
        lib.scan("a")
            + lib.scan("b")
            + lib.pressing("a", Some("rel-1"), &[])
            + lib.match_naming("b", "rel-1"),
    )
    .await;
    lib.run(lib.pressing("a", None, &[])).await;
    assert_eq!(stored(&lib.db, &rel_1).await, (true, 1), "b's match still names it");
    lib.run(lib.file_decision("b")).await;
    assert_eq!(stored(&lib.db, &rel_1).await, (false, 0), "nothing names it");
}

/// `release` as a fetch hands it over, unstored.
fn release(key: &str) -> SourceRelease {
    SourceRelease::for_test(LookupRef::new(LookupCatalog::MusicBrainz, key))
}

/// A verdict's write stores the releases its rows name from the ones it
/// carries, and none of the others; the verdict that replaces it, naming
/// none, takes them.
#[tokio::test]
async fn a_verdict_stores_only_the_releases_its_rows_name() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let candidate = track_files_candidate(&[("01 Track.flac", 123_456)]);
    let hash = candidate.content_hash();
    let mut row = new_candidate_row(&hash, &host_root("/music/Some Album"), &sample_verdict());
    store_candidate_state(&db, &candidate, &row.candidate_key.test_folder()).await;
    let preparations = crate::tables::ImportTables::new(db.clone());

    row.releases = vec![release("rel-1"), release("rel-unnamed")];
    assert!(preparations.store_verdict(&row).await.unwrap());
    let named = LookupRef::new(LookupCatalog::MusicBrainz, "rel-1");
    let unnamed = LookupRef::new(LookupCatalog::MusicBrainz, "rel-unnamed");
    assert_eq!(
        tables.load_source_release(&named).await.unwrap(),
        Some(release("rel-1")),
        "the verdict's match names it"
    );
    assert_eq!(tables.load_source_release(&unnamed).await.unwrap(), None);

    row.verdict = TerminalVerdict::NotFoundAnywhere { ledger: None };
    row.releases = Vec::new();
    assert!(preparations.store_verdict(&row).await.unwrap());
    assert_eq!(tables.load_source_release(&named).await.unwrap(), None);
}

/// Store `releases` as what `run` of the library release `key` read, its
/// answer the sample verdict, as a release run's publish stores them.
async fn store_release_run(
    db: &Store,
    key: &crate::CandidateKey,
    run: IdentifyRunId,
    releases: Vec<SourceRelease>,
) {
    let key = key.clone();
    db.call(move |sql| {
        crate::tables::import_state::store_release_run_on(
            sql,
            &key,
            run,
            &RunOutcome::Answered(sample_verdict()),
            &releases,
            None,
        )
    })
    .await
    .unwrap();
}

/// What a run in flight publishes keeps the releases it read that its rows
/// name, and none it read that they don't; the run's end takes them.
#[tokio::test]
async fn a_run_s_rows_keep_the_releases_it_read_until_the_run_ends() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let key = crate::CandidateKey::test("release-reidentify");
    let run = IdentifyRunId::for_test(1);
    store_release_run(&db, &key, run, vec![release("rel-1"), release("rel-unnamed")]).await;
    let named = LookupRef::new(LookupCatalog::MusicBrainz, "rel-1");
    assert!(tables.load_source_release(&named).await.unwrap().is_some());
    assert_eq!(
        tables
            .load_source_release(&LookupRef::new(LookupCatalog::MusicBrainz, "rel-unnamed"))
            .await
            .unwrap(),
        None,
        "no row of the run names it"
    );

    tables.publish_release_run(&key, run, &bae_identify::IdentifyState::Idle, None).await.unwrap();
    assert_eq!(tables.load_source_release(&named).await.unwrap(), None);
}

/// A write stores its copy of a release over the stored one only where its
/// copy read a document the stored one did not: a copy read earlier never
/// overwrites a fuller one stored since.
#[tokio::test]
async fn a_stored_release_is_replaced_only_by_a_copy_that_read_more() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let key = crate::CandidateKey::test("release-reidentify");
    let run = IdentifyRunId::for_test(1);
    let mut partial = release("rel-1");
    partial.unfetched = vec![UnfetchedDocument {
        document: bae_catalogs::records::PayloadSource::MusicBrainz,
        key: "rel-1".to_string(),
        reason: UnfetchedReason::StoppedShort,
    }];
    let complete = release("rel-1");
    let publish = |copy: SourceRelease| {
        let db = db.clone();
        let tables = crate::tables::ImportTables::new(db.clone());
        let key = key.clone();
        async move {
            store_release_run(&db, &key, run, vec![copy]).await;
            tables
                .load_source_release(&LookupRef::new(LookupCatalog::MusicBrainz, "rel-1"))
                .await
                .unwrap()
                .expect("the run's match names it")
        }
    };
    assert_eq!(publish(partial.clone()).await, partial);
    assert_eq!(publish(complete.clone()).await, complete, "it read more");
    assert_eq!(
        publish(partial).await,
        complete,
        "the copy read earlier stops short of the stored one"
    );
}
