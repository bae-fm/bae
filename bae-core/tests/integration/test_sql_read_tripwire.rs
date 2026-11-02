//! Pure reads must run on coven's read-only connection, which rejects a write
//! callback that prepares no INSERT, UPDATE, or DELETE.

#[tokio::test]
async fn pure_reads_use_the_read_connection() {
    let (db, tmp) = bae_test_support::temp_test_db().await;

    let tables = bae_core::import::tables::ImportTables::new(db.clone());

    // A device-local write still uses the write connection.
    db.save_playback_state(&bae_core::db::DbPlaybackState { current: None, position_ms: None })
        .await
        .unwrap();

    // At least one read per db/client file.
    db.find_album_by_id("missing").await.unwrap();
    db.get_album_count().await.unwrap();
    db.get_albums(&[]).await.unwrap();
    db.find_artist_by_id("missing").await.unwrap();
    db.get_artist_count().await.unwrap();
    db.find_track_by_id("missing").await.unwrap();
    db.get_track_ids_for_release("missing").await.unwrap();
    db.find_release_by_id("missing").await.unwrap();
    db.get_release_records("missing").await.unwrap();
    db.load_playback_state().await.unwrap();
    db.queue_value().await.unwrap();
    let queue = db.playback_queue_tables();
    queue.repeat().await.unwrap();
    queue.context_track_count().await.unwrap();
    queue.front().await.unwrap();
    queue.next_in_order().await.unwrap();
    db.has_pending_cloud_upload("missing").await.unwrap();

    // The tables the pane writes read back the same way.
    tables.load_import_candidate_state("missing").await.unwrap();
    assert_eq!(
        tables.load_import_candidate_pane_rows("missing").await.unwrap_err().to_string(),
        "candidate missing has no state"
    );

    // Writers that already have the requested state decide before opening a
    // write transaction.
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();

    db.clear_playback_state().await.unwrap();
    db.clear_playback_state().await.unwrap();
    tables.add_watched_import_folder(std::path::Path::new(root)).await.unwrap();
    assert!(!tables.add_watched_import_folder(std::path::Path::new(root)).await.unwrap());
    let album = std::path::Path::new(root).join("Album");
    let album = album.to_str().unwrap();
    let never_skipped = std::path::Path::new(root).join("Never Skipped");
    tables.set_import_candidate_skipped(std::path::Path::new(album), true).await.unwrap();
    assert!(!tables.set_import_candidate_skipped(std::path::Path::new(album), true).await.unwrap());
    tables.set_import_candidate_skipped(&never_skipped, false).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(root), bae_core::import::VolumeKind::Local)
        .await
        .unwrap();
    assert!(tables
        .finish_folder_scan(std::path::Path::new(root), generation - 1, None)
        .await
        .unwrap()
        .is_none());
    assert!(!bae_core::import::CandidatePreparations::new(db.clone())
        .store_verdict(&bae_core::import::tables::NewImportCandidateVerdict {
            content_hash: "hash-with-no-row".to_string(),
            file_edit_revision: 7,
            candidate_key: bae_core::import::CandidateKey::folder(
                &std::path::Path::new(root).join("Album")
            ),
            verdict: bae_core::identify::TerminalVerdict::NotFoundAnywhere { ledger: None },
            signals: bae_core::identify::signals::Signals {
                origin: bae_core::catalog_model::AudioOrigin::default(),
                disc_id: bae_core::identify::signals::DiscIdSignal::Absent,
                barcode: bae_core::identify::signals::BarcodeSignal::Absent,
                text: bae_core::identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: Some(bae_core::import::tables::VerdictPick {
                link: bae_core::identify::PressingLink {
                    record: bae_core::catalog_model::records::LookupRef::new(
                        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
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
                            barcodes: Vec::new(),
                        },
                        mediums: Vec::new(),
                        tracks: Vec::new(),
                        release_roles: Vec::new(),
                        works: Default::default(),
                    },
                    provenance: None,
                    cover: None,
                    assets: bae_core::import::CandidatePreparedAssets::default(),
                },
            }),
            releases: Vec::new(),
        })
        .await
        .unwrap());
    assert!(tables
        .remove_watched_import_folders(
            vec![std::path::PathBuf::from("/nothing/watches/this")],
            None
        )
        .await
        .unwrap()
        .is_none());
}
