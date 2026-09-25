//! The source-release tables hold every fact an extraction reads, and hold it
//! for exactly one extraction of a release at a time.

use super::empty_db;
use crate::import::payloads::ReleasePayloads;
use crate::import::{Catalog, MetadataRef, PayloadSource, SourcePayload};

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
        "date": "1999-04-01",
        "country": "GB",
        "barcode": "0123456789012",
        "artist-credit": [
            { "name": "Artist Name", "artist": { "id": "mb-artist", "name": "Artist Name", "sort-name": "Name, Artist" } }
        ],
        "release-group": { "id": "mb-group", "first-release-date": "1998" },
        "label-info": [{ "label": { "name": "Label Name" }, "catalog-number": "CAT-1" }],
        "media": [
            {
                "format": "12\" Vinyl",
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
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
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
        "formats": [{ "name": "CD" }, { "name": "Album" }],
        "labels": [{ "name": "Label Name", "catno": "CAT-1" }],
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
        MetadataRef::new(Catalog::Discogs, "4242"),
        anchor.to_string(),
        Vec::new(),
    )
}

/// What a release extracts to is what reading it back returns: nothing an
/// import surface reads is lost between the extraction and the tables.
#[tokio::test]
async fn a_stored_release_reads_back_as_it_was_extracted() {
    let (db, _tmp) = empty_db().await;
    let mut musicbrainz = musicbrainz_documents().extract().unwrap();
    musicbrainz
        .unfetched
        .push(crate::import::source_release::UnfetchedDocument {
            document: PayloadSource::Discogs,
            key: "4242".to_string(),
            reason: crate::import::source_release::UnfetchedReason::DiscogsNotConfigured,
        });
    let performed = &musicbrainz.mediums[0].entries[0].works[0].work;
    let crate::import::source_release::SourceWorkEvent::Part { work: parent, .. } =
        &performed.events[1]
    else {
        panic!("the movement names its parent work: {performed:?}");
    };
    assert_eq!(parent.events.len(), 1, "the parent names its other part");
    assert_eq!(musicbrainz.mediums[0].entries[0].credits.len(), 2);
    let discogs = discogs_documents().extract().unwrap();
    assert_eq!(discogs.mediums.len(), 2, "the positions number two discs");
    assert_eq!(discogs.mediums[1].entries[0].children.len(), 2);
    for extracted in [musicbrainz, discogs] {
        db.save_source_release(&extracted).await.unwrap();
        let stored = db
            .load_source_release(extracted.release())
            .await
            .unwrap()
            .expect("the saved release reads back");
        assert_eq!(stored, extracted);
    }
}

/// Fetching a release again replaces everything stored under it: no row of
/// the earlier extraction outlives it.
#[tokio::test]
async fn saving_a_release_again_replaces_its_rows() {
    let (db, _tmp) = empty_db().await;
    let earlier = musicbrainz_documents().extract().unwrap();
    db.save_source_release(&earlier).await.unwrap();

    let later = ReleasePayloads::for_test(
        MetadataRef::new(Catalog::MusicBrainz, "mb-release"),
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
    db.save_source_release(&later).await.unwrap();

    assert_eq!(
        db.load_source_release(later.release()).await.unwrap(),
        Some(later)
    );
}

/// A release nothing fetched reads back as nothing.
#[tokio::test]
async fn an_unfetched_release_reads_as_nothing() {
    let (db, _tmp) = empty_db().await;
    assert_eq!(
        db.load_source_release(&MetadataRef::new(Catalog::Discogs, "4242"))
            .await
            .unwrap(),
        None
    );
}
