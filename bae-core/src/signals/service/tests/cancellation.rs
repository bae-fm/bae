//! An extraction ended from outside: its candidate removed, or a later start
//! for the same key replacing it.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn cancelled_ocr_run_does_not_settle() {
    // A cancelled run tears down without reaching its final `Settled` snapshot: three
    // delayed images, cancelled mid-OCR, must emit no `Settled` for that key.
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg", "p2.jpg", "p3.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .with("p2.jpg", vec!["Artist B".to_string()])
            .with("p3.jpg", vec!["Artist C".to_string()])
            .with_delay(Duration::from_millis(100)),
    );
    let (handle, mut rx, _lib_tmp) = start_signals(folder, analyzer).await;

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.cancel("cand-1");

    loop {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(ImportEvent::SignalsUpdated { signals, .. })) => {
                assert!(
                    !matches!(signals.text, TextSignal::Settled { .. }),
                    "cancelled OCR run must not emit a Settled snapshot, got {:?}",
                    signals.text,
                );
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn candidate_removed_event_cancels_in_flight_extraction() {
    // Three images at 200ms of OCR each, with a `CandidateRemoved` landing during the
    // first. The service's bus listener cancels the run, so it never settles and stops
    // short of analyzing every image.
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg", "p2.jpg", "p3.jpg"], &[]);
    let analyzer = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Line A".to_string()])
            .with("p2.jpg", vec!["Line B".to_string()])
            .with("p3.jpg", vec!["Line C".to_string()])
            .with_delay(Duration::from_millis(200)),
    );
    let (handle, tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer.clone());

    handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    tx.send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: "cand-1".to_string(),
    }))
    .unwrap();

    loop {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(ImportEvent::SignalsUpdated { signals, .. })) => {
                assert!(
                    !matches!(signals.text, TextSignal::Settled { .. }),
                    "extraction for a removed candidate must not settle, got {:?}",
                    signals.text,
                );
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }
    // The token is checked between images, so the pass stops before the third.
    assert!(analyzer.calls() < 3, "cancel must stop the OCR pass early");
}

#[tokio::test(flavor = "multi_thread")]
async fn restart_for_same_key_cancels_prior_then_starts_fresh() {
    // The second `start` for a key cancels the first. Only a completed run settles, so
    // seeing a `Settled` snapshot proves the surviving run finished — and that the
    // generation-guarded teardown neither deadlocked nor panicked.
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .with_delay(Duration::from_millis(100)),
    );
    let (handle, mut rx, _lib_tmp) = start_signals(folder.clone(), analyzer).await;
    handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    let mut saw_settled = false;
    loop {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(ImportEvent::SignalsUpdated { signals, .. })) => {
                if matches!(signals.text, TextSignal::Settled { .. }) {
                    saw_settled = true;
                }
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }

    assert!(
        saw_settled,
        "expected a Settled snapshot from the completed run",
    );
}

/// Three consecutive `start`s for one key. Without per-task generations, the first
/// task's teardown could fire *after* the second `start` inserted its token and
/// remove it, leaving the third `start` nothing to cancel.
///
/// OCR is held long enough that the first task is still alive at the third `start`. A
/// cancelled run emits no final snapshot, so they can't be counted directly; instead
/// the completed run must settle, and the `(generation, token)` guard must neither
/// deadlock nor panic under the interleaving.
#[tokio::test(flavor = "multi_thread")]
async fn three_starts_cancel_each_predecessor() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .with_delay(Duration::from_millis(200)),
    );
    let (handle, mut rx, _lib_tmp) = start_signals(folder.clone(), analyzer).await;
    tokio::time::sleep(Duration::from_millis(40)).await;
    handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(folder.clone()),
        CallPriority::Interactive,
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    let mut saw_settled = false;
    loop {
        match tokio::time::timeout(Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(ImportEvent::SignalsUpdated { signals, .. })) => {
                if matches!(signals.text, TextSignal::Settled { .. }) {
                    saw_settled = true;
                }
            }
            Ok(Ok(_)) => continue,
            _ => break,
        }
    }

    assert!(
        saw_settled,
        "expected a Settled snapshot from the completed run",
    );
}
