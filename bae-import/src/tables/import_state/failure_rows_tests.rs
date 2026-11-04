//! An import's failure as stored on its candidate and read back.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::ImportFailure;
use bae_db::test_fixtures::empty_db;
use bae_db::test_fixtures::fixed_now;
use bae_db::watched_path::host_root;
use bae_db::DbArtist;

/// A failed import is recorded on the discovered candidate, and a later one
/// replaces it.
#[tokio::test]
async fn a_failure_on_a_discovered_candidate_is_replaced_then_cleared() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;

    tables
        .save_import_candidate_failure(
            &hash,
            0,
            &ImportFailure::error_only("the folder vanished", fixed_now()),
        )
        .await
        .unwrap();

    let failure = tables
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .failure
        .expect("the failure is stored");
    assert_eq!(failure.reason, crate::ImportFailureReason::error("the folder vanished"));
    assert_eq!(failure.failed_at, fixed_now());
    assert!(tables
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap()
        .draft
        .release_edit()
        .is_blank());

    tables
        .save_import_candidate_failure(
            &hash,
            0,
            &ImportFailure::error_only("the disc would not read", fixed_now()),
        )
        .await
        .unwrap();
    assert_eq!(
        tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.unwrap().reason,
        crate::ImportFailureReason::error("the disc would not read"),
        "the second failure replaces the first"
    );
}

#[tokio::test]
async fn an_artist_identity_conflict_round_trips_with_both_library_artists() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    let discogs = DbArtist {
        id: bae_util::test_ids::test_uuid("conflict-round-trip-discogs-artist"),
        name: "Artist One".to_string(),
        sort_name: None,
        discogs_artist_id: Some("discogs-1".to_string()),
        musicbrainz_artist_id: None,
        created_at: fixed_now(),
    };
    let musicbrainz = DbArtist {
        id: bae_util::test_ids::test_uuid("conflict-round-trip-musicbrainz-artist"),
        name: "Artist One".to_string(),
        sort_name: Some("Artist One, The".to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: Some("mb-1".to_string()),
        created_at: fixed_now(),
    };
    db.insert_artist(&discogs).await.unwrap();
    db.insert_artist(&musicbrainz).await.unwrap();
    let expected = artist_identity_failure(&discogs, &musicbrainz);

    tables.save_import_candidate_failure(&hash, 0, &expected).await.unwrap();

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.unwrap();
    assert_eq!(stored, expected);
}

/// A failure stopping on a label conflict reads back with both library
/// labels as they stand.
#[tokio::test]
async fn a_label_identity_conflict_round_trips_with_both_library_labels() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let (_, hash) = stored_pane_candidate(&db).await;
    db.call(|sql| {
        sql.execute_batch(
            "INSERT INTO labels (id, name, name_key, discogs_label_id, created_at) \
                 VALUES ('by-discogs', 'Label Name', 'labelname', '7190', \
                         '2026-01-01T00:00:00Z'); \
             INSERT INTO labels (id, name, name_key, musicbrainz_label_id, created_at) \
                 VALUES ('by-musicbrainz', 'Label Name Records', 'labelname', 'mb-label', \
                         '2026-01-01T00:00:00Z');",
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let existing = |id: &str, name: &str, musicbrainz: Option<&str>, discogs: Option<&str>| {
        bae_db::ExistingLabel {
            label_id: id.to_string(),
            name: name.to_string(),
            musicbrainz_label_id: musicbrainz.map(str::to_string),
            discogs_label_id: discogs.map(str::to_string),
        }
    };
    let expected = ImportFailure {
        reason: crate::ImportFailureReason::error("the label identities disagree"),
        failed_at: fixed_now(),
        identity_conflict: Some(crate::IdentityConflict::Label(bae_db::LabelIdentityConflict {
            incoming_label_name: "Label Name".to_string(),
            discogs_label_id: "7190".to_string(),
            musicbrainz_label_id: "mb-label".to_string(),
            discogs_label: existing("by-discogs", "Label Name", None, Some("7190")),
            musicbrainz_label: existing(
                "by-musicbrainz",
                "Label Name Records",
                Some("mb-label"),
                None,
            ),
        })),
    };

    tables.save_import_candidate_failure(&hash, 0, &expected).await.unwrap();

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap().failure.unwrap();
    assert_eq!(stored, expected);
}

#[tokio::test]
async fn a_late_import_failure_cannot_recreate_state_after_root_removal() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/mounted/library");
    let candidate = scanned_candidate(&root, "Release");
    let crate::folder_scanner::ScanItem::Valid(folder) = &candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = folder.files.content_hash();
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(&root, generation, &candidate).await.unwrap();
    tables.remove_watched_import_folders(vec![root.clone()], None).await.unwrap();

    tables
        .save_import_candidate_failure(
            &content_hash,
            0,
            &crate::ImportFailure::error_only("the source disappeared", fixed_now()),
        )
        .await
        .expect_err("a removed candidate cannot receive an import failure");
    assert!(tables.load_import_candidate_state(&content_hash).await.unwrap().is_none());
}
