#![cfg(feature = "test-utils")]
//! Pure reads must run on coven's read-only connection, which rejects a write
//! callback that prepares no INSERT, UPDATE, or DELETE.

#[tokio::test]
async fn pure_reads_use_the_read_connection() {
    let (db, tmp) = bae_test_support::temp_test_db().await;

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

    // At least one read per db/client file.
    db.find_album_by_id("missing").await.unwrap();
    db.get_album_count().await.unwrap();
    db.get_albums(&[]).await.unwrap();
    db.find_artist_by_id("missing").await.unwrap();
    db.get_artist_count().await.unwrap();
    db.find_track_by_id("missing").await.unwrap();
    db.get_all_track_ids().await.unwrap();
    db.find_release_by_id("missing").await.unwrap();
    db.get_release_records("missing").await.unwrap();
    db.load_playback_state().await.unwrap();
    db.has_pending_cloud_upload("missing").await.unwrap();
    db.outbox_queue().await.unwrap();

    // The tables the pane writes read back the same way.
    db.load_import_candidate_state("missing").await.unwrap();
    assert_eq!(
        db.load_import_candidate_pane_rows("missing")
            .await
            .unwrap_err()
            .to_string(),
        "database error: candidate missing has no editable metadata draft"
    );

    // Writers that already have the requested state decide before opening a
    // write transaction.
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();

    db.clear_playback_state().await.unwrap();
    db.clear_playback_state().await.unwrap();
    db.add_watched_import_folder(root).await.unwrap();
    assert!(!db.add_watched_import_folder(root).await.unwrap());
    let album = std::path::Path::new(root).join("Album");
    let album = album.to_str().unwrap();
    let never_skipped = std::path::Path::new(root).join("Never Skipped");
    db.set_import_candidate_skipped(album, true).await.unwrap();
    assert!(!db.set_import_candidate_skipped(album, true).await.unwrap());
    db.set_import_candidate_skipped(never_skipped.to_str().unwrap(), false)
        .await
        .unwrap();
    let generation = db
        .begin_folder_scan(root, bae_core::import::VolumeKind::Local)
        .await
        .unwrap();
    assert!(db
        .finish_folder_scan(root, generation - 1, None)
        .await
        .unwrap()
        .is_none());
    assert!(!bae_core::import::CandidatePreparations::new(db.clone())
        .store_verdict(&bae_core::db::NewImportCandidateVerdict {
            content_hash: "hash-with-no-row".to_string(),
            file_edit_revision: 7,
            folder_path: format!("{root}/Album"),
            verdict: bae_core::identify::TerminalVerdict::NotFoundAnywhere { ledger: None },
            signals: bae_core::signals::Signals {
                origin: bae_core::signals::AudioOrigin::default(),
                disc_id: bae_core::signals::DiscIdSignal::Absent,
                barcode: bae_core::signals::BarcodeSignal::Absent,
                text: bae_core::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: Some(bae_core::db::VerdictPick {
                link: bae_core::import::PressingLink {
                    record: bae_core::import::MetadataRef::new(
                        bae_core::import::Catalog::MusicBrainz,
                        "unwritten-release",
                    ),
                    partners: Vec::new(),
                },
                metadata: bae_core::import::CandidateMetadataDraft {
                    draft: bae_core::import::CandidateDraft {
                        album_title: "Unwritten candidate".to_string(),
                        album_artist_assignments: Vec::new(),
                        album_year: String::new(),
                        pressing: bae_core::import::RawPressingEdit {
                            year: String::new(),
                            labels: Vec::new(),
                            facts: Default::default(),
                            barcode: String::new(),
                        },
                        tracks: Vec::new(),
                    },
                    source_discogs_artist_ids: Default::default(),
                    provenance: None,
                    cover: None,
                    assets: bae_core::import::CandidatePreparedAssets::default(),
                },
            }),
        })
        .await
        .unwrap());
    assert!(db
        .remove_watched_import_folders(vec!["/nothing/watches/this".to_string()], None)
        .await
        .unwrap()
        .is_none());
}
