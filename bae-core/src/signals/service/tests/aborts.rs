//! An extraction that cannot finish: a blocking task that panicked. What it
//! says, and that it says something.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_fast_pass_that_panics_says_why_there_is_no_pass_and_is_reported() {
    let bugs = Arc::new(crate::diagnostics::RecordingBugReporter::default());
    let (library_manager, _lib_tmp) = make_library_manager_reporting_to(
        crate::diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone()),
    )
    .await;
    let fast_pass = run_fast_pass_blocking(
        &library_manager,
        || -> Result<FastPass, crate::import::ImportError> {
            panic!("fast-pass blocking task panicked")
        },
    )
    .await;

    let Err(detail) = fast_pass else {
        panic!("a fast pass that panics fails, not swallowed");
    };
    assert_eq!(
        detail,
        "read a folder's signals panicked: fast-pass blocking task panicked"
    );
    assert_eq!(
        bugs.reports(),
        vec![("read a folder's signals".to_string(), detail)]
    );
}

/// An extraction that cannot gather its inputs does not go silent: it says so
/// with one snapshot that fails every signal, and nothing follows it. The run
/// it feeds ends on that as an error instead of waiting on a snapshot that is
/// not coming.
#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_extraction_fails_every_signal_in_one_snapshot() {
    let candidates = CandidateRuntime::for_test().await;
    let tx = ImportEventBus::new(candidates.clone());
    let mut rx = tx.every_event();
    let (library_manager, _lib_tmp) = make_library_manager().await;
    let inner = ExtractionServiceInner {
        runtime_handle: tokio::runtime::Handle::current(),
        event_tx: tx,
        analyzer: std::sync::Mutex::new(None),
        library_manager,
        settled: SessionCache::new("Settled folder signals", SETTLED_CAPACITY),
        candidates,
        image_reads: Arc::new(tokio::sync::Semaphore::new(IMAGES_IN_FLIGHT)),
    };
    let generation = inner.candidates.start_work(
        CandidateWork::Extraction,
        crate::import::CandidateKey::test("cand-1"),
        |_, generation| generation,
    );
    let extraction = RunningExtraction {
        run: IdentifyRunId::for_test(1),
        key: crate::import::CandidateKey::test("cand-1"),
        generation,
        priority: CallPriority::Interactive,
        snapshots: watch::channel(None).0,
    };

    let failure = crate::signals::InternalFailure {
        detail: "read a folder's signals panicked: the reader broke".to_string(),
    };
    emit_aborted_signals(
        &inner,
        &extraction,
        DiscIdSignal::Failed {
            failure: failure.clone(),
        },
        failure.clone(),
    );

    let snapshots = collect_snapshots(&mut rx, 1).await;
    let (signals, artwork) = &snapshots[0];
    assert!(
        matches!(&signals.disc_id, DiscIdSignal::Failed { failure: f, .. } if *f == failure),
        "the disc ID failed with the abort's detail, got {:?}",
        signals.disc_id
    );
    assert!(
        matches!(&signals.barcode, BarcodeSignal::Failed { failure: f, codes } if *f == failure && codes.is_empty()),
        "the barcode failed with the abort's detail, got {:?}",
        signals.barcode
    );
    assert!(
        matches!(&signals.text, TextSignal::Failed { failure: f, .. } if *f == failure),
        "the text failed with the abort's detail, got {:?}",
        signals.text
    );
    assert!(
        matches!(
            artwork,
            ArtworkScan::Failed {
                read: 0,
                total: 0,
                ..
            }
        ),
        "the artwork pass failed before reading anything, got {artwork:?}"
    );
    assert_no_more_snapshots(&mut rx, "the aborted extraction's one snapshot");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_image_read_that_panics_fails_the_text_and_barcode_signals() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["cover.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(PanicAnalyzer);
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 2).await;
    assert!(matches!(signals[0].text, TextSignal::Scanning { .. }));

    match &signals[1].text {
        TextSignal::Failed { failure, .. } => {
            assert!(
                failure.detail.contains("read an image's text panicked"),
                "a panicked image read names what panicked, got {failure:?}",
            );
        }
        other => panic!("a panicked image read fails the text signal, got {other:?}"),
    }
    match &signals[1].barcode {
        BarcodeSignal::Failed { failure, .. } => {
            assert!(
                failure.detail.contains("read an image's text panicked"),
                "a panicked image read names what panicked, got {failure:?}",
            );
        }
        other => panic!("a panicked image read fails the barcode signal, got {other:?}"),
    }
}
