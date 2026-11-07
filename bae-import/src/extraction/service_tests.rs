//! The extraction service through its handle: what one pass over a folder
//! sends, the settled reading kept and reused, an extraction that cannot
//! finish, and one ended from outside. Fixtures the pass's own tests share.

use super::*;
use crate::extraction::{ArtworkAnalysis, ArtworkAnalyzer};
use crate::ScanEvent;
use bae_identify::signals::ArtworkScan;
use bae_util::test_channels::HANG_GUARD;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tempfile::TempDir;

/// Canned text lines keyed by file name; the optional gate holds each image
/// until the test opens it, so a test can act mid-OCR.
pub(super) struct StubAnalyzer {
    responses: StdMutex<HashMap<String, Vec<String>>>,
    gate: Option<crate::tests::gate::Held>,
    calls: std::sync::atomic::AtomicUsize,
}

impl StubAnalyzer {
    pub(super) fn new() -> Self {
        Self {
            responses: StdMutex::new(HashMap::new()),
            gate: None,
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(super) fn with(self, filename: &str, lines: Vec<String>) -> Self {
        self.responses.lock().unwrap().insert(filename.to_string(), lines);
        self
    }

    pub(super) fn gated(mut self, gate: crate::tests::gate::Held) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Lets a test assert a cancelled OCR pass stopped before every image.
    pub(super) fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl ArtworkAnalyzer for StubAnalyzer {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.pass();
        }
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        let text_lines = self.responses.lock().unwrap().get(&filename).cloned().unwrap_or_default();
        ArtworkAnalysis { barcodes: Vec::new(), text_lines }
    }
}

pub(super) struct PanicAnalyzer;

impl ArtworkAnalyzer for PanicAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        panic!("OCR analyzer panicked");
    }
}

/// Drain `SignalsUpdated` events until `expected` of them arrive.
pub(super) async fn collect_signals(rx: &mut crate::ImportEvents, expected: usize) -> Vec<Signals> {
    collect_snapshots(rx, expected).await.into_iter().map(|(signals, _)| signals).collect()
}

/// Wait until the run behind `watch` has ended, which drops its end.
pub(super) async fn run_ended(watch: &mut crate::extraction::ExtractionWatch) {
    tokio::time::timeout(HANG_GUARD, async { while watch.changed().await.is_ok() {} })
        .await
        .expect("the extraction ends");
}

/// No `SignalsUpdated` is left on `rx`: once the run has ended, the
/// extraction said everything it had to say.
pub(super) fn assert_no_more_snapshots(rx: &mut crate::ImportEvents, after: &str) {
    while let Ok(event) = rx.try_recv() {
        if let ImportEvent::SignalsUpdated { signals, .. } = event {
            panic!("no snapshot follows {after}, got {signals:?}")
        }
    }
}

/// Every snapshot with where the artwork pass was when it went out.
pub(super) async fn collect_snapshots(
    rx: &mut crate::ImportEvents,
    expected: usize,
) -> Vec<(Signals, ArtworkScan)> {
    let mut out = Vec::new();
    while out.len() < expected {
        let event = tokio::time::timeout(HANG_GUARD, rx.recv())
            .await
            .expect("timed out collecting events")
            .expect("channel closed");
        if let ImportEvent::SignalsUpdated { signals, artwork, .. } = event {
            out.push((signals, artwork));
        }
    }
    out
}

/// A throwaway library records over a temp dir, which must outlive it.
pub(super) async fn make_database() -> (bae_store::Store, TempDir) {
    let tmp = TempDir::new().unwrap();
    let db =
        bae_store::Store::new_test(crate::TEST_SCHEMA, tmp.path(), Arc::new(coven::SystemClock))
            .await
            .unwrap();
    (db, tmp)
}

/// Start a service; the bus sender comes back so a test can inject events.
pub(super) async fn make_service(
) -> (ExtractionService, ImportEventBus, crate::ImportEvents, TempDir) {
    let candidates = CandidateRuntime::for_test().await;
    let tx = ImportEventBus::new(candidates.clone());
    let rx = tx.every_event();
    let (db, lib_tmp) = make_database().await;
    let handle = ExtractionService::new(
        tokio::runtime::Handle::current(),
        tx.clone(),
        candidates,
        db.clone(),
        bae_records::test_fixtures::test_records(&db),
        bae_diagnostics::Diagnostics::noop(),
    );
    (handle, tx, rx, lib_tmp)
}

/// Start a service with `analyzer` registered and an extraction running over
/// `folder` as `"cand-1"`, with the watch its run holds open until it ends.
/// The `TempDir` must outlive the service.
pub(super) async fn start_signals(
    folder: PathBuf,
    analyzer: Arc<dyn ArtworkAnalyzer>,
) -> (ExtractionService, crate::ImportEvents, crate::extraction::ExtractionWatch, TempDir) {
    let (handle, _tx, rx, lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);
    let run = handle.start(
        IdentifyRunId::for_test(1),
        crate::CandidateKey::test("cand-1"),
        folder_source(folder),
    );
    (handle, rx, run, lib_tmp)
}

pub(super) fn fixture_flac() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/flac/01 Test Track 1.flac"))
        .expect("read FLAC fixture")
}

/// Just the JPEG magic — enough for `is_valid_image` to accept it.
pub(super) fn minimal_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
}

/// A release folder holding one FLAC plus the given images and documents.
pub(super) fn build_release(
    tmp: &TempDir,
    folder_name: &str,
    images: &[&str],
    documents: &[(&str, &str)],
) -> PathBuf {
    let folder = tmp.path().join(folder_name);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("01 - Track.flac"), fixture_flac()).unwrap();
    for img in images {
        fs::write(folder.join(img), minimal_jpeg()).unwrap();
    }
    for (name, content) in documents {
        fs::write(folder.join(name), content.as_bytes()).unwrap();
    }
    folder
}

pub(super) fn folder_source(folder: PathBuf) -> ExtractionSource {
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("test candidate scan");
    ExtractionSource::Candidate {
        candidate: crate::FolderCandidate {
            name: folder.file_name().unwrap().to_string_lossy().into_owned(),
            relative_folder: crate::RelativeFolder::root(),
            watched_folder_path: folder.clone(),
            file_root: folder.clone(),
            path: folder,
            files,
            scope: crate::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            grouping: None,
        },
    }
}

/// Start an extraction over `folder` as `"cand-1"` and return the watch its
/// run holds open until it ends.
pub(super) fn start(
    handle: &ExtractionService,
    folder: PathBuf,
) -> crate::extraction::ExtractionWatch {
    handle.start(
        IdentifyRunId::for_test(1),
        crate::CandidateKey::test("cand-1"),
        folder_source(folder),
    )
}

/// Wait until the gated analyzer has entered an image.
pub(super) async fn ocr_entered(entries: std::sync::mpsc::Receiver<()>) {
    tokio::task::spawn_blocking(move || entries.recv_timeout(HANG_GUARD))
        .await
        .unwrap()
        .expect("the OCR pass reaches an image");
}

/// Every `SignalsUpdated` delivered so far.
pub(super) fn delivered_signals(rx: &mut crate::ImportEvents) -> Vec<Signals> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|event| match event {
            ImportEvent::SignalsUpdated { signals, .. } => Some(signals),
            _ => None,
        })
        .collect()
}

pub(super) fn settled(signals: &Signals) -> bool {
    matches!(signals.text, TextSignal::Settled { .. })
}

#[tokio::test(flavor = "multi_thread")]
async fn no_artwork_emits_one_settled_snapshot() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Artist Name - Album Title", &[], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (_handle, mut rx, mut run, _lib_tmp) = start_signals(folder, analyzer).await;

    // Nothing is scanned, so the settled snapshot is the only one.
    let signals = collect_signals(&mut rx, 1).await;
    assert_eq!(signals.len(), 1);
    assert!(matches!(signals[0].text, TextSignal::Settled { .. }));

    // No artwork means no barcode source, so the signal is `Absent`.
    assert!(matches!(signals[0].barcode, BarcodeSignal::Absent));

    assert!(signals[0]
        .text
        .free_text()
        .iter()
        .any(|s| s.contains("Artist Name") || s.contains("Album Title")));
    run_ended(&mut run).await;
    assert_no_more_snapshots(&mut rx, "a folder with nothing to scan");
}

#[tokio::test(flavor = "multi_thread")]
async fn cue_fields_land_in_fast_pass() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();
    let cue = r#"PERFORMER "Artist Alpha"
TITLE "Album Title A"
FILE "audio.flac" WAVE
  TRACK 01 AUDIO
    PERFORMER "Artist Alpha"
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    // No artwork, so the settled snapshot is the only one.
    let signals = collect_signals(&mut rx, 1).await;
    let fast = signals[0].text.free_text();
    assert!(
        fast.contains(&"Artist Alpha".to_string()),
        "fast pass missing CUE PERFORMER, got {fast:?}",
    );
    assert!(
        fast.contains(&"Album Title A".to_string()),
        "fast pass missing CUE TITLE, got {fast:?}",
    );
    assert!(
        fast.contains(&"Track One".to_string()),
        "fast pass missing CUE track TITLE, got {fast:?}",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cue_catalog_becomes_barcode() {
    // A CUE `CATALOG` is the disc's UPC/EAN: a barcode, not a catalog number.
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();
    let cue = "CATALOG 0075678164521\n\
PERFORMER \"Artist Alpha\"\n\
TITLE \"Album Title A\"\n\
FILE \"audio.flac\" WAVE\n  \
  TRACK 01 AUDIO\n    \
    TITLE \"Track One\"\n    \
    INDEX 01 00:00:00\n";
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 1).await;
    let final_signals = &signals[signals.len() - 1];
    assert!(
        final_signals.barcode.codes().iter().any(|c| c.value == "0075678164521"),
        "CUE CATALOG should surface as a barcode code, got {:?}",
        final_signals.barcode,
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_all_zero_cue_catalog_is_not_a_barcode() {
    // An unfilled `CATALOG` of zeros is a placeholder, not a barcode.
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();
    let cue = "CATALOG 0000000000000\n\
PERFORMER \"Artist Alpha\"\n\
TITLE \"Album Title A\"\n\
FILE \"audio.flac\" WAVE\n  \
  TRACK 01 AUDIO\n    \
    TITLE \"Track One\"\n    \
    INDEX 01 00:00:00\n";
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 1).await;
    let final_signals = &signals[signals.len() - 1];
    assert!(
        final_signals.barcode.codes().is_empty(),
        "an all-zero CATALOG must not become a barcode, got {:?}",
        final_signals.barcode,
    );
}

/// A `CATALOG` whose check digit fails is not a code to look up.
#[tokio::test(flavor = "multi_thread")]
async fn a_cue_catalog_whose_check_digit_fails_is_not_a_barcode() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();
    let cue = "CATALOG 5012345678901\n\
FILE \"audio.flac\" WAVE\n  \
  TRACK 01 AUDIO\n    \
    INDEX 01 00:00:00\n";
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 1).await;
    assert_eq!(
        signals[0].barcode,
        BarcodeSignal::Absent,
        "no code was stated and there is no artwork to read"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn text_files_feed_free_text() {
    // A text-file line matching a path component clusters with it, which shows
    // the text file reached the pipeline.
    let tmp = TempDir::new().unwrap();
    let folder = build_release(
        &tmp,
        "Artist Alpha - Album Title B",
        &[],
        &[("info.txt", "Artist Alpha - Album Title B\nSome Other Thing\n")],
    );

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 1).await;
    let final_free_text = signals[signals.len() - 1].text.free_text();
    assert!(
        final_free_text.iter().any(|s| s == "Artist Alpha - Album Title B"),
        "expected path/text-file cluster to survive, got {final_free_text:?}",
    );
}

/// Without an analyzer, artwork is no barcode source: the signal is `Absent`,
/// not a scan that found none.
#[tokio::test(flavor = "multi_thread")]
async fn no_analyzer_leaves_artwork_absent_rather_than_scanned() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Album Title", &["Cover.jpg", "Back.jpg"], &[]);

    // No `register_analyzer` — this is Windows and Linux today.
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;

    let mut run = handle.start(
        IdentifyRunId::for_test(1),
        crate::CandidateKey::test("cand-1"),
        folder_source(folder),
    );

    // One settled snapshot: nothing to decode with, so nothing is read.
    let signals = collect_signals(&mut rx, 1).await;
    assert!(matches!(signals[0].text, TextSignal::Settled { .. }));
    assert_eq!(
        signals[0].barcode,
        BarcodeSignal::Absent,
        "artwork is not a barcode source without an analyzer",
    );
    run_ended(&mut run).await;
    assert_no_more_snapshots(&mut rx, "a scan that never ran");
}

/// A CUE `CATALOG` barcode needs no analyzer and settles without one.
#[tokio::test(flavor = "multi_thread")]
async fn no_analyzer_still_settles_cue_catalog_barcodes() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Album Title");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();
    fs::write(folder.join("Cover.jpg"), minimal_jpeg()).unwrap();
    let cue = "CATALOG 0075678164521\n\
FILE \"audio.flac\" WAVE\n  \
  TRACK 01 AUDIO\n    \
    TITLE \"Track One\"\n    \
    INDEX 01 00:00:00\n";
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.start(
        IdentifyRunId::for_test(1),
        crate::CandidateKey::test("cand-1"),
        folder_source(folder),
    );

    let signals = collect_signals(&mut rx, 1).await;
    let codes: Vec<&str> = signals[0].barcode.codes().iter().map(|c| c.value.as_str()).collect();
    assert_eq!(codes, vec!["0075678164521"]);
    assert!(
        matches!(signals[0].barcode, BarcodeSignal::Settled { .. }),
        "a CUE catalog barcode settles without an analyzer, got {:?}",
        signals[0].barcode,
    );
}

/// The settled reading is kept before it is announced, so a run started by
/// anything that heard the settle reuses it.
#[tokio::test(flavor = "multi_thread")]
async fn a_settled_reading_is_kept_before_it_is_announced() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Album Title [XX34b]", &["Cover.jpg"], &[]);
    let analyzer = Arc::new(StubAnalyzer::new().with("Cover.jpg", vec!["Line One".to_string()]));
    let (handle, tx, _rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);
    let source = folder_source(folder);
    let ExtractionSource::Candidate { candidate } = &source else {
        unreachable!("a folder source is a candidate");
    };
    let key = candidate.files.content_hash();
    tx.hold_send_where(|event| {
        matches!(
            event,
            ImportEvent::SignalsUpdated { signals, .. }
                if matches!(signals.text, TextSignal::Settled { .. })
        )
    });
    let mut run =
        handle.start(IdentifyRunId::for_test(1), crate::CandidateKey::test("cand-1"), source);

    tokio::time::timeout(HANG_GUARD, tx.send_held())
        .await
        .expect("the run announces its settled reading");
    let kept = handle.inner.settled.get_cloned(&key).is_some();
    tx.release_send();
    run_ended(&mut run).await;
    assert!(kept, "the settled reading is kept before it is announced");
}

/// A second run over an unchanged folder reuses the first run's settled
/// reading and reads no image again.
#[tokio::test(flavor = "multi_thread")]
async fn an_unchanged_folder_is_not_read_again() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Album Title [XX34b]", &["Cover.jpg"], &[]);
    let analyzer = Arc::new(StubAnalyzer::new().with("Cover.jpg", vec!["Line One".to_string()]));
    let (handle, mut rx, mut first_run, _lib_tmp) =
        start_signals(folder.clone(), analyzer.clone()).await;
    let first = collect_snapshots(&mut rx, 2).await;
    assert!(matches!(first[1].1, ArtworkScan::Done { total: 1 }));
    assert_eq!(analyzer.calls(), 1);

    let mut second_run = handle.start(
        IdentifyRunId::for_test(2),
        crate::CandidateKey::test("cand-1"),
        folder_source(folder),
    );
    let second = collect_snapshots(&mut rx, 1).await;
    assert_eq!(second[0].0, first[1].0);
    assert!(matches!(second[0].1, ArtworkScan::Done { total: 1 }));
    run_ended(&mut first_run).await;
    run_ended(&mut second_run).await;
    assert_no_more_snapshots(&mut rx, "the reused reading");
    assert_eq!(analyzer.calls(), 1, "no image is read again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fast_pass_that_panics_says_why_there_is_no_pass_and_is_reported() {
    let bugs = Arc::new(bae_diagnostics::RecordingBugReporter::default());
    let diagnostics = bae_diagnostics::Diagnostics::noop().reporting_bugs_to(bugs.clone());
    let fast_pass =
        run_fast_pass_blocking(&diagnostics, || -> Result<FastPass, crate::ImportError> {
            panic!("fast-pass blocking task panicked")
        })
        .await;

    let Err(detail) = fast_pass else {
        panic!("a fast pass that panics fails, not swallowed");
    };
    assert_eq!(detail, "read a folder's signals panicked: fast-pass blocking task panicked");
    assert_eq!(bugs.reports(), vec![("read a folder's signals".to_string(), detail)]);
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
    let (db, _lib_tmp) = make_database().await;
    let inner = ExtractionServiceInner {
        runtime_handle: tokio::runtime::Handle::current(),
        event_tx: tx,
        analyzer: std::sync::Mutex::new(None),
        records: bae_records::test_fixtures::test_records(&db),
        store: db,
        diagnostics: bae_diagnostics::Diagnostics::noop(),
        settled: SessionCache::new("Settled folder signals", SETTLED_CAPACITY),
        candidates,
        image_reads: Arc::new(tokio::sync::Semaphore::new(IMAGES_IN_FLIGHT)),
    };
    let generation = inner.candidates.start_work(
        CandidateWork::Extraction,
        crate::CandidateKey::test("cand-1"),
        |_, generation| generation,
    );
    let extraction = RunningExtraction {
        run: IdentifyRunId::for_test(1),
        key: crate::CandidateKey::test("cand-1"),
        generation,
        snapshots: watch::channel(None).0,
    };

    let failure = bae_catalogs::InternalFailure {
        detail: "read a folder's signals panicked: the reader broke".to_string(),
    };
    emit_aborted_signals(
        &inner,
        &extraction,
        DiscIdSignal::Failed { failure: failure.clone() },
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
    assert_no_more_snapshots(&mut rx, "the aborted extraction's one snapshot");
}

/// A `CandidateRemoved` sent mid-OCR cancels the run by the time the send
/// returns: it never settles, and it reads none of the images that were not
/// already being read when it was cancelled.
#[tokio::test]
async fn candidate_removed_event_cancels_in_flight_extraction() {
    let tmp = TempDir::new().unwrap();
    let images: Vec<String> = (1..=IMAGES_IN_FLIGHT + 2).map(|n| format!("p{n}.jpg")).collect();
    let image_names: Vec<&str> = images.iter().map(String::as_str).collect();
    let folder = build_release(&tmp, "Some Folder", &image_names, &[]);
    let (gate, held, entries) = crate::tests::gate::closed();
    let analyzer = Arc::new(
        image_names
            .iter()
            .fold(StubAnalyzer::new(), |analyzer, name| {
                analyzer.with(name, vec![format!("Line {name}")])
            })
            .gated(held),
    );
    let (handle, tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer.clone());
    let mut watch = start(&handle, folder);

    ocr_entered(entries).await;
    let cancelled = handle
        .cancelled_for_test(&crate::CandidateKey::test("cand-1"))
        .expect("the extraction is in flight");
    tx.send(ImportEvent::Scan(ScanEvent::CandidateRemoved {
        candidate_key: crate::CandidateKey::test("cand-1"),
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
    assert!(
        analyzer.calls() <= IMAGES_IN_FLIGHT,
        "cancel must stop the OCR pass before the images not yet started, read {}",
        analyzer.calls()
    );
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
    let (gate, held, entries) = crate::tests::gate::closed();
    let analyzer: Arc<dyn ArtworkAnalyzer> =
        Arc::new(StubAnalyzer::new().with("p1.jpg", vec!["Artist A".to_string()]).gated(held));
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
