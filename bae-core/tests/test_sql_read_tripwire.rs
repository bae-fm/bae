#![cfg(feature = "test-utils")]
//! Pure reads must run on coven's read-only companion connection. Coven rejects
//! a write callback that prepares no INSERT, UPDATE, or DELETE statement, so
//! every read below checks bae's routing without exposing coven's retained
//! handle.

use std::sync::Arc;

use bae_core::db::Database;

#[tokio::test]
async fn pure_reads_use_the_read_connection() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db = Database::new_test(
        tmp.path().join("tripwire.db").to_str().unwrap(),
        Arc::new(coven::SystemClock),
        Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();

    // A device-local write still uses the write connection.
    db.save_playback_state(&bae_core::db::DbPlaybackState {
        context: None,
        manual: "off".to_string(),
        repeat: "off".to_string(),
        current_track_id: None,
        position_ms: None,
        volume: 1.0,
        is_muted: false,
    })
    .await
    .unwrap();

    // At least one read per db/client file. A read routed through write returns
    // coven's ReadOnlyWriteTransaction error here.
    db.find_album_by_id("missing").await.unwrap();
    db.get_album_count().await.unwrap();
    db.get_albums(&[]).await.unwrap();
    db.find_artist_by_id("missing").await.unwrap();
    db.get_artist_count().await.unwrap();
    db.find_track_by_id("missing").await.unwrap();
    db.get_all_track_ids().await.unwrap();
    db.find_release_by_id("missing").await.unwrap();
    db.get_release_identities("missing").await.unwrap();
    db.load_playback_state().await.unwrap();
    db.has_pending_cloud_upload("missing").await.unwrap();
    db.outbox_queue().await.unwrap();

    // The six device-local tables the pane writes read back the same way.
    // `load_import_candidate_state` covers import_candidate_signals and
    // import_candidate_signal_value;
    // `load_import_candidate_pane_rows` covers import_candidate_failure,
    // import_candidate_cover, import_candidate_edit and
    // import_candidate_track_edit.
    db.load_import_candidate_state("missing").await.unwrap();
    db.load_import_candidate_pane_rows("missing").await.unwrap();

    // Writers that already have the requested state decide before opening a
    // write transaction.
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();

    db.clear_playback_state().await.unwrap();
    db.clear_playback_state().await.unwrap();
    db.add_watched_import_folder(root).await.unwrap();
    assert!(!db.add_watched_import_folder(root).await.unwrap());
    db.set_import_candidate_skipped(root, "Album", true)
        .await
        .unwrap();
    assert!(!db
        .set_import_candidate_skipped(root, "Album", true)
        .await
        .unwrap());
    db.set_import_candidate_skipped(root, "Never Skipped", false)
        .await
        .unwrap();
    let generation = db.begin_folder_scan(root).await.unwrap();
    assert!(db
        .finish_folder_scan(root, generation - 1, None)
        .await
        .unwrap()
        .is_none());
    assert!(!db
        .save_import_candidate_verdict(&bae_core::db::NewImportCandidateVerdict {
            content_hash: "hash-with-no-row".to_string(),
            folder_path: format!("{root}/Album"),
            verdict: bae_core::identify::TerminalVerdict::NotFoundAnywhere,
            signals: bae_core::signals::Signals {
                disc_id: bae_core::signals::DiscIdSignal::Absent { track_count: 0 },
                barcode: bae_core::signals::BarcodeSignal::Absent,
                text: bae_core::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                durations: bae_core::import::probe::SourceDurations::default(),
            },
            expected_edit_revision: 7,
            expected_metadata_revision: 7,
            metadata: bae_core::import::CandidateMetadataDraft {
                edit: bae_core::import::RawReleaseEdit {
                    album_title: "Unwritten candidate".to_string(),
                    album_artist_assignments: Vec::new(),
                    album_year: String::new(),
                    pressing: bae_core::import::RawPressingEdit {
                        year: String::new(),
                        format: String::new(),
                        label: String::new(),
                        catalog_number: String::new(),
                        country: String::new(),
                        barcode: String::new(),
                    },
                    tracks: Vec::new(),
                },
                provenance: None,
                cover: None,
            },
        })
        .await
        .unwrap());
    assert!(db
        .remove_watched_import_folder("/nothing/watches/this")
        .await
        .unwrap()
        .is_none());

    // Clearing a failure nothing stored still names a DELETE, so coven takes
    // the write callback rather than rejecting it as a disguised read.
    db.clear_import_candidate_failure("hash-that-never-failed")
        .await
        .unwrap();
}
