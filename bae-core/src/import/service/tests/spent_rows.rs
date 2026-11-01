/// The rows an import spends, seeded under `content_hash` the way the pane
/// and a run leave them: a found verdict with one release and the ledger
/// whose barcode lookup found it, the lookups the person let it use, where
/// the pane was, an artist decision, a label decision, a picked album and a
/// link to an album, beside the draft and the folder's cover the candidate
/// holds.
async fn seed_spent_rows(test: &TestService, content_hash: &str) {
    let database = &test.database;
    for statement in [
        "INSERT INTO import_candidate_verdict \
             (content_hash, kind, track_count, kept_own_draft, disc_id_step, barcode_step, \
              catalog_step, isrc_step, search_step) \
         VALUES (?1, 'found', 3, 0, 'absent', 'rows', 'none_found', 'absent', 'not_needed')",
        "INSERT INTO import_candidate_match \
             (content_hash, position, listing, pressing, source, release_id, title, media_kind, \
              album_links) \
         VALUES (?1, 0, 'offered', 0, 'musicbrainz', 'release-1', 'Box Set', 'undescribed', \
                 'not_asked')",
        "INSERT INTO import_candidate_lookup_provider (content_hash, position, catalog) \
         VALUES (?1, 0, 'musicbrainz')",
        "INSERT INTO import_candidate_lookup_value \
             (content_hash, step, position, value, excluded) \
         VALUES (?1, 'barcode', 0, '012345678905', 0)",
        "INSERT INTO import_candidate_lookup \
             (content_hash, step, position, cell, catalog, answer) \
         VALUES (?1, 'barcode', 0, 0, 'musicbrainz', 'answered')",
        "INSERT INTO import_candidate_lookup_release \
             (content_hash, step, position, cell, ordinal, match_position) \
         VALUES (?1, 'barcode', 0, 0, 0, 0)",
        "INSERT INTO import_candidate_lookup_choices (content_hash, disc_id_excluded) \
         VALUES (?1, 0)",
        "INSERT INTO import_candidate_chosen_catalog (content_hash, position, value) \
         VALUES (?1, 0, 'CAT-1')",
        "INSERT INTO import_candidate_session \
             (content_hash, presentation, find_online_section, search_tab, search_artist, \
              search_album, search_catalog, search_barcode) \
         VALUES (?1, 'find_online', 'automatic', 'general', '', '', '', '')",
        "INSERT INTO import_candidate_artist_identity_choice \
             (content_hash, discogs_artist_id, musicbrainz_artist_id, choice, artist_id) \
         VALUES (?1, 'discogs-1', 'musicbrainz-1', 'neither', 'artist-reserved')",
        "INSERT INTO import_candidate_label_identity_choice \
             (content_hash, discogs_label_id, musicbrainz_label_id, choice, label_id) \
         VALUES (?1, 'discogs-label-1', 'musicbrainz-label-1', 'neither', 'label-reserved')",
        "INSERT INTO albums (id, title, created_at) \
         VALUES ('album-picked', 'Picked Album', '2026-01-01T00:00:00+00:00')",
        "INSERT INTO import_candidate_album_pick (content_hash, album_id) \
         VALUES (?1, 'album-picked')",
        "INSERT INTO import_candidate_release_link (content_hash, kind) VALUES (?1, 'album')",
        "INSERT INTO import_candidate_release_link_album (content_hash, kind, catalog, album_id) \
         VALUES (?1, 'album', 'musicbrainz', 'mb-group')",
    ] {
        // The album is the library's own row; the rest are the candidate's.
        let params = match statement.contains("?1") {
            true => vec![content_hash.to_string()],
            false => Vec::new(),
        };
        database
            .execute_for_test(statement, params)
            .await
            .unwrap();
    }
}

/// How many rows each spent table holds under `content_hash`.
async fn spent_row_counts(test: &TestService, content_hash: &str) -> Vec<(&'static str, i64)> {
    let mut counts = Vec::new();
    for table in [
        "import_candidate_verdict",
        "import_candidate_match",
        "import_candidate_lookup_provider",
        "import_candidate_lookup_value",
        "import_candidate_lookup",
        "import_candidate_lookup_release",
        "import_candidate_lookup_choices",
        "import_candidate_chosen_catalog",
        "import_candidate_session",
        "import_candidate_artist_identity_choice",
        "import_candidate_label_identity_choice",
        "import_candidate_album_pick",
        "import_candidate_edit",
        "import_candidate_album_artist_assignment",
        "import_candidate_track",
        "import_candidate_release_link",
        "import_candidate_release_link_album",
        "import_candidate_cover",
    ] {
        let count = test
            .database
            .count_rows_for_test(table, "content_hash", content_hash)
            .await
            .unwrap();
        counts.push((table, count));
    }
    counts
}

/// A three-track folder with a front cover image, stored as a scanned
/// candidate named "Box Set".
async fn spent_rows_candidate(
    test: &TestService,
) -> (PathBuf, crate::import::CandidateKey, super::ImportExpectation) {
    let folder = test.temp.path().join("box-set");
    std::fs::create_dir_all(&folder).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for track in 1..=3 {
        std::fs::copy(&fixture, folder.join(format!("0{track} Track {track}.flac"))).unwrap();
    }
    write_test_jpeg(&folder.join("front.jpg"));
    let (candidate_key, expectation) = store_scanned_candidate(test, &folder, "Box Set").await;
    (folder, candidate_key, expectation)
}

/// The import's own write deletes what the candidate holds that nothing reads
/// once its release is in — the verdict, its releases and its ledger, the
/// lookup choices, where the pane was, the artist and label decisions, the
/// picked album, and the draft with its source, its link and its cover — and
/// the folder's Done row shows the library album the release was filed
/// under.
#[tokio::test]
async fn an_import_deletes_what_it_spent_and_the_done_row_still_shows() {
    let test = setup_import_service().await;
    let (folder, candidate_key, expectation) = spent_rows_candidate(&test).await;
    let content_hash = expectation.candidate.content_hash.clone();
    seed_spent_rows(&test, &content_hash).await;
    assert!(
        spent_row_counts(&test, &content_hash)
            .await
            .iter()
            .all(|(_, count)| *count > 0),
        "every spent table holds the candidate's rows before the import"
    );

    test.service.queue_import_for_test(&candidate_key, "import-1").await;
    test.service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            candidate_key.clone(),
            crate::import::release_candidate::CandidateSource {
                path: folder.clone(),
                scope: crate::import::ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            expectation,
            ImportDestination::Local,
        )
        .await
        .expect("the import lands");

    assert_eq!(
        spent_row_counts(&test, &content_hash)
            .await
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .collect::<Vec<_>>(),
        Vec::new(),
        "the import's write leaves none of what it spent"
    );
    let done = test
        .database
        .query_texts_for_test(
            "SELECT tab || '|' || shown_title || '|' || shown_artists \
             FROM import_list_placement WHERE item = 'candidate'",
        )
        .await
        .unwrap();
    assert_eq!(
        done,
        vec!["done|Picked Album|".to_string()],
        "the picked album, which credits nobody"
    );
}

/// An import that fails leaves the candidate's rows as they were, so a retry
/// reads what the person and the run settled.
#[tokio::test]
async fn a_failed_import_keeps_what_a_retry_reads() {
    let test = setup_import_service().await;
    let (folder, candidate_key, expectation) = spent_rows_candidate(&test).await;
    let content_hash = expectation.candidate.content_hash.clone();
    seed_spent_rows(&test, &content_hash).await;
    let before = spent_row_counts(&test, &content_hash).await;

    let blocked = crate::test_files::UnopenableFile::block(&folder.join("02 Track 2.flac"));
    test.service.queue_import_for_test(&candidate_key, "import-blocked").await;
    test.service
        .do_import(
            local_import_command("import-blocked", &candidate_key, &folder),
            expectation,
        )
        .await;
    drop(blocked);

    assert!(
        !test
            .service
            .library_manager
            .is_content_hash_imported(&content_hash)
            .await
            .unwrap(),
        "the import failed"
    );
    assert_eq!(spent_row_counts(&test, &content_hash).await, before);
}

/// Store the scanned folder `folder` again as its watched root's next scan
/// reads it, as `name`.
async fn rescan_folder(test: &TestService, folder: &Path, name: &str) {
    let manager = &test.service.library_manager;
    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let generation = manager.begin_folder_scan(folder).await.unwrap();
    manager
        .save_folder_scan_item(
            folder,
            generation,
            &ScanItem::Valid(crate::import::folder_scanner::FolderCandidate {
                path: folder.to_path_buf(),
                file_root: folder.to_path_buf(),
                name: name.to_string(),
                files,
                watched_folder_path: folder.to_path_buf(),
                scope: crate::import::ReleaseFileScope::Recursive,
                file_edit_revision: 0,
                relative_folder: crate::import::RelativeFolder::test(name),
                grouping: None,
            }),
        )
        .await
        .unwrap()
        .expect("the stored scan generation is current");
    manager
        .finish_folder_scan(folder, generation, None)
        .await
        .unwrap();
}

/// What the folder's row shows: its tab, title and artists.
async fn shown_row(test: &TestService) -> Vec<String> {
    test.database
        .query_texts_for_test(
            "SELECT tab || '|' || shown_title || '|' || COALESCE(shown_artists, '-') \
             FROM import_list_placement WHERE item = 'candidate'",
        )
        .await
        .unwrap()
}

/// An imported candidate holds no draft, and a scan seeds none for it. Once
/// its release is deleted the folder is back in Found as a new one is when
/// its tags are not read: its blank draft, which no row holds and nobody
/// wrote, under its folder's name, until the first write to its draft
/// stores the row.
#[tokio::test]
async fn a_folder_whose_release_is_deleted_reads_as_its_blank_draft() {
    let test = setup_import_service().await;
    let (folder, candidate_key, expectation) = spent_rows_candidate(&test).await;
    let content_hash = expectation.candidate.content_hash.clone();
    test.service.queue_import_for_test(&candidate_key, "import-1").await;
    test.service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            candidate_key.clone(),
            crate::import::release_candidate::CandidateSource {
                path: folder.clone(),
                scope: crate::import::ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            expectation,
            ImportDestination::Local,
        )
        .await
        .expect("the import lands");
    let drafts = || async {
        test.database
            .count_rows_for_test("import_candidate_edit", "content_hash", &content_hash)
            .await
            .unwrap()
    };
    // Under another name, so the scan writes the folder's row anew.
    rescan_folder(&test, &folder, "Box Set Renamed").await;
    assert_eq!(drafts().await, 0, "a scan seeds no draft for files imported here");

    let release_id = test
        .service
        .library_manager
        .release_imported_from(&content_hash)
        .await
        .unwrap()
        .expect("the import is recorded");
    test.database
        .execute_for_test("DELETE FROM releases WHERE id = ?1", vec![release_id])
        .await
        .unwrap();
    assert_eq!(shown_row(&test).await, vec!["pending|Box Set Renamed|-".to_string()]);
    let pane = test
        .service
        .library_manager
        .load_import_candidate(&candidate_key)
        .await
        .unwrap()
        .expect("the folder is listed")
        .resolve();
    let draft = pane.draft().expect("a folder nobody imported holds a draft");
    assert!(draft.metadata_draft.is_blank(), "{:?}", draft.metadata_draft);
    assert_eq!(draft.metadata_draft.tracks.len(), 3);
    assert_eq!(draft.metadata_author, crate::import::MetadataAuthor::Nobody);
    assert_eq!(draft.metadata_provenance, None);
    assert_eq!(
        draft.cover.as_ref().map(|cover| cover.selection.clone()),
        Some(crate::import::CoverSelection::Local("front.jpg".to_string())),
        "the folder's own cover"
    );
    rescan_folder(&test, &folder, "Box Set").await;
    assert_eq!(drafts().await, 0, "the scan leaves a stored state as it is");
    assert_eq!(shown_row(&test).await, vec!["pending|Box Set|-".to_string()]);

    prepare_named_candidate(
        &test.service,
        &test.preparations,
        &content_hash,
        &folder,
        &candidate_key,
        "Box Set Again",
    )
    .await;
    assert_eq!(drafts().await, 1, "the first write stores the draft");
    assert_eq!(
        shown_row(&test).await,
        vec!["pending|Box Set Again|Artist Name".to_string()]
    );
}
