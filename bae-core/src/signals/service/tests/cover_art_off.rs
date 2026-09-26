//! A run that leaves the cover art unread: no image reaches the analyzer, and
//! the pass says how many it left unread rather than that there were none.

use super::*;

fn without_cover_art() -> crate::config::IdentificationSteps {
    let mut steps = crate::config::IdentificationSteps::default();
    steps.set(crate::config::IdentificationStep::ReadCoverArt, false);
    steps
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_that_leaves_the_cover_art_unread_reads_no_image() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Album [XX34b]", &["Cover.jpg", "Back.jpg"], &[]);
    let analyzer = Arc::new(StubAnalyzer::new().with("Cover.jpg", vec!["WPCR-80001".to_string()]));
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer.clone());

    handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(folder.clone()),
        CallPriority::Interactive,
        without_cover_art(),
    );

    let snapshots = collect_snapshots(&mut rx, 1).await;
    let (signals, artwork) = &snapshots[0];
    assert_eq!(artwork, &ArtworkScan::Off { total: 2 });
    assert_eq!(
        signals.barcode,
        BarcodeSignal::Absent,
        "nothing was read a code off, which is not a read that found none"
    );
    assert!(
        matches!(signals.text, TextSignal::Settled { .. }),
        "the folder's other text settles at once: {:?}",
        signals.text
    );
    assert!(
        signals.text.catalogs().iter().any(|c| c.value == "XX34b"),
        "the folder's own text is still read: {:?}",
        signals.text.catalogs()
    );
    assert_no_more_snapshots(&mut rx, "the only snapshot").await;
    assert_eq!(analyzer.calls(), 0, "no image reached the analyzer");

    // The same files under a run that reads the art are read afresh: the
    // reading taken without it answers nothing about what the art says.
    handle.start(
        IdentifyRunId::for_test(2),
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
        crate::config::IdentificationSteps::default(),
    );
    let snapshots = collect_snapshots(&mut rx, 3).await;
    assert_eq!(snapshots[2].1, ArtworkScan::Done { total: 2 });
    assert_eq!(analyzer.calls(), 2, "both images are read this time");
}
