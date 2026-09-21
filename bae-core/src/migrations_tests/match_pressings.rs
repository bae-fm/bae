use super::*;
use crate::identify::TerminalVerdict;

fn version_forty_five() -> Vec<coven::Migration> {
    let mut migrations = all();
    migrations.truncate(45);
    migrations
}

/// A verdict as the previous schema stored it: three releases it settled on —
/// two of them one pressing, by the barcode both print — and two it narrowed
/// out, which are one pressing as well.
async fn seed_verdict(handle: &coven::CovenHandle) {
    handle
        .write(|sql| {
            sql.execute_batch(
                "INSERT INTO import_candidate_state (content_hash, folder_path)
                     VALUES ('hash-1', '/music/album');
                 INSERT INTO import_candidate_verdict
                     (content_hash, kind, track_count, probed_total_duration_ms, identified_at)
                     VALUES ('hash-1', 'found', 9, 2400000, '2026-01-01T00:00:00Z');
                 INSERT INTO import_candidate_match (
                     content_hash, position, source, release_id, title, year, media_kind,
                     by_disc_id, by_barcode, by_catalog, narrowed_out
                 ) VALUES
                     ('hash-1', 0, 'musicbrainz', 'mb-1', 'Album Title', 1992, 'undescribed',
                      1, 0, 0, 0),
                     ('hash-1', 1, 'discogs', 'dg-1', 'Album Title', 1992, 'undescribed',
                      0, 1, 0, 0),
                     ('hash-1', 2, 'discogs', 'dg-2', 'Album Title', 2013, 'undescribed',
                      0, 1, 0, 0),
                     ('hash-1', 3, 'musicbrainz', 'mb-9', 'Other Album', 1999, 'undescribed',
                      0, 1, 0, 1),
                     ('hash-1', 4, 'discogs', 'dg-9', 'Other Album', 1999, 'undescribed',
                      0, 1, 0, 1);
                 INSERT INTO import_candidate_match_barcode
                     (content_hash, position, ordinal, barcode)
                 VALUES
                     ('hash-1', 0, 0, '4988014720311'),
                     ('hash-1', 1, 0, '4988014720311'),
                     ('hash-1', 2, 0, '5051961234567'),
                     ('hash-1', 3, 0, '0012345678905'),
                     ('hash-1', 4, 0, '0012345678905');",
            )?;
            Ok(())
        })
        .await
        .unwrap();
}

/// Every stored match is given the pressing row it belongs to, each list
/// numbering its own rows: the two records of one object share a row, the
/// third stands alone, and the narrowed-out pair is one row of its own.
#[tokio::test]
#[serial]
async fn stored_matches_are_given_the_row_they_belong_to() {
    let temp = tempfile::tempdir().unwrap();
    let store = StoreDir::new_ephemeral(temp.path());
    let handle = open(store.clone(), "match-pressings", version_forty_five()).unwrap();
    seed_verdict(&handle).await;
    drop(handle);

    let db = crate::db::Database::open(
        store.clone(),
        config("match-pressings"),
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
    let TerminalVerdict::Found {
        matches,
        pressings,
        narrowed_out,
        narrowed_out_pressings,
        ..
    } = &states["hash-1"]
        .identify
        .as_ref()
        .expect("the verdict reads back")
        .verdict
    else {
        panic!("the verdict found releases");
    };
    assert_eq!(
        matches
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-1", "dg-1", "dg-2"]
    );
    assert_eq!(pressings, &vec![0, 0, 1]);
    assert_eq!(
        narrowed_out
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-9", "dg-9"]
    );
    assert_eq!(
        narrowed_out_pressings,
        &vec![0, 0],
        "each list numbers its own rows"
    );
    assert_eq!(
        crate::import::release_group::row_count(pressings),
        2,
        "the offered list holds two pressings"
    );
}
