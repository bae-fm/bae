use super::*;
use crate::signals::{ArtworkAnalysis, ArtworkAnalyzer, ArtworkScan, TextOrigin};
use crate::util::rate_limiter::CallPriority;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tempfile::TempDir;
use crate::import::ScanEvent;
use tokio::sync::mpsc::UnboundedReceiver;

mod aborts;
mod cancellation;
mod tags;

/// Canned text lines keyed by file name; the optional gate holds each image
/// until the test opens it, so a test can act mid-OCR.
struct StubAnalyzer {
    responses: StdMutex<HashMap<String, Vec<String>>>,
    gate: Option<crate::test_gate::Held>,
    calls: std::sync::atomic::AtomicUsize,
}

impl StubAnalyzer {
    fn new() -> Self {
        Self {
            responses: StdMutex::new(HashMap::new()),
            gate: None,
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn with(self, filename: &str, lines: Vec<String>) -> Self {
        self.responses
            .lock()
            .unwrap()
            .insert(filename.to_string(), lines);
        self
    }

    fn gated(mut self, gate: crate::test_gate::Held) -> Self {
        self.gate = Some(gate);
        self
    }

    /// Lets a test assert a cancelled OCR pass stopped before every image.
    fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl ArtworkAnalyzer for StubAnalyzer {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.pass();
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let text_lines = self
            .responses
            .lock()
            .unwrap()
            .get(&filename)
            .cloned()
            .unwrap_or_default();
        ArtworkAnalysis {
            barcodes: Vec::new(),
            text_lines,
        }
    }
}

struct PanicAnalyzer;

impl ArtworkAnalyzer for PanicAnalyzer {
    fn analyze(&self, _path: &Path) -> ArtworkAnalysis {
        panic!("OCR analyzer panicked");
    }
}

/// Drain `SignalsUpdated` events until `expected` of them arrive, or time out.
async fn collect_signals(
    rx: &mut UnboundedReceiver<ImportEvent>,
    expected: usize,
) -> Vec<Signals> {
    collect_snapshots(rx, expected)
        .await
        .into_iter()
        .map(|(signals, _)| signals)
        .collect()
}

/// Wait until the run behind `watch` has ended, which drops its end.
async fn run_ended(watch: &mut crate::signals::ExtractionWatch) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while watch.changed().await.is_ok() {}
    })
    .await
    .expect("the extraction ends");
}

/// No `SignalsUpdated` is left on `rx`: once the run has ended, the
/// extraction said everything it had to say.
fn assert_no_more_snapshots(rx: &mut UnboundedReceiver<ImportEvent>, after: &str) {
    while let Ok(event) = rx.try_recv() {
        if let ImportEvent::SignalsUpdated { signals, .. } = event {
            panic!("no snapshot follows {after}, got {signals:?}")
        }
    }
}

/// Every snapshot with where the artwork pass was when it went out.
async fn collect_snapshots(
    rx: &mut UnboundedReceiver<ImportEvent>,
    expected: usize,
) -> Vec<(Signals, ArtworkScan)> {
    let mut out = Vec::new();
    while out.len() < expected {
        let event = tokio::time::timeout(Duration::from_secs(30), rx.recv())
            .await
            .expect("timed out collecting events")
            .expect("channel closed");
        if let ImportEvent::SignalsUpdated {
            signals, artwork, ..
        } = event
        {
            out.push((signals, artwork));
        }
    }
    out
}

/// A throwaway `LibraryManager` over a temp dir, which must outlive it.
async fn make_library_manager() -> (crate::library::LibraryManager, TempDir) {
    let tmp = TempDir::new().unwrap();
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let database =
        crate::db::Database::new_test(tmp.path().join("test.db").to_str().unwrap(), clock.clone())
            .await
            .unwrap();
    let library_dir = coven::StoreDir::new(tmp.path());
    // Unique per test so keyring entries don't collide in the shared mock store.
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let config = crate::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    let config_handle = Arc::new(crate::config::ConfigHandle::new(config));
    crate::config::install_test_keyring();
    let manager = crate::library::LibraryManager::new(
        database,
        crate::config::AppDir::under_home(tmp.path()),
        config_handle,
        clock,
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(crate::util::http::Http::for_test()),
        crate::providers::Providers::offline(),
    );
    (manager, tmp)
}

/// Start a service; the bus sender comes back so a test can inject events.
async fn make_service() -> (
    ExtractionServiceHandle,
    ImportEventBus,
    UnboundedReceiver<ImportEvent>,
    TempDir,
) {
    let candidates = CandidateRuntime::default();
    let tx = ImportEventBus::new(candidates.clone());
    let rx = tx.every_event();
    let (library_manager, lib_tmp) = make_library_manager().await;
    let handle = ExtractionService::start(
        tokio::runtime::Handle::current(),
        tx.clone(),
        candidates,
        library_manager,
    );
    (handle, tx, rx, lib_tmp)
}

/// Start a service with `analyzer` registered and an extraction running over
/// `folder` as `"cand-1"`, with the watch its run holds open until it ends.
/// The `TempDir` must outlive the service.
async fn start_signals(
    folder: PathBuf,
    analyzer: Arc<dyn ArtworkAnalyzer>,
) -> (
    ExtractionServiceHandle,
    UnboundedReceiver<ImportEvent>,
    crate::signals::ExtractionWatch,
    TempDir,
) {
    let (handle, _tx, rx, lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);
    let run = handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );
    (handle, rx, run, lib_tmp)
}

fn fixture_flac() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/flac/01 Test Track 1.flac"
    ))
    .expect("read FLAC fixture")
}

/// Just the JPEG magic — enough for `is_valid_image` to accept it.
fn minimal_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
}

/// A release folder holding one FLAC plus the given images and documents.
fn build_release(
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

fn folder_source(folder: PathBuf) -> ExtractionSource {
    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("test candidate scan");
    ExtractionSource::Candidate {
        candidate: crate::import::FolderCandidate {
            name: folder.file_name().unwrap().to_string_lossy().into_owned(),
            display_path: folder.file_name().unwrap().to_string_lossy().into_owned(),
            watched_folder_path: folder.to_string_lossy().into_owned(),
            file_root: folder.clone(),
            path: folder,
            files,
            scope: crate::import::ReleaseFileScope::Recursive,
            file_edit_revision: 0,
            grouping: None,
        },
    }
}

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
            &ArtworkScan::Reading {
                current: Some("Back.jpg".to_string()),
                position: 1,
                total: 2,
            },
            &ArtworkScan::Reading {
                current: Some("Cover.jpg".to_string()),
                position: 2,
                total: 2,
            },
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
        signals[0]
            .text
            .catalogs()
            .iter()
            .any(|c| c == "XX34b"),
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
    assert!(signals[1]
        .text
        .catalogs()
        .iter()
        .any(|c| c == "XX34b"));

    assert!(
        matches!(signals[2].text, TextSignal::Settled { .. }),
        "final text should be Settled, got {:?}",
        signals[2].text,
    );
    assert!(signals[2]
        .text
        .catalogs()
        .iter()
        .any(|c| c == "XX34b"));
    assert!(signals[2]
        .text
        .catalogs()
        .iter()
        .any(|c| c == "WPCR-80001"));
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
        final_signals
            .barcode
            .codes()
            .iter()
            .any(|c| c.value == "0075678164521"),
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
        settled
            .text_pool
            .iter()
            .any(|line| line.text == "Artist Alpha"),
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
        [SourcedValue::in_file(
            "5012345678900".to_string(),
            "Back.jpg".to_string()
        )]
    );
}

#[test]
fn non_utf8_cue_is_decoded_not_dropped() {
    // A Windows-1252 CUE's curly apostrophe (0x92) is decoded, not the sheet dropped.
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();

    let mut cue: Vec<u8> = Vec::new();
    cue.extend_from_slice(b"PERFORMER \"Artist Alpha\"\n");
    cue.extend_from_slice(b"TITLE \"Album Title A\"\n");
    cue.extend_from_slice(b"FILE \"audio.flac\" WAVE\n");
    cue.extend_from_slice(b"  TRACK 01 AUDIO\n");
    cue.extend_from_slice(b"    TITLE \"I Ain");
    cue.push(0x92); // Windows-1252 right single quotation mark
    cue.extend_from_slice(b"t Got No Heart\"\n");
    cue.extend_from_slice(b"    INDEX 01 00:00:00\n");
    fs::write(folder.join("Album.cue"), &cue).unwrap();

    let files = crate::import::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::import::ReleaseFileScope::Recursive,
        &crate::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    let pass = gather_non_ocr_sources(&[folder], &files)
        .expect("scanned fixture audio has complete timing");
    let texts: Vec<&str> = pass.lines.iter().map(|l| l.text.as_str()).collect();

    assert!(
        texts.contains(&"Artist Alpha"),
        "non-UTF-8 CUE dropped entirely; got {texts:?}",
    );
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("I Ain") && t.ends_with("t Got No Heart")),
        "non-ASCII track title not recovered; got {texts:?}",
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
        &[(
            "info.txt",
            "Artist Alpha - Album Title B\nSome Other Thing\n",
        )],
    );

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 1).await;
    let final_free_text = signals[signals.len() - 1].text.free_text();
    assert!(
        final_free_text
            .iter()
            .any(|s| s == "Artist Alpha - Album Title B"),
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
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
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
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    let signals = collect_signals(&mut rx, 1).await;
    let codes: Vec<&str> = signals[0]
        .barcode
        .codes()
        .iter()
        .map(|c| c.value.as_str())
        .collect();
    assert_eq!(codes, vec!["0075678164521"]);
    assert!(
        matches!(signals[0].barcode, BarcodeSignal::Settled { .. }),
        "a CUE catalog barcode settles without an analyzer, got {:?}",
        signals[0].barcode,
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
        &[("info.txt", "Atlantic Records, Inc.\n")],
    );
    let cue = r#"PERFORMER "Artist Alpha"
FILE "01 - Track.flac" WAVE
  TRACK 01 AUDIO
    INDEX 01 00:00:00
"#;
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new().with(
        "Artist Alpha - Back Cover.jpg",
        vec!["Made in US · 1976".to_string()],
    ));
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
        found("Atlantic Records, Inc.").unwrap_or_else(|| panic!("the .txt line; got {pool:?}"));
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
    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new().with(
        "front.jpg",
        vec![
            "Atlantic Records".to_string(),
            "Atlantic Records".to_string(),
        ],
    ));
    let (_handle, mut rx, _run, _lib_tmp) = start_signals(folder, analyzer).await;

    let signals = collect_signals(&mut rx, 2).await;
    let pool = &signals[signals.len() - 1].text_pool;
    assert_eq!(
        pool.iter()
            .filter(|line| line.text == "Atlantic Records")
            .count(),
        1,
        "got {pool:?}",
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
    let mut run = handle.start(
        IdentifyRunId::for_test(1),
        "cand-1".to_string(),
        source,
        CallPriority::Interactive,
    );

    tokio::time::timeout(Duration::from_secs(30), tx.send_held())
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
    let (handle, mut rx, mut first_run, _lib_tmp) = start_signals(folder.clone(), analyzer.clone()).await;
    let first = collect_snapshots(&mut rx, 2).await;
    assert!(matches!(first[1].1, ArtworkScan::Done { total: 1 }));
    assert_eq!(analyzer.calls(), 1);

    let mut second_run = handle.start(
        IdentifyRunId::for_test(2),
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );
    let second = collect_snapshots(&mut rx, 1).await;
    assert_eq!(second[0].0, first[1].0);
    assert!(matches!(second[0].1, ArtworkScan::Done { total: 1 }));
    run_ended(&mut first_run).await;
    run_ended(&mut second_run).await;
    assert_no_more_snapshots(&mut rx, "the reused reading");
    assert_eq!(analyzer.calls(), 1, "no image is read again");
}
