/// The rows an import spends, seeded under `content_hash` the way the pane
/// and a run leave them: a found verdict with one release and the ledger
/// whose barcode lookup found it, the lookups the person let it use, where
/// the pane was, an artist decision and a picked album.
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
        "INSERT INTO albums (id, title, created_at) \
         VALUES ('album-picked', 'Picked Album', '2026-01-01T00:00:00+00:00')",
        "INSERT INTO import_candidate_album_pick (content_hash, album_id) \
         VALUES (?1, 'album-picked')",
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
        "import_candidate_album_pick",
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

/// A three-track folder stored as a scanned candidate named "Box Set".
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
    let (candidate_key, expectation) = store_scanned_candidate(test, &folder, "Box Set").await;
    (folder, candidate_key, expectation)
}

/// The import's own write deletes what the candidate holds that nothing reads
/// once its release is in — the verdict, its releases and its ledger, the
/// lookup choices, where the pane was, the artist decisions and the picked
/// album — and the folder's Done row still shows the draft it was
/// imported from.
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
    assert_eq!(done, vec!["done|Box Set|Artist Name".to_string()]);
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
