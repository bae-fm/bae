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
    let bugs = Arc::new(crate::diagnostics::RecordingBugReporter::default());
    test.service.diagnostics =
        crate::diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
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
        test.database.get_albums(&[]).await.unwrap().len(),
        1,
        "the next import lands the release"
    );
}
