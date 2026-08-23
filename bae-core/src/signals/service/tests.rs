use super::*;
use crate::signals::{ArtworkAnalysis, ArtworkAnalyzer};
use crate::test_logs::capture_warn_logs;
use crate::util::rate_limiter::CallPriority;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tempfile::TempDir;

/// Returns canned text lines keyed by filename rather than full path, so a temp-dir
/// path can't break it. The optional delay is what lets a test cancel mid-OCR.
struct StubAnalyzer {
    responses: StdMutex<HashMap<String, Vec<String>>>,
    delay: Option<Duration>,
    calls: std::sync::atomic::AtomicUsize,
}

impl StubAnalyzer {
    fn new() -> Self {
        Self {
            responses: StdMutex::new(HashMap::new()),
            delay: None,
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

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
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
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
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
    rx: &mut broadcast::Receiver<ImportEvent>,
    expected: usize,
) -> Vec<Signals> {
    let mut out: Vec<Signals> = Vec::new();
    while out.len() < expected {
        let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out collecting events")
            .expect("channel closed");
        if let ImportEvent::SignalsUpdated { signals, .. } = event {
            out.push(signals);
        }
    }
    out
}

/// A throwaway `LibraryManager` over a temp dir. The folder extraction path these
/// tests drive never reads from the library, but `ExtractionService::start` requires
/// one anyway. The returned `TempDir` must outlive it.
async fn make_library_manager() -> (crate::library::LibraryManager, TempDir) {
    let tmp = TempDir::new().unwrap();
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let database = crate::db::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        clock.clone(),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    let library_dir = coven::StoreDir::new(tmp.path());
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
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
        config_handle,
        clock,
        Arc::new(coven::UuidProvider),
        crate::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        crate::import::cover_art::RemoteImageCache::for_test(),
    );
    (manager, tmp)
}

/// Start a service, keeping the library's temp dir alive. The bus sender comes back
/// too, so a test can inject an event the service listens for, like
/// `CandidateRemoved`.
async fn make_service() -> (
    ExtractionServiceHandle,
    broadcast::Sender<ImportEvent>,
    broadcast::Receiver<ImportEvent>,
    TempDir,
) {
    let (tx, rx) = broadcast::channel(64);
    let (library_manager, lib_tmp) = make_library_manager().await;
    let handle = ExtractionService::start(
        tokio::runtime::Handle::current(),
        tx.clone(),
        library_manager,
    );
    (handle, tx, rx, lib_tmp)
}

/// Just an ID3v2 header — enough for `is_valid_audio` to accept the file during
/// folder categorization.
fn minimal_mp3() -> Vec<u8> {
    let mut v = Vec::with_capacity(32);
    v.extend_from_slice(b"ID3");
    v.resize(32, 0);
    v
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

/// A release folder holding one MP3 (which satisfies the audio gate) plus whatever
/// images and documents the caller passes.
fn build_release(
    tmp: &TempDir,
    folder_name: &str,
    images: &[&str],
    documents: &[(&str, &str)],
) -> PathBuf {
    let folder = tmp.path().join(folder_name);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("01 - Track.mp3"), minimal_mp3()).unwrap();
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
    ExtractionSource::Folder {
        path: folder,
        files,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn emits_fast_pass_then_ocr_then_settled() {
    // Folder name carries a catalog-shaped bracket (XX34b), parent
    // carries an artist-shaped name. Images carry OCR lines.
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("Artist Name");
    fs::create_dir_all(&parent).unwrap();
    let folder = parent.join("1989 - Album Title [XX34b]");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("01 - Track.mp3"), minimal_mp3()).unwrap();
    fs::write(folder.join("Cover.jpg"), minimal_jpeg()).unwrap();
    fs::write(folder.join("Back.jpg"), minimal_jpeg()).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("Cover.jpg", vec!["WPCR-80001".to_string()])
            .with("Back.jpg", vec!["Extra Line".to_string()]),
    );
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder.clone()),
        CallPriority::Interactive,
    );

    // Fast-pass snapshot + 2 OCR snapshots + final settled = 4 snapshots.
    let signals = collect_signals(&mut rx, 4).await;
    assert_eq!(signals.len(), 4);

    // The folder bracket `XX34b` lands in catalogs and the path components (score 3,
    // above the cutoff) in free_text, while the text signal is still `Scanning`.
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
            .any(|c| c.value == "XX34b"),
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
        .any(|c| c.value == "XX34b"));

    assert!(signals[2]
        .text
        .catalogs()
        .iter()
        .any(|c| c.value == "WPCR-80001"));

    assert!(
        matches!(signals[3].text, TextSignal::Settled { .. }),
        "final text should be Settled, got {:?}",
        signals[3].text,
    );
    assert!(signals[3]
        .text
        .catalogs()
        .iter()
        .any(|c| c.value == "XX34b"));
    assert!(signals[3]
        .text
        .catalogs()
        .iter()
        .any(|c| c.value == "WPCR-80001"));
}

#[tokio::test(flavor = "multi_thread")]
async fn no_artwork_still_emits_fast_pass_and_settled() {
    let tmp = TempDir::new().unwrap();
    // No images, just a folder name with signals.
    let folder = build_release(&tmp, "Artist Name - Album Title", &[], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    // Fast pass + final settled.
    let signals = collect_signals(&mut rx, 2).await;
    assert_eq!(signals.len(), 2);
    assert!(matches!(signals[0].text, TextSignal::Scanning { .. }));
    assert!(matches!(signals[1].text, TextSignal::Settled { .. }));

    // No artwork means no barcode source, so the signal is `Absent` throughout and
    // never passes through `Scanning`.
    assert!(matches!(signals[0].barcode, BarcodeSignal::Absent));
    assert!(matches!(signals[1].barcode, BarcodeSignal::Absent));

    assert!(signals[0]
        .text
        .free_text()
        .iter()
        .any(|s| s.contains("Artist Name") || s.contains("Album Title")));
}

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

    let logs = capture_warn_logs(|| {
        emit_signals(
            &inner,
            "cand-1",
            Signals {
                disc_id: DiscIdSignal::Absent { track_count: 0 },
                barcode: BarcodeSignal::Absent,
                text: TextSignal::Settled {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                durations: crate::import::probe::ProbedDurations::default(),
            },
            CallPriority::Interactive,
        );
    });

    assert!(
        logs.contains("signals: SignalsUpdated broadcast had no subscribers"),
        "expected no-subscriber warning, got {logs:?}",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_pass_join_error_aborts_without_empty_snapshot() {
    let fast_pass = run_fast_pass_blocking(&tokio::runtime::Handle::current(), || -> FastPass {
        panic!("fast-pass blocking task panicked")
    })
    .await;

    assert!(
        fast_pass.is_none(),
        "fast-pass JoinError must abort the extraction run",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn ocr_join_error_aborts_without_settled_snapshot() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["cover.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(PanicAnalyzer);
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

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
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    // Fast pass + final settled.
    let signals = collect_signals(&mut rx, 2).await;
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
    // A CUE `CATALOG` is the disc's UPC/EAN, so it must surface as a barcode, not as
    // a catalog-number string.
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
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    // Fast pass + final settled.
    let signals = collect_signals(&mut rx, 2).await;
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

#[test]
fn non_utf8_cue_is_decoded_not_dropped() {
    // A Windows-1252 CUE, with a curly apostrophe (byte 0x92) inside a track title.
    // The scanner parses CUEs through `text_encoding`, so that byte is decoded rather
    // than the whole sheet being dropped.
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
    let pass = gather_non_ocr_sources(&folder, &files);
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
    // A text-file line scores 1, which can't clear the cutoff alongside path
    // components at 3. So make the text-file line match a path component: they cluster
    // together, and the combined score proves the text file reached the pipeline.
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
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    // Fast pass + final settled. The cluster scores PathComponent(3) + TextFile(1).
    let signals = collect_signals(&mut rx, 2).await;
    let final_free_text = signals[signals.len() - 1].text.free_text();
    assert!(
        final_free_text
            .iter()
            .any(|s| s == "Artist Alpha - Album Title B"),
        "expected path/text-file cluster to survive, got {final_free_text:?}",
    );
}

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
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

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
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder.clone()),
        CallPriority::Interactive,
    );
    handle.start(
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
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        folder_source(folder.clone()),
        CallPriority::Interactive,
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    handle.start(
        "cand-1".to_string(),
        folder_source(folder.clone()),
        CallPriority::Interactive,
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    handle.start(
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

/// A platform with no artwork analyzer has no barcode source in a candidate's
/// artwork, however many images it holds — nothing decodes them. The barcode
/// signal must say `Absent`, not `Settled { codes: [] }`: identify reads the
/// difference as "never scanned" vs "scanned and found none", and the second is a
/// claim about a decode that never happened.
#[tokio::test(flavor = "multi_thread")]
async fn no_analyzer_leaves_artwork_absent_rather_than_scanned() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Album Title", &["Cover.jpg", "Back.jpg"], &[]);

    // No `register_analyzer` — this is Windows and Linux today.
    let (handle, _tx, mut rx, _lib_tmp) = make_service().await;

    handle.start(
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    // Fast pass + final settled. No OCR snapshots: there is nothing to decode with.
    let signals = collect_signals(&mut rx, 2).await;
    assert_eq!(
        signals[0].barcode,
        BarcodeSignal::Absent,
        "artwork is not a barcode source without an analyzer",
    );
    assert_eq!(
        signals[1].barcode,
        BarcodeSignal::Absent,
        "settling must not report a scan that never ran",
    );
}

/// A CUE `CATALOG` barcode is a source in its own right — it needs no analyzer.
/// It settles even on a platform that can't decode artwork.
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
        "cand-1".to_string(),
        folder_source(folder),
        CallPriority::Interactive,
    );

    let signals = collect_signals(&mut rx, 2).await;
    let codes: Vec<&str> = signals[1]
        .barcode
        .codes()
        .iter()
        .map(|c| c.value.as_str())
        .collect();
    assert_eq!(codes, vec!["0075678164521"]);
    assert!(
        matches!(signals[1].barcode, BarcodeSignal::Settled { .. }),
        "a CUE catalog barcode settles without an analyzer, got {:?}",
        signals[1].barcode,
    );
}
