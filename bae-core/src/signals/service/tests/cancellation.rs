//! An extraction ended from outside: its candidate removed, or a later start
//! for the same key replacing it.

use super::*;

/// Start an extraction over `folder` as `"cand-1"` and return the watch its
/// run holds open until it ends.
fn start(handle: &ExtractionServiceHandle, folder: PathBuf) -> crate::signals::ExtractionWatch {
    handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    )
}

/// Wait until the gated analyzer has entered an image.
async fn ocr_entered(entries: std::sync::mpsc::Receiver<()>) {
    tokio::task::spawn_blocking(move || entries.recv_timeout(Duration::from_secs(30)))
        .await
        .unwrap()
        .expect("the OCR pass reaches an image");
}

/// Every `SignalsUpdated` delivered so far.
fn delivered_signals(rx: &mut UnboundedReceiver<ImportEvent>) -> Vec<Signals> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            ImportEvent::SignalsUpdated { signals, .. } => Some(signals),
            _ => None,
        })
        .collect()
}

fn settled(signals: &Signals) -> bool {
    matches!(signals.text, TextSignal::Settled { .. })
}

/// A run cancelled mid-OCR never reaches its `Settled` snapshot.
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_ocr_run_does_not_settle() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg", "p2.jpg", "p3.jpg"], &[]);
    let (gate, held, entries) = crate::test_gate::closed();
    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .with("p2.jpg", vec!["Artist B".to_string()])
            .with("p3.jpg", vec!["Artist C".to_string()])
            .gated(held),
    );
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);
    let mut watch = start(&handle, folder);

    ocr_entered(entries).await;
    handle.cancel("cand-1");
    gate.open();
    run_ended(&mut watch).await;

    let settled: Vec<Signals> = delivered_signals(&mut rx)
        .into_iter()
        .filter(settled)
        .collect();
    assert!(
        settled.is_empty(),
        "cancelled OCR run must not emit a Settled snapshot, got {settled:?}"
    );
}

/// A `CandidateRemoved` sent mid-OCR cancels the run by the time the send
/// returns: it never settles, and it stops short of analyzing every image,
/// since the token is checked between images.
#[tokio::test]
async fn candidate_removed_event_cancels_in_flight_extraction() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg", "p2.jpg", "p3.jpg"], &[]);
    let (gate, held, entries) = crate::test_gate::closed();
    let analyzer = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Line A".to_string()])
            .with("p2.jpg", vec!["Line B".to_string()])
            .with("p3.jpg", vec!["Line C".to_string()])
            .gated(held),
    );
    let (handle, tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer.clone());
    let mut watch = start(&handle, folder);

    ocr_entered(entries).await;
    let cancelled = handle
        .cancelled_for_test("cand-1")
        .expect("the extraction is in flight");
    tx.send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: "cand-1".to_string(),
    }));
    tokio::time::timeout(Duration::ZERO, cancelled)
        .await
        .expect("the removal cancels the extraction");
    gate.open();
    run_ended(&mut watch).await;

    assert!(
        !delivered_signals(&mut rx).iter().any(settled),
        "extraction for a removed candidate must not settle"
    );
    assert!(analyzer.calls() < 3, "cancel must stop the OCR pass early");
}

/// A second `start` for a key cancels the first, and the run it started
/// completes: only a completed run settles, so its `Settled` snapshot proves
/// the replacement neither deadlocked nor panicked.
#[tokio::test(flavor = "multi_thread")]
async fn restart_for_same_key_cancels_prior_then_starts_fresh() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);
    let analyzer: Arc<dyn ArtworkAnalyzer> =
        Arc::new(StubAnalyzer::new().with("p1.jpg", vec!["Artist A".to_string()]));
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);
    let _first = start(&handle, folder.clone());
    let mut second = start(&handle, folder);
    run_ended(&mut second).await;

    assert!(
        delivered_signals(&mut rx).iter().any(settled),
        "expected a Settled snapshot from the completed run"
    );
}

/// Three `start`s for one key while the first is still inside OCR: each
/// cancels its predecessor, and the per-task generation keeps a cancelled
/// task's teardown from removing a newer task's entry, so the last run still
/// settles.
#[tokio::test(flavor = "multi_thread")]
async fn three_starts_cancel_each_predecessor() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);
    let (gate, held, entries) = crate::test_gate::closed();
    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .gated(held),
    );
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);
    let _first = start(&handle, folder.clone());
    ocr_entered(entries).await;
    let _second = start(&handle, folder.clone());
    let mut third = start(&handle, folder);
    gate.open();
    run_ended(&mut third).await;

    assert!(
        delivered_signals(&mut rx).iter().any(settled),
        "expected a Settled snapshot from the completed run"
    );
}
