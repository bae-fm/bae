//! An extraction that cannot finish: a blocking task that died, a bus with
//! nobody listening. What it says, and that it says something.

use super::*;
use crate::test_logs::capture_warn_logs;

#[tokio::test(flavor = "multi_thread")]
async fn emit_signals_warns_when_broadcast_has_no_subscribers() {
    // Build the inner directly, because a *started* service always holds a receiver
    // (its own candidate-removal listener). The no-subscriber state this warn guards
    // therefore only exists at app shutdown.
    let (tx, rx) = broadcast::channel(64);
    drop(rx);
    let (library_manager, _lib_tmp) = make_library_manager().await;
    let inner = ExtractionServiceInner {
        runtime_handle: tokio::runtime::Handle::current(),
        event_tx: tx,
        analyzer: std::sync::Mutex::new(None),
        library_manager,
        cancellation: CancellationRegistry::default(),
    };

    // A registered generation, as every running extraction has: an
    // unregistered one is not current and sends nothing to warn about.
    let generation = inner
        .cancellation
        .register("cand-1".to_string(), |_, generation| generation);
    let extraction = RunningExtraction {
        run: IdentifyRunId::for_test(1),
        key: "cand-1".to_string(),
        generation,
        priority: CallPriority::Interactive,
        snapshots: watch::channel(None).0,
    };

    let logs = capture_warn_logs(|| {
        emit_signals(
            &inner,
            &extraction,
            Signals {
                disc_id: DiscIdSignal::Absent { track_count: 0 },
                barcode: BarcodeSignal::Absent,
                text: TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                durations: crate::import::probe::SourceDurations::default(),
            },
            ArtworkScan::Absent,
        );
    });

    assert!(
        logs.contains("signals: SignalsUpdated broadcast had no subscribers"),
        "expected no-subscriber warning, got {logs:?}",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_pass_join_error_reports_why_there_is_no_pass() {
    let fast_pass = run_fast_pass_blocking(
        &tokio::runtime::Handle::current(),
        || -> Result<FastPass, crate::import::ImportError> {
            panic!("fast-pass blocking task panicked")
        },
    )
    .await;

    let Err(detail) = fast_pass else {
        panic!("a fast-pass JoinError is reported, not swallowed");
    };
    assert!(
        detail.contains("fast-pass spawn_blocking failed"),
        "the detail names what died, got {detail:?}",
    );
}

/// An extraction that cannot gather its inputs does not go silent: it says so
/// with one snapshot that fails every signal, and nothing follows it. The run
/// it feeds settles on that as a failure instead of waiting on a snapshot that
/// is not coming.
#[tokio::test(flavor = "multi_thread")]
async fn an_aborted_extraction_fails_every_signal_in_one_snapshot() {
    let (tx, mut rx) = broadcast::channel(64);
    let (library_manager, _lib_tmp) = make_library_manager().await;
    let inner = ExtractionServiceInner {
        runtime_handle: tokio::runtime::Handle::current(),
        event_tx: tx,
        analyzer: std::sync::Mutex::new(None),
        library_manager,
        cancellation: CancellationRegistry::default(),
    };
    let generation = inner
        .cancellation
        .register("cand-1".to_string(), |_, generation| generation);
    let extraction = RunningExtraction {
        run: IdentifyRunId::for_test(1),
        key: "cand-1".to_string(),
        generation,
        priority: CallPriority::Interactive,
        snapshots: watch::channel(None).0,
    };

    let failure = LookupFailure::Diagnostic {
        detail: "fast-pass spawn_blocking failed: task panicked".to_string(),
    };
    emit_aborted_signals(
        &inner,
        &extraction,
        DiscIdSignal::Failed {
            failure: failure.clone(),
            track_count: 0,
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
        matches!(artwork, ArtworkScan::Failed { read: 0, total: 0, .. }),
        "the artwork pass failed before reading anything, got {artwork:?}"
    );
    assert_no_more_snapshots(&mut rx, "the aborted extraction's one snapshot").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ocr_join_error_aborts_without_settled_snapshot() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["cover.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(PanicAnalyzer);
    let (_handle, mut rx, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 2).await;
    assert!(matches!(signals[0].text, TextSignal::Scanning { .. }));

    match &signals[1].text {
        TextSignal::Failed { failure, .. } => {
            assert!(
                matches!(failure, LookupFailure::ArtworkAnalysis),
                "OCR JoinError must emit an artwork-analysis text failure, got {failure:?}",
            );
        }
        other => panic!("OCR JoinError must emit a failed text signal, got {other:?}"),
    }
    match &signals[1].barcode {
        BarcodeSignal::Failed { failure, .. } => {
            assert!(
                matches!(failure, LookupFailure::ArtworkAnalysis),
                "OCR JoinError must emit an artwork-analysis barcode failure, got {failure:?}",
            );
        }
        other => panic!("OCR JoinError must emit a failed barcode signal, got {other:?}"),
    }
}
