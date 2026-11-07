//! Pure reads must run on coven's read-only connection, which rejects a write
//! callback that prepares no INSERT, UPDATE, or DELETE.

#[tokio::test]
async fn pure_reads_use_the_read_connection() {
    let (db, tmp) = bae_store::test_fixtures::temp_db(bae_library::SCHEMA).await;
    let records = bae_records::test_fixtures::test_records(&db);

    let tables = bae_import::tables::ImportTables::new(db.clone());

    // A device-local write still uses the write connection.
    let queue = bae_playback::tables::PlaybackQueueTables::new(db.clone());
    queue
        .save_playback_state(&bae_playback::tables::PlaybackResumeRow {
            current: None,
            position_ms: None,
        })
        .await
        .unwrap();

    // At least one read per file of the records' client.
    records.find_album_by_id("missing").await.unwrap();
    bae_views::LibraryViews::new(db.clone()).get_album_count().await.unwrap();
    bae_views::LibraryViews::new(db.clone()).get_albums(&[]).await.unwrap();
    records.find_artist_by_id("missing").await.unwrap();
    bae_views::LibraryViews::new(db.clone()).get_artist_count().await.unwrap();
    records.find_track_by_id("missing").await.unwrap();
    records.get_track_ids_for_release("missing").await.unwrap();
    records.find_release_by_id("missing").await.unwrap();
    records.get_release_records("missing").await.unwrap();
    bae_playback::tables::load_playback_state(&db).await.unwrap();
    bae_playback::tables::queue_value(&db).await.unwrap();
    let queue = bae_playback::tables::PlaybackQueueTables::new(db.clone());
    queue.repeat().await.unwrap();
    queue.context_track_count().await.unwrap();
    queue.front().await.unwrap();
    queue.next_in_order().await.unwrap();
    records.has_pending_cloud_upload("missing").await.unwrap();

    // The tables the pane writes read back the same way.
    tables.load_import_candidate_state("missing").await.unwrap();
    assert!(matches!(
        tables.load_import_candidate_pane_rows("missing").await,
        Err(bae_store::DbError::Unreadable { .. })
    ));

    // Writers that already have the requested state decide before opening a
    // write transaction.
    let root = tmp.path().join("watched");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.to_str().unwrap();

    queue.clear_playback_state().await.unwrap();
    queue.clear_playback_state().await.unwrap();
    tables.add_watched_import_folder(std::path::Path::new(root)).await.unwrap();
    assert!(tables.add_watched_import_folder(std::path::Path::new(root)).await.unwrap().is_none());
    let album = std::path::Path::new(root).join("Album");
    let album = album.to_str().unwrap();
    let never_skipped = std::path::Path::new(root).join("Never Skipped");
    tables.set_import_candidate_skipped(std::path::Path::new(album), true).await.unwrap();
    assert!(!tables.set_import_candidate_skipped(std::path::Path::new(album), true).await.unwrap());
    tables.set_import_candidate_skipped(&never_skipped, false).await.unwrap();
    let generation = tables
        .begin_folder_scan(std::path::Path::new(root), bae_import::VolumeKind::Local)
        .await
        .unwrap();
    assert!(tables
        .finish_folder_scan(std::path::Path::new(root), generation - 1, None)
        .await
        .unwrap()
        .is_none());
    assert!(!bae_import::tables::ImportTables::new(db.clone())
        .store_verdict(&bae_import::tables::NewImportCandidateVerdict {
            content_hash: "hash-with-no-row".to_string(),
            file_edit_revision: 7,
            candidate_key: bae_import::CandidateKey::folder(
                &std::path::Path::new(root).join("Album")
            ),
            verdict: bae_identify::TerminalVerdict::NotFoundAnywhere { ledger: None },
            signals: bae_identify::signals::Signals {
                origin: bae_catalogs::AudioOrigin::default(),
                disc_id: bae_identify::signals::DiscIdSignal::Absent,
                barcode: bae_identify::signals::BarcodeSignal::Absent,
                text: bae_identify::signals::TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                isrcs: Vec::new(),
                track_titles: Vec::new(),
            },
            pick: Some(bae_import::tables::VerdictPick {
                link: bae_identify::PressingLink {
                    record: bae_catalogs::records::LookupRef::new(
                        bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                        "unwritten-release",
                    ),
                    partners: Vec::new(),
                },
                metadata: bae_import::CandidateMetadataDraft {
                    draft: bae_import::CandidateDraft {
                        album_title: "Unwritten candidate".to_string(),
                        album_artist_assignments: Vec::new(),
                        album_year: String::new(),
                        pressing: bae_import::RawPressingEdit {
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
