//! An import run: its failure as stored, a retry after a source could not
//! be opened, an import that panics, what it spends and keeps, its reading
//! progress, and committing after a pass found its folder unchanged.

use super::*;
use crate::folder_scanner::ScanItem;
use crate::service::tests::*;

fn local_import_command(
    import_id: &str,
    candidate_key: &crate::CandidateKey,
    folder: &Path,
) -> ImportCommand {
    ImportCommand {
        import_id: import_id.to_string(),
        candidate_key: candidate_key.clone(),
        source: crate::release_candidate::CandidateSource {
            path: folder.to_path_buf(),
            scope: crate::ReleaseFileScope::Recursive,
            parts: Vec::new(),
        },
        selected_cover: None,
        destination: ImportDestination::Local,
        pick: None,
        read_file_tags: false,
        user_edit: None,
    }
}

/// A source file that cannot be opened mid-import fails the import with the
/// open's own error and leaves nothing of the release behind: no album, no
/// release, and the candidate's failure recorded for its pane. Importing the
/// same candidate again once the file opens lands the whole release.
#[tokio::test]
async fn an_import_that_cannot_open_a_source_writes_nothing_and_a_retry_lands_it() {
    let test = setup_import_service().await;
    let tables = test.service.tables.clone();
    let folder = test.temp.path().join("box-set");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for disc in 1..=2 {
        let disc_dir = folder.join(format!("CD{disc}"));
        std::fs::create_dir_all(&disc_dir).unwrap();
        for track in 1..=3 {
            std::fs::copy(&fixture, disc_dir.join(format!("0{track} Track {track}.flac"))).unwrap();
        }
    }
    let (candidate_key, stored) = store_scanned_candidate(&test, &folder, "Box Set").await;
    let candidate = stored.candidate.clone();
    let expectation = || stored.clone();

    // The scan read every file; now one stops opening. Its size and
    // modification time are unchanged, so the import's identity check passes
    // and the failure is the open itself.
    let blocked_name = Path::new("CD2").join("02 Track 2.flac");
    let blocked = crate::tests::files::UnopenableFile::block(&folder.join(&blocked_name));
    test.service.queue_import_for_test(&candidate_key, "import-blocked").await;
    test.service
        .do_import(local_import_command("import-blocked", &candidate_key, &folder), expectation())
        .await;
    drop(blocked);

    let failure = tables
        .load_import_candidate_pane_rows(&candidate.content_hash)
        .await
        .unwrap()
        .failure
        .expect("the failed import is recorded on its candidate");
    let detail = &failure.reason.detail;
    assert!(
        detail.contains(&*blocked_name.to_string_lossy()),
        "the failure names the file that would not open: {detail}"
    );
    assert!(
        bae_db::test_fixtures::row_count(&test.database, "albums").await == 0,
        "a failed import writes no album"
    );
    assert!(
        test.service.tables.release_imported_from(&candidate.content_hash).await.unwrap().is_none(),
        "a failed import writes no release of its files"
    );

    test.service.queue_import_for_test(&candidate_key, "import-retry").await;
    test.service
        .prepare_and_run_folder_import(
            "import-retry".to_string(),
            candidate_key.clone(),
            crate::release_candidate::CandidateSource {
                path: folder.clone(),
                scope: crate::ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            expectation(),
            ImportDestination::Local,
        )
        .await
        .expect("the retried import lands");

    assert_eq!(
        bae_db::test_fixtures::row_count(&test.database, "albums").await,
        1,
        "the retry writes the release once"
    );
    assert_eq!(bae_db::test_fixtures::row_count(&test.database, "releases").await, 1);
    let release = tables
        .release_imported_from(&candidate.content_hash)
        .await
        .unwrap()
        .expect("the retried import records the release of its files");
    assert_eq!(test.database.get_tracks_for_release(&release).await.unwrap().len(), 6);
}

/// Ids for an import whose first id panics, as a broken dependency of the
/// import's own work would.
struct IdsThatPanicOnce {
    panicked: std::sync::atomic::AtomicBool,
}

impl coven::IdSource for IdsThatPanicOnce {
    fn new_id(&self) -> uuid::Uuid {
        if !self.panicked.swap(true, std::sync::atomic::Ordering::SeqCst) {
            panic!("the id source broke");
        }
        coven::IdSource::new_id(&coven::UuidIds)
    }
}

/// An import whose work panics fails as one that errs: its failure is
/// recorded on its candidate, the crash reporter hears of it once, and the
/// worker takes up the next import of the candidate, which lands.
#[tokio::test]
async fn an_import_that_panics_is_recorded_as_failed_and_the_next_import_runs() {
    let mut test = setup_import_service().await;
    let tables = test.service.tables.clone();
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    test.service.diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let folder = test.temp.path().join("album");
    std::fs::create_dir_all(&folder).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac");
    for track in 1..=2 {
        std::fs::copy(&fixture, folder.join(format!("0{track} Track {track}.flac"))).unwrap();
    }
    let (candidate_key, stored) = store_scanned_candidate(&test, &folder, "Album").await;
    test.service.ids =
        Arc::new(IdsThatPanicOnce { panicked: std::sync::atomic::AtomicBool::new(false) });

    test.service.queue_import_for_test(&candidate_key, "import-panics").await;
    test.service
        .do_import(local_import_command("import-panics", &candidate_key, &folder), stored.clone())
        .await;

    let failure = tables
        .load_import_candidate_pane_rows(&stored.candidate.content_hash)
        .await
        .unwrap()
        .failure
        .expect("the panicked import is recorded on its candidate");
    assert_eq!(
        failure.reason.detail,
        "internal import error: an import panicked: the id source broke"
    );
    assert_eq!(
        bugs.reports(),
        vec![("an import".to_string(), "an import panicked: the id source broke".to_string())]
    );

    test.service.queue_import_for_test(&candidate_key, "import-again").await;
    test.service
        .do_import(local_import_command("import-again", &candidate_key, &folder), stored)
        .await;
    assert_eq!(
        bae_db::test_fixtures::row_count(&test.database, "albums").await,
        1,
        "the next import lands the release"
    );
}

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
        database.execute_for_test(statement, params).await.unwrap();
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
        "import_candidate_album_pick",
        "import_candidate_edit",
        "import_candidate_album_artist_assignment",
        "import_candidate_track",
        "import_candidate_release_link",
        "import_candidate_release_link_album",
        "import_candidate_cover",
    ] {
        let count =
            test.database.count_rows_for_test(table, "content_hash", content_hash).await.unwrap();
        counts.push((table, count));
    }
    counts
}

/// A three-track folder with a front cover image, stored as a scanned
/// candidate named "Box Set".
async fn spent_rows_candidate(
    test: &TestService,
) -> (PathBuf, crate::CandidateKey, crate::service::ImportExpectation) {
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
        spent_row_counts(&test, &content_hash).await.iter().all(|(_, count)| *count > 0),
        "every spent table holds the candidate's rows before the import"
    );

    test.service.queue_import_for_test(&candidate_key, "import-1").await;
    test.service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            candidate_key.clone(),
            crate::release_candidate::CandidateSource {
                path: folder.clone(),
                scope: crate::ReleaseFileScope::Recursive,
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

    let blocked = crate::tests::files::UnopenableFile::block(&folder.join("02 Track 2.flac"));
    test.service.queue_import_for_test(&candidate_key, "import-blocked").await;
    test.service
        .do_import(local_import_command("import-blocked", &candidate_key, &folder), expectation)
        .await;
    drop(blocked);

    assert!(
        test.service.tables.release_imported_from(&content_hash).await.unwrap().is_none(),
        "the import failed"
    );
    assert_eq!(spent_row_counts(&test, &content_hash).await, before);
}

/// Store the scanned folder `folder` again as its watched root's next scan
/// reads it, as `name`.
async fn rescan_folder(test: &TestService, folder: &Path, name: &str) {
    let tables = &test.service.tables;
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let generation = tables.begin_folder_scan(folder, crate::VolumeKind::Local).await.unwrap();
    crate::store_scan_item(
        tables,
        &bae_diagnostics::Diagnostics::noop(),
        folder,
        generation,
        &ScanItem::Valid(crate::folder_scanner::FolderCandidate {
            path: folder.to_path_buf(),
            file_root: folder.to_path_buf(),
            name: name.to_string(),
            files,
            watched_folder_path: folder.to_path_buf(),
            scope: crate::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            relative_folder: crate::RelativeFolder::test(name),
            grouping: None,
        }),
    )
    .await
    .unwrap()
    .expect("the stored scan generation is current");
    tables.finish_folder_scan(folder, generation, None).await.unwrap();
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
    let tables = test.service.tables.clone();
    let (folder, candidate_key, expectation) = spent_rows_candidate(&test).await;
    let content_hash = expectation.candidate.content_hash.clone();
    test.service.queue_import_for_test(&candidate_key, "import-1").await;
    test.service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            candidate_key.clone(),
            crate::release_candidate::CandidateSource {
                path: folder.clone(),
                scope: crate::ReleaseFileScope::Recursive,
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
        .tables
        .release_imported_from(&content_hash)
        .await
        .unwrap()
        .expect("the import is recorded");
    test.database
        .execute_for_test("DELETE FROM releases WHERE id = ?1", vec![release_id])
        .await
        .unwrap();
    assert_eq!(shown_row(&test).await, vec!["pending|Box Set Renamed|-".to_string()]);
    let pane = tables
        .load_import_candidate(&candidate_key)
        .await
        .unwrap()
        .expect("the folder is listed")
        .resolve();
    let draft = pane.draft().expect("a folder nobody imported holds a draft");
    assert!(draft.metadata_draft.is_blank(), "{:?}", draft.metadata_draft);
    assert_eq!(draft.metadata_draft.tracks.len(), 3);
    assert_eq!(draft.metadata_author, crate::MetadataAuthor::Nobody);
    assert_eq!(draft.metadata_provenance, None);
    assert_eq!(
        draft.cover.as_ref().map(|cover| cover.selection().clone()),
        Some(crate::CoverSelection::Local("front.jpg".to_string())),
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
    assert_eq!(shown_row(&test).await, vec!["pending|Box Set Again|Artist Name".to_string()]);
}

#[tokio::test]
async fn reading_progress_advances_while_coven_prepares_a_dominant_file() {
    assert_reading_progress(ImportDestination::Remote { pin: false }).await;
}

/// A local import reads each original whole too, for the hashes its move to
/// the cloud is checked against, so its progress counts the bytes read.
#[tokio::test]
async fn local_reading_progress_advances_while_coven_reads_a_dominant_file() {
    assert_reading_progress(ImportDestination::Local).await;
}

async fn assert_reading_progress(destination: ImportDestination) {
    let test = setup_import_service().await;
    let folder = test.temp.path().join("reading-progress-candidate");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("01-payload.bin"), vec![0x5a; 1024 * 1024]).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        folder.join("00-track.flac"),
    )
    .unwrap();
    let track_bytes = std::fs::metadata(folder.join("00-track.flac")).unwrap().len();
    // The percent once the track, read first, is read: what counting whole
    // files alone would report until the payload is read too.
    let after_track = (track_bytes * 100 / (track_bytes + 1024 * 1024)) as u8;
    let (candidate_key, expectation) =
        store_scanned_candidate(&test, &folder, "Reading Progress Candidate").await;
    let service = &test.service;

    let mut events = service.event_tx.every_event();
    service.queue_import_for_test(&candidate_key, "import-reading-progress").await;
    service
        .prepare_and_run_folder_import(
            "import-reading-progress".to_string(),
            candidate_key.clone(),
            crate::release_candidate::CandidateSource {
                path: folder,
                scope: crate::ReleaseFileScope::Recursive,
                parts: Vec::new(),
            },
            expectation,
            destination,
        )
        .await
        .unwrap();

    let mut reading_percents = Vec::new();
    loop {
        match events.try_recv() {
            Ok(crate::ImportEvent::ImportProgress {
                progress:
                    ImportProgress::Progress {
                        percent: Some(percent),
                        phase: ImportPhase::ReadingFiles,
                        ..
                    },
                ..
            }) => reading_percents.push(percent),
            Ok(_) => {}
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
            Err(error) => panic!("import progress event stream failed: {error}"),
        }
    }
    assert_eq!(reading_percents.first(), Some(&0));
    assert_eq!(reading_percents.last(), Some(&100));
    assert!(
        reading_percents.iter().any(|percent| (after_track + 1..100).contains(percent)),
        "a release dominated by one file must advance while that file is read, past \
         {after_track}%: {reading_percents:?}",
    );
    assert!(
        reading_percents.windows(2).all(|pair| pair[0] <= pair[1]),
        "the candidate's Reading Files progress must never move backward: {reading_percents:?}",
    );
}

/// A pass that finds a folder unchanged stamps its file-tag reading with the
/// pass's generation. An import queued against that reading before the pass
/// still commits: what the files say did not change.
#[tokio::test]
async fn an_import_commits_after_a_pass_finds_its_folder_unchanged() {
    let test = setup_import_service().await;
    let tables = test.service.tables.clone();
    let root = test.temp.path().join("watched");
    let folder = root.join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("01.flac"), flac()).unwrap();
    let root_text = root.to_string_lossy().into_owned();
    let key = crate::CandidateKey::folder(&folder);
    tables.add_watched_import_folder(std::path::Path::new(&root_text)).await.unwrap();
    let (scan, _) = test.scan();
    scan.rescan(&root).await.unwrap();
    let candidate = tables.load_release_candidate(&key).await.unwrap().unwrap().unwrap();
    let hash = candidate.files.content_hash();
    let revision = prepare_named_candidate(
        &test.service,
        &test.preparations,
        &hash,
        std::path::Path::new(&root_text),
        &key,
        "Album Title",
    )
    .await;
    let reading = async || {
        tables
            .load_candidate_file_tag_snapshot(std::path::Path::new(&root_text), &key)
            .await
            .unwrap()
            .unwrap()
            .snapshot
            .unwrap()
    };
    let queued = reading().await;

    scan.rescan(&root).await.unwrap();
    assert_ne!(
        reading().await.scan_generation,
        queued.scan_generation,
        "the pass stamps the reading it kept"
    );

    test.service.queue_import_for_test(&key, "import-1").await;
    test.service
        .prepare_and_run_folder_import(
            "import-1".to_string(),
            key,
            candidate.source(),
            crate::service::ImportExpectation {
                candidate: crate::CandidateAsRead {
                    content_hash: hash,
                    file_edit_revision: candidate.file_edit_revision,
                    metadata_revision: revision,
                },
                file_tag_snapshot: Some(queued),
            },
            ImportDestination::Local,
        )
        .await
        .expect("the reading the import was queued against still stands");
}
