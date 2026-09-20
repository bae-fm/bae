use super::*;
use crate::import::search::{MetadataResult, SourceTracks, StatedMedia};
use crate::import::Catalog;

fn version_forty_three() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(43);
    migrations
}

/// The ledger a run recorded at the previous schema: one MusicBrainz release
/// found by a disc ID, stored with the one barcode and the one format the
/// result then carried.
fn stored_ledger() -> String {
    r#"{
        "providers": ["MusicBrainz"],
        "disc_id": {
            "Read": {
                "disc_id": "disc-1",
                "source": null,
                "lookup": {
                    "Found": {
                        "count": 1,
                        "groups": [{
                            "id": "group-1",
                            "title": "Album Title",
                            "artist": "Artist Name",
                            "label": null,
                            "cover_art": null,
                            "sources": [{ "source": "MusicBrainz", "group_url": "https://musicbrainz.org/release-group/group-1" }],
                            "year_min": 1999,
                            "year_max": 1999,
                            "pressings": [{
                                "releases": [{
                                    "source": "MusicBrainz",
                                    "release_id": "mb-1",
                                    "title": "Album Title",
                                    "artist": "Artist Name",
                                    "year": 1999,
                                    "format": "CD",
                                    "label": null,
                                    "catalog_number": "CAT-1",
                                    "country": "US",
                                    "barcode": "012345678905",
                                    "cover_art": null,
                                    "source_group_id": "group-1",
                                    "source_tracks": { "Listed": { "count": 9, "total_duration_ms": 2400000 } }
                                }]
                            }]
                        }]
                    }
                }
            }
        },
        "barcode": "Absent",
        "catalog": "NoneFound"
    }"#
    .to_string()
}

async fn seed_verdict(handle: &coven::CovenHandle) {
    handle
        .write(|sql| {
            sql.execute(
                "INSERT INTO import_candidate_state (content_hash, folder_path) VALUES ('hash-1', '/music/album')",
                [],
            )?;
            sql.execute(
                "INSERT INTO import_candidate_verdict \
                     (content_hash, kind, track_count, ledger_json, probed_total_duration_ms, identified_at) \
                 VALUES ('hash-1', 'found', 9, ?, 2400000, '2026-01-01T00:00:00Z')",
                [stored_ledger()],
            )?;
            sql.execute_batch(
                "INSERT INTO import_candidate_match (
                     content_hash, position, source, release_id, title, year, format,
                     catalog_number, country, barcode, source_group_id,
                     source_tracks_kind, source_tracks_count, source_tracks_total_ms,
                     by_disc_id, by_barcode, by_catalog, narrowed_out
                 ) VALUES
                     ('hash-1', 0, 'musicbrainz', 'mb-1', 'Album Title', 1999, 'CD',
                      'CAT-1', 'US', '012345678905', 'group-1', 'listed', 9, 2400000, 1, 0, 0, 0),
                     ('hash-1', 1, 'discogs', 'dg-1', 'Album Title', 1999, 'CD, Album, Reissue',
                      'CAT-1', 'US', NULL, '7', NULL, NULL, NULL, 0, 1, 0, 0),
                     ('hash-1', 2, 'discogs', 'dg-2', 'Album Title', NULL, NULL,
                      NULL, NULL, '5051961234567', NULL, NULL, NULL, NULL, 0, 1, 0, 1);",
            )?;
            Ok(())
        })
    .await
    .unwrap();
}

/// Existing rows come across losslessly: the one barcode becomes the first
/// entry of the list, a format describes the record's media as the
/// descriptors it was written from, and no document has been read for links.
/// The stored ledger's results take the same shape, and both read back
/// through the production readers as the results they were.
#[tokio::test]
#[serial]
async fn match_rows_and_ledgers_carry_their_evidence_across() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let handle = open(store.clone(), "match-evidence", version_forty_three()).unwrap();
    seed_verdict(&handle).await;
    drop(handle);

    let db = crate::db::Database::open(
        store.clone(),
        config("match-evidence"),
        Arc::new(FixedClock(
            Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
                .single()
                .expect("valid migration test instant"),
        )),
        Arc::new(coven::UuidProvider),
        fixture_synced_tables(),
        None,
    )
    .unwrap();
    let states = db.load_import_candidate_states().await.unwrap();
    let crate::identify::TerminalVerdict::Found {
        matches,
        narrowed_out,
        ..
    } = &states["hash-1"]
        .identify
        .as_ref()
        .expect("the verdict reads back")
        .verdict
    else {
        panic!("the verdict found releases");
    };
    let results: Vec<&MetadataResult> = matches.iter().chain(narrowed_out.iter()).collect();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].barcodes, vec!["012345678905".to_string()]);
    assert_eq!(
        results[0].media,
        StatedMedia::Descriptors(vec!["CD".to_string()])
    );
    assert_eq!(results[1].barcodes, Vec::<String>::new());
    assert_eq!(
        results[1].media,
        StatedMedia::Descriptors(vec![
            "CD".to_string(),
            "Album".to_string(),
            "Reissue".to_string()
        ])
    );
    assert_eq!(results[2].media, StatedMedia::Undescribed);
    assert_eq!(results[2].barcodes, vec!["5051961234567".to_string()]);
    assert!(results.iter().all(|result| result.links.is_empty()));
    assert_eq!(results[0].source, Catalog::MusicBrainz);
    assert_eq!(
        results[0].source_tracks,
        Some(SourceTracks::Listed {
            count: 9,
            total_duration_ms: Some(2_400_000)
        })
    );

    let connection = coven::rusqlite::Connection::open(store.db_path()).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 44);
    let ledger_json: String = connection
        .query_row(
            "SELECT ledger_json FROM import_candidate_verdict WHERE content_hash = 'hash-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let ledger: crate::identify::IdentifyRunView =
        serde_json::from_str(&ledger_json).expect("the rewritten ledger reads back");
    let crate::identify::DiscIdStepView::Read { lookup, .. } = ledger.disc_id else {
        panic!("the ledger's disc ID step was read");
    };
    let crate::identify::LookupView::Found { groups, .. } = lookup else {
        panic!("the ledger's disc ID lookup found releases");
    };
    let release = &groups[0].pressings[0].releases[0];
    assert_eq!(release.barcodes, vec!["012345678905".to_string()]);
    assert_eq!(
        release.media,
        StatedMedia::Descriptors(vec!["CD".to_string()])
    );
    assert!(release.links.is_empty());
    assert_eq!(release.format.as_deref(), Some("CD"));
}
