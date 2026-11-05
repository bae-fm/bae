//! The streamed pass over the artwork: a snapshot per image read, in the
//! images' order whatever order the reads finish in, no more than
//! `IMAGES_IN_FLIGHT` reads at once across every pass, and a pass that a
//! failed read or a cancel stops.

use super::*;
use crate::extraction::service::tests::*;
use crate::extraction::service::*;
use crate::extraction::{ArtworkAnalysis, ArtworkAnalyzer};
use bae_catalogs::rate_limiter::CallPriority;
use bae_identify::signals::{ArtworkScan, TextOrigin};
use bae_util::test_channels::HANG_GUARD;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn emits_fast_pass_then_ocr_then_settled() {
    // The folder name carries a catalog bracket, the parent an artist name.
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("Artist Name");
    fs::create_dir_all(&parent).unwrap();
    let folder = parent.join("1989 - Album Title [XX34b]");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("01 - Track.flac"), fixture_flac()).unwrap();
    fs::write(folder.join("Cover.jpg"), minimal_jpeg()).unwrap();
    fs::write(folder.join("Back.jpg"), minimal_jpeg()).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("Cover.jpg", vec!["WPCR-80001".to_string()])
            .with("Back.jpg", vec!["Extra Line".to_string()]),
    );
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder.clone(), analyzer).await;

    // The fast pass, one per image but the last, then the settled snapshot.
    let snapshots = collect_snapshots(&mut rx, 3).await;
    assert_eq!(snapshots.len(), 3);
    let artwork: Vec<&ArtworkScan> = snapshots.iter().map(|(_, a)| a).collect();
    assert_eq!(
        artwork,
        vec![
            &ArtworkScan::Reading { current: Some("Back.jpg".to_string()), position: 1, total: 2 },
            &ArtworkScan::Reading { current: Some("Cover.jpg".to_string()), position: 2, total: 2 },
            &ArtworkScan::Done { total: 2 },
        ]
    );
    let signals: Vec<Signals> = snapshots.into_iter().map(|(s, _)| s).collect();

    // While scanning, the bracket is a catalog and the path components are free text.
    assert!(
        matches!(signals[0].text, TextSignal::Scanning { .. }),
        "fast-pass text should be Scanning, got {:?}",
        signals[0].text,
    );
    assert!(
        signals[0].text.catalogs().iter().any(|c| c == "XX34b"),
        "expected folder-bracket catalog in fast pass, got {:?}",
        signals[0].text.catalogs(),
    );
    assert!(
        signals[0]
            .text
            .free_text()
            .iter()
            .any(|s| s.contains("Artist Name") || s.contains("Album Title")),
        "expected folder/parent path components in fast pass, got {:?}",
        signals[0].text.free_text(),
    );

    // Artwork is OCR'd in sorted order — Back.jpg, then Cover.jpg.
    assert!(signals[1].text.catalogs().iter().any(|c| c == "XX34b"));

    assert!(
        matches!(signals[2].text, TextSignal::Settled { .. }),
        "final text should be Settled, got {:?}",
        signals[2].text,
    );
    assert!(signals[2].text.catalogs().iter().any(|c| c == "XX34b"));
    assert!(signals[2].text.catalogs().iter().any(|c| c == "WPCR-80001"));
}

/// The digits printed under a back cover's bars are the code, read off that
/// image and spelled as the CUE sheet's; the image's other lines still reach
/// the text pool.
#[tokio::test(flavor = "multi_thread")]
async fn a_barcode_printed_as_text_on_the_artwork_is_a_barcode_sighting() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();
    fs::write(folder.join("Back.jpg"), minimal_jpeg()).unwrap();
    let cue = "CATALOG 0012345678905\n\
FILE \"audio.flac\" WAVE\n  \
  TRACK 01 AUDIO\n    \
    INDEX 01 00:00:00\n";
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new().with(
        "Back.jpg",
        vec![
            "Artist Alpha".to_string(),
            "0 12345 67890 5".to_string(),
            "5 012345 678901".to_string(),
        ],
    ));
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    // The fast pass, then the settled snapshot after the one image.
    let signals = collect_signals(&mut rx, 2).await;
    let settled = &signals[1];
    assert!(matches!(settled.barcode, BarcodeSignal::Settled { .. }));
    assert_eq!(
        settled.barcode.codes(),
        [
            SourcedValue::in_file("0012345678905".to_string(), "Album.cue".to_string()),
            SourcedValue::in_file("0012345678905".to_string(), "Back.jpg".to_string()),
        ]
    );
    assert!(
        settled.text_pool.iter().any(|line| line.text == "Artist Alpha"),
        "the image's text still reaches the pool, got {:?}",
        settled.text_pool,
    );
}

/// Where the detector decodes the bars and the recognizer reads the digits
/// under them, the image holds one sighting of the code.
#[tokio::test(flavor = "multi_thread")]
async fn the_bars_and_their_printed_digits_are_one_sighting() {
    struct BarsAndDigits;
    impl ArtworkAnalyzer for BarsAndDigits {
        fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
            ArtworkAnalysis {
                barcodes: vec!["5012345678900".to_string()],
                text_lines: vec!["5 012345 678900".to_string()],
            }
        }
    }

    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();
    fs::write(folder.join("Back.jpg"), minimal_jpeg()).unwrap();

    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, Arc::new(BarsAndDigits)).await;

    let signals = collect_signals(&mut rx, 2).await;
    assert_eq!(
        signals[1].barcode.codes(),
        [SourcedValue::in_file("5012345678900".to_string(), "Back.jpg".to_string())]
    );
}

/// Every surface a folder carries puts its lines in the pool as read, each with
/// the surface it came from, and nothing stripped.
#[tokio::test(flavor = "multi_thread")]
async fn every_surface_lands_in_the_text_pool_as_it_was_read() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(
        &tmp,
        "Artist Alpha - Album Title [16033-2]",
        &["Artist Alpha - Back Cover.jpg"],
        &[("info.txt", "Label One Records, Inc.\n")],
    );
    let cue = r#"PERFORMER "Artist Alpha"
FILE "01 - Track.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("Artist Alpha - Back Cover.jpg", vec!["Made in US · 1976".to_string()]),
    );
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 2).await;
    let pool = &signals[signals.len() - 1].text_pool;
    let found = |text: &str| pool.iter().find(|line| line.text == text);

    let folder_line = found("Artist Alpha - Album Title [16033-2]")
        .unwrap_or_else(|| panic!("the folder's own name, as written; got {pool:?}"));
    assert_eq!(folder_line.origin, TextOrigin::FolderName);

    let filename_line = found("Artist Alpha - Back Cover")
        .unwrap_or_else(|| panic!("the image's file name; got {pool:?}"));
    assert_eq!(filename_line.origin, TextOrigin::Filename);

    let cue_line =
        found("Artist Alpha").unwrap_or_else(|| panic!("the CUE PERFORMER; got {pool:?}"));
    assert_eq!(cue_line.origin, TextOrigin::CueSheet);

    let text_file_line =
        found("Label One Records, Inc.").unwrap_or_else(|| panic!("the .txt line; got {pool:?}"));
    assert_eq!(text_file_line.origin, TextOrigin::TextFile);

    let ocr_line =
        found("Made in US · 1976").unwrap_or_else(|| panic!("the OCR line; got {pool:?}"));
    assert_eq!(ocr_line.origin, TextOrigin::Artwork);
}

/// One line read twice off one surface is pooled once.
#[tokio::test(flavor = "multi_thread")]
async fn a_line_read_twice_off_one_surface_is_pooled_once() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["front.jpg"], &[]);
    let analyzer: Arc<dyn ArtworkAnalyzer> =
        Arc::new(StubAnalyzer::new().with(
            "front.jpg",
            vec!["Label One Records".to_string(), "Label One Records".to_string()],
        ));
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 2).await;
    let pool = &signals[signals.len() - 1].text_pool;
    assert_eq!(
        pool.iter().filter(|line| line.text == "Label One Records").count(),
        1,
        "got {pool:?}",
    );
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

/// A run cancelled mid-OCR never reaches its `Settled` snapshot.
#[tokio::test(flavor = "multi_thread")]
async fn cancelled_ocr_run_does_not_settle() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg", "p2.jpg", "p3.jpg"], &[]);
    let (gate, held, entries) = crate::tests::gate::closed();
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
    handle.cancel(&crate::CandidateKey::test("cand-1"));
    gate.open();
    run_ended(&mut watch).await;

    let settled: Vec<Signals> = delivered_signals(&mut rx).into_iter().filter(settled).collect();
    assert!(
        settled.is_empty(),
        "cancelled OCR run must not emit a Settled snapshot, got {settled:?}"
    );
}

/// A pass waiting for a read permit, while other passes' reads hold every
/// one, reads nothing once it is cancelled, even when a permit frees at the
/// same moment. The runtime runs one task at a time, so the pass is parked
/// waiting when the test cancels it and frees the permits, and finds both at
/// once when it next runs; each attempt is that moment again.
#[tokio::test]
async fn a_cancelled_pass_waiting_for_a_read_starts_none() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);
    let analyzer = Arc::new(StubAnalyzer::new().with("p1.jpg", vec!["Line".to_string()]));
    let (handle, _tx, _rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer.clone());
    for attempt in 0..32 {
        let reads_elsewhere = handle
            .inner
            .image_reads
            .clone()
            .acquire_many_owned(IMAGES_IN_FLIGHT as u32)
            .await
            .unwrap();
        let key = crate::CandidateKey::test(&format!("cand-{attempt}"));
        let mut watch = handle.start(
            IdentifyRunId::for_test(1),
            key.clone(),
            folder_source(folder.clone()),
            CallPriority::Interactive,
        );
        // The pass sends its first snapshot just before it waits for a permit,
        // and runs on until it waits before the test runs again.
        tokio::time::timeout(HANG_GUARD, watch.wait_for(|snapshot| snapshot.is_some()))
            .await
            .expect("the pass reaches its images")
            .expect("the pass is running");

        handle.cancel(&key);
        drop(reads_elsewhere);
        run_ended(&mut watch).await;

        assert_eq!(analyzer.calls(), 0, "attempt {attempt}: the cancelled pass started a read");
    }
}

/// Answers each image only once every image before it in `finish_order` has
/// been answered, so a test fixes the order the reads finish in. The image
/// named `panics_on` panics on its turn instead of answering.
struct OrderedAnalyzer {
    finish_order: Vec<String>,
    panics_on: Option<String>,
    answered: StdMutex<usize>,
    turn_changed: Condvar,
}

impl OrderedAnalyzer {
    fn new(finish_order: Vec<String>) -> Self {
        Self {
            finish_order,
            panics_on: None,
            answered: StdMutex::new(0),
            turn_changed: Condvar::new(),
        }
    }

    fn panicking_on(mut self, name: &str) -> Self {
        self.panics_on = Some(name.to_string());
        self
    }
}

impl ArtworkAnalyzer for OrderedAnalyzer {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let turn = self
            .finish_order
            .iter()
            .position(|finishes| *finishes == name)
            .expect("every image has a turn");
        let answered = self
            .turn_changed
            .wait_timeout_while(self.answered.lock().unwrap(), HANG_GUARD, |answered| {
                *answered < turn
            })
            .unwrap();
        assert!(!answered.1.timed_out(), "{name} waited for its turn");
        let mut answered = answered.0;
        *answered += 1;
        drop(answered);
        self.turn_changed.notify_all();
        if self.panics_on.as_deref() == Some(name.as_str()) {
            panic!("OCR analyzer panicked on {name}");
        }
        image_analysis(&name)
    }
}

/// What each image reads: a line naming it, and on `p3.jpg` a barcode, so
/// both the text pool and the barcode sightings depend on the order images
/// are taken in.
fn image_analysis(name: &str) -> ArtworkAnalysis {
    ArtworkAnalysis {
        barcodes: if name == "p3.jpg" { vec!["5012345678900".to_string()] } else { Vec::new() },
        text_lines: vec![format!("Line read off {name}"), "Catalog CAT-1001".to_string()],
    }
}

/// `count` image names, in the order the folder lists them.
fn image_names(count: usize) -> Vec<String> {
    (1..=count).map(|n| format!("p{n}.jpg")).collect()
}

fn folder_with(tmp: &TempDir, images: &[String]) -> PathBuf {
    let names: Vec<&str> = images.iter().map(String::as_str).collect();
    build_release(tmp, "Album Title [XX34b]", &names, &[])
}

/// Every snapshot of one pass over `folder` with `analyzer`, through its end.
async fn pass_snapshots(
    folder: PathBuf,
    analyzer: Arc<dyn ArtworkAnalyzer>,
    expected: usize,
) -> Vec<(Signals, ArtworkScan)> {
    let (_handle, mut rx, mut run, _lib_tmp) = start_signals(folder, analyzer).await;
    let snapshots = collect_snapshots(&mut rx, expected).await;
    run_ended(&mut run).await;
    assert_no_more_snapshots(&mut rx, "the pass's last snapshot");
    snapshots
}

/// Reads that finish last image first send the snapshots, progress and
/// settled signals that reads finishing in the images' order send.
#[tokio::test(flavor = "multi_thread")]
async fn reads_finishing_out_of_order_send_what_reads_in_order_send() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT);
    let folder = folder_with(&tmp, &images);
    // The fast pass, one per image but the last, then the settled snapshot.
    let expected = images.len() + 1;

    let in_order =
        pass_snapshots(folder.clone(), Arc::new(OrderedAnalyzer::new(images.clone())), expected)
            .await;
    let reversed: Vec<String> = images.iter().rev().cloned().collect();
    let last_first =
        pass_snapshots(folder, Arc::new(OrderedAnalyzer::new(reversed)), expected).await;

    assert_eq!(last_first, in_order);
    let progress: Vec<&ArtworkScan> = in_order.iter().map(|(_, artwork)| artwork).collect();
    let mut reading: Vec<ArtworkScan> = images
        .iter()
        .enumerate()
        .map(|(index, name)| ArtworkScan::Reading {
            current: Some(name.clone()),
            position: index as u32 + 1,
            total: images.len() as u32,
        })
        .collect();
    reading.push(ArtworkScan::Done { total: images.len() as u32 });
    assert_eq!(progress, reading.iter().collect::<Vec<_>>());
    let settled = &in_order[images.len()].0;
    let artwork_lines: Vec<&str> = settled
        .text_pool
        .iter()
        .filter(|line| line.text.starts_with("Line read off"))
        .map(|line| line.text.as_str())
        .collect();
    let in_image_order: Vec<String> =
        images.iter().map(|name| format!("Line read off {name}")).collect();
    assert_eq!(artwork_lines, in_image_order);
    assert_eq!(
        settled.barcode,
        BarcodeSignal::Settled {
            codes: vec![SourcedValue::in_file("5012345678900".to_string(), "p3.jpg".to_string())]
        }
    );
}

/// Counts the reads under way at once, each held until the test opens the
/// gate.
struct ConcurrencyAnalyzer {
    reading: AtomicUsize,
    most_at_once: AtomicUsize,
    calls: AtomicUsize,
    gate: crate::tests::gate::Held,
}

impl ArtworkAnalyzer for ConcurrencyAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let reading = self.reading.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_at_once.fetch_max(reading, Ordering::SeqCst);
        self.gate.pass();
        self.reading.fetch_sub(1, Ordering::SeqCst);
        ArtworkAnalysis::empty()
    }
}

/// Wait until `IMAGES_IN_FLIGHT` reads have entered the held analyzer, and
/// check that no other read starts while they are held.
async fn bound_reached_and_held(
    entries: std::sync::mpsc::Receiver<()>,
) -> std::sync::mpsc::Receiver<()> {
    tokio::task::spawn_blocking(move || {
        for _ in 0..IMAGES_IN_FLIGHT {
            entries.recv_timeout(HANG_GUARD).expect("the bound's worth of images are read at once");
        }
        assert!(
            entries.recv_timeout(Duration::from_millis(200)).is_err(),
            "no image starts while the bound's worth are being read"
        );
        entries
    })
    .await
    .unwrap()
}

fn concurrency_analyzer(gate: crate::tests::gate::Held) -> Arc<ConcurrencyAnalyzer> {
    Arc::new(ConcurrencyAnalyzer {
        reading: AtomicUsize::new(0),
        most_at_once: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
        gate,
    })
}

/// `IMAGES_IN_FLIGHT` images are read at once, and no more: while they are
/// all held no other image starts.
#[tokio::test(flavor = "multi_thread")]
async fn no_more_than_the_bound_are_read_at_once() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT * 3);
    let folder = folder_with(&tmp, &images);
    let (gate, held, entries) = crate::tests::gate::closed();
    let analyzer = concurrency_analyzer(held);
    let (_handle, mut rx, mut run, _lib_tmp) = start_signals(folder, analyzer.clone()).await;

    let entries = bound_reached_and_held(entries).await;
    gate.open();
    let snapshots = collect_snapshots(&mut rx, images.len() + 1).await;
    run_ended(&mut run).await;
    drop(entries);

    assert_eq!(
        snapshots.last().map(|(_, artwork)| artwork),
        Some(&ArtworkScan::Done { total: images.len() as u32 })
    );
    assert_eq!(analyzer.calls.load(Ordering::SeqCst), images.len());
    assert_eq!(analyzer.most_at_once.load(Ordering::SeqCst), IMAGES_IN_FLIGHT);
}

/// Two passes running at once share the bound: between them no more than
/// `IMAGES_IN_FLIGHT` images are read at once, and both settle.
#[tokio::test(flavor = "multi_thread")]
async fn passes_running_at_once_share_the_bound() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT * 2);
    let names: Vec<&str> = images.iter().map(String::as_str).collect();
    let first = build_release(&tmp, "Album Title A", &names, &[]);
    let second = build_release(&tmp, "Album Title B", &names, &[]);
    let (gate, held, entries) = crate::tests::gate::closed();
    let analyzer = concurrency_analyzer(held);
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer.clone());
    let mut first_run = handle.start(
        IdentifyRunId::for_test(1),
        crate::CandidateKey::test("cand-1"),
        folder_source(first),
        CallPriority::Interactive,
    );
    let mut second_run = handle.start(
        IdentifyRunId::for_test(2),
        crate::CandidateKey::test("cand-2"),
        folder_source(second),
        CallPriority::Interactive,
    );

    let entries = bound_reached_and_held(entries).await;
    gate.open();
    let snapshots = collect_snapshots(&mut rx, 2 * (images.len() + 1)).await;
    run_ended(&mut first_run).await;
    run_ended(&mut second_run).await;
    drop(entries);

    let done = ArtworkScan::Done { total: images.len() as u32 };
    assert_eq!(
        snapshots.iter().filter(|(_, artwork)| *artwork == done).count(),
        2,
        "both passes settle"
    );
    assert_eq!(analyzer.calls.load(Ordering::SeqCst), 2 * images.len());
    assert_eq!(analyzer.most_at_once.load(Ordering::SeqCst), IMAGES_IN_FLIGHT);
}

/// An image whose read dies fails the pass with what the images before it
/// read; the images after it that had already finished add nothing, and
/// nothing follows the failed snapshot.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_read_fails_the_pass_with_the_images_before_it() {
    let tmp = TempDir::new().unwrap();
    let images = image_names(IMAGES_IN_FLIGHT);
    let folder = folder_with(&tmp, &images);
    // p1 finishes, then the images after p2, then p2 panics.
    let finish_order = vec![
        "p1.jpg".to_string(),
        "p4.jpg".to_string(),
        "p3.jpg".to_string(),
        "p2.jpg".to_string(),
    ];
    let analyzer = Arc::new(OrderedAnalyzer::new(finish_order).panicking_on("p2.jpg"));

    // The fast pass, p1's snapshot, then the failed one.
    let snapshots = pass_snapshots(folder, analyzer, 3).await;

    let (after_p1, _) = &snapshots[1];
    let (failed, artwork) = &snapshots[2];
    let ArtworkScan::Failed { failure, read, total } = artwork else {
        panic!("the pass fails at p2, got {artwork:?}");
    };
    assert_eq!((*read, *total), (1, images.len() as u32));
    assert!(
        failure.detail.contains("read an image's text panicked")
            && failure.detail.contains("p2.jpg"),
        "the failure names what panicked and its image, got {failure:?}"
    );
    assert_eq!(
        failed.text,
        TextSignal::Failed {
            failure: failure.clone(),
            catalogs: after_p1.text.catalogs().to_vec(),
            free_text: after_p1.text.free_text().to_vec(),
        }
    );
    assert_eq!(
        failed.barcode,
        BarcodeSignal::Failed { failure: failure.clone(), codes: Vec::new() },
        "p3's barcode was read after the failure, so it is not a sighting"
    );
    assert_eq!(failed.text_pool, after_p1.text_pool);
    assert!(
        after_p1.text_pool.iter().any(|line| line.text == "Line read off p1.jpg"),
        "p1's reading is in, got {:?}",
        after_p1.text_pool
    );
}
