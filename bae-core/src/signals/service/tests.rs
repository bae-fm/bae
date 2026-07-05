use super::*;
use crate::identify::analyzer::ArtworkAnalyzer;
use crate::identify::ArtworkAnalysis;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tempfile::TempDir;

/// Test analyzer: returns canned text lines keyed by filename (not full
/// path, to stay portable across temp-dir paths). Optionally delays
/// each call so cancellation can be exercised.
struct StubAnalyzer {
    responses: StdMutex<HashMap<String, Vec<String>>>,
    delay: Option<Duration>,
}

impl StubAnalyzer {
    fn new() -> Self {
        Self {
            responses: StdMutex::new(HashMap::new()),
            delay: None,
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
}

impl ArtworkAnalyzer for StubAnalyzer {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
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

/// Drain events until we've seen the target count, or timeout. Filters
/// to `SignalsUpdated` only, returning each snapshot's `Signals`.
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

/// Build a throwaway `LibraryManager` over a temp dir. The folder/CD
/// extraction paths exercised here don't read from the library, but
/// `ExtractionService::start` requires one. The returned `TempDir`
/// must outlive the manager.
async fn make_library_manager() -> (crate::library::LibraryManager, TempDir) {
    let tmp = TempDir::new().unwrap();
    let clock: coven::ClockRef = Arc::new(coven::SystemClock);
    let database =
        crate::db::Database::new_test(tmp.path().join("test.db").to_str().unwrap(), clock.clone())
            .await
            .unwrap();
    let library_dir = coven::LibraryDir::new(tmp.path());
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
    let key_service = crate::keys::KeyService::new(library_id);
    let manager = crate::library::LibraryManager::new(
        database,
        library_dir,
        config_handle,
        key_service,
        clock,
        Arc::new(coven::UuidProvider),
        tokio::runtime::Handle::current(),
    );
    (manager, tmp)
}

/// Start a service and keep the library's temp dir alive for the test.
async fn make_service() -> (
    ExtractionServiceHandle,
    broadcast::Receiver<ImportEvent>,
    TempDir,
) {
    let (tx, rx) = broadcast::channel(64);
    let (library_manager, lib_tmp) = make_library_manager().await;
    let handle = ExtractionService::start(
        tokio::runtime::Handle::current(),
        tx,
        Arc::new(coven::SystemClock),
        library_manager,
    );
    (handle, rx, lib_tmp)
}

/// Minimal MP3 bytes — ID3v2 header. Enough for `is_valid_audio` to
/// accept the file during folder categorization.
fn minimal_mp3() -> Vec<u8> {
    // "ID3" magic + 7 bytes of padding is enough for the validator.
    let mut v = Vec::with_capacity(32);
    v.extend_from_slice(b"ID3");
    v.resize(32, 0);
    v
}

/// Minimal JPEG bytes — 0xFFD8FF magic + a byte. Enough for
/// `is_valid_image` to accept it.
fn minimal_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
}

/// Build a release folder with one MP3 (to satisfy the audio gate),
/// plus any images and documents the caller passes in. Returns the
/// folder path under the temp dir.
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
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        ExtractionSource::Folder(folder.clone()),
    );

    // Fast-pass snapshot + 2 OCR snapshots + final settled = 4 snapshots.
    let signals = collect_signals(&mut rx, 4).await;
    assert_eq!(signals.len(), 4);

    // Fast pass: folder-bracket `XX34b` lands in catalogs; path
    // components (score 3, above cutoff) land in free_text. Text is
    // still `Scanning` while artwork OCR is pending.
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

    // Artwork is processed in sorted order: Back.jpg, then Cover.jpg.
    // After Back.jpg: pool still dominated by path components; the
    // folder-bracket catalog is still present.
    assert!(signals[1]
        .text
        .catalogs()
        .iter()
        .any(|c| c.value == "XX34b"));

    // After Cover.jpg: WPCR-80001 joins catalogs.
    assert!(signals[2]
        .text
        .catalogs()
        .iter()
        .any(|c| c.value == "WPCR-80001"));

    // Final snapshot is settled with both catalogs.
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
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

    // Fast pass + final settled.
    let signals = collect_signals(&mut rx, 2).await;
    assert_eq!(signals.len(), 2);
    assert!(matches!(signals[0].text, TextSignal::Scanning { .. }));
    assert!(matches!(signals[1].text, TextSignal::Settled { .. }));

    // With no artwork there's no barcode source — the signal is `Absent`
    // throughout, never `Scanning`.
    assert!(matches!(signals[0].barcode, BarcodeSignal::Absent));
    assert!(matches!(signals[1].barcode, BarcodeSignal::Absent));

    // Fast pass contains at least the folder-name path component.
    assert!(signals[0]
        .text
        .free_text()
        .iter()
        .any(|s| s.contains("Artist Name") || s.contains("Album Title")));
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_folder_settles_gracefully() {
    // Service handles nonexistent folders — the scanner errors but the
    // service falls back to folder-name signals only. Here there's no
    // folder name to extract, so we still settle cleanly.
    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        ExtractionSource::Folder(PathBuf::from("/nonexistent-folder-path")),
    );

    // Fast pass (empty pool) + final settled.
    let signals = collect_signals(&mut rx, 2).await;
    assert!(matches!(signals[0].text, TextSignal::Scanning { .. }));
    assert!(matches!(
        signals[signals.len() - 1].text,
        TextSignal::Settled { .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn fast_pass_join_error_aborts_without_empty_snapshot() {
    let fast_pass =
        run_fast_pass_blocking(|| -> FastPass { panic!("fast-pass blocking task panicked") }).await;

    assert!(
        fast_pass.is_none(),
        "fast-pass JoinError must abort the extraction run",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cue_fields_land_in_fast_pass() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.mp3"), minimal_mp3()).unwrap();
    // CUE file with album-level + track-level values. The CUE's FILE
    // directive references a non-existent FLAC, which means this CUE
    // won't pair with the mp3 — it lands in documents and still gets
    // parsed by the fast pass.
    let cue = r#"PERFORMER "Artist Alpha"
TITLE "Album Title A"
FILE "audio.wav" WAVE
  TRACK 01 AUDIO
    PERFORMER "Artist Alpha"
    TITLE "Track One"
    INDEX 01 00:00:00
"#;
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

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
    // A CUE with a `CATALOG` field. The disc's UPC/EAN surfaces as a
    // barcode code, not as a catalog-number string. The CUE's FILE
    // directive references a non-existent WAV, so
    // it doesn't pair with the mp3 and lands in `unpaired_cue_sheets`;
    // its CATALOG is still harvested. Track count (1) matches the
    // on-disk audio (1 mp3), so the incomplete-rip guard doesn't fire.
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.mp3"), minimal_mp3()).unwrap();
    let cue = "CATALOG 0075678164521\n\
PERFORMER \"Artist Alpha\"\n\
TITLE \"Album Title A\"\n\
FILE \"audio.wav\" WAVE\n  \
  TRACK 01 AUDIO\n    \
    TITLE \"Track One\"\n    \
    INDEX 01 00:00:00\n";
    fs::write(folder.join("Album.cue"), cue).unwrap();

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(StubAnalyzer::new());
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

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
    // A Windows-1252 CUE: a curly apostrophe (byte 0x92) inside a track
    // title. The folder scanner parses CUEs through `text_encoding` (BOM /
    // UTF-8 / chardetng), so the non-UTF-8 byte is decoded rather than
    // dropping the whole sheet, and the names are harvested from the parsed
    // result.
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.mp3"), minimal_mp3()).unwrap();

    let mut cue: Vec<u8> = Vec::new();
    cue.extend_from_slice(b"PERFORMER \"Artist Alpha\"\n");
    cue.extend_from_slice(b"TITLE \"Album Title A\"\n");
    cue.extend_from_slice(b"FILE \"audio.wav\" WAVE\n");
    cue.extend_from_slice(b"  TRACK 01 AUDIO\n");
    cue.extend_from_slice(b"    TITLE \"I Ain");
    cue.push(0x92); // Windows-1252 right single quotation mark
    cue.extend_from_slice(b"t Got No Heart\"\n");
    cue.extend_from_slice(b"    INDEX 01 00:00:00\n");
    fs::write(folder.join("Album.cue"), &cue).unwrap();

    let pass = gather_non_ocr_sources(&folder);
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
    // Text-file content alone is score 1 per line, which won't clear
    // the primary cutoff when path components are present at score 3.
    // To verify the text-file content is flowing into the pipeline at
    // all, arrange for the text-file content to match a path component
    // — they'll cluster together and pick up the combined score.
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
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

    // Fast pass + final settled.
    let signals = collect_signals(&mut rx, 2).await;
    // The path component `Artist Alpha - Album Title B` clusters with
    // the identical line from info.txt. Cluster score: PathComponent(3)
    // + TextFile(1) = 4 → well above cutoff.
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
    // A cancelled OCR run stops emitting and tears down — it never
    // reaches its final `Settled` snapshot. With three delayed images
    // and a cancel mid-OCR, the run must not produce a `TextSignal::
    // Settled` for the cancelled key.
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg", "p2.jpg", "p3.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .with("p2.jpg", vec!["Artist B".to_string()])
            .with("p3.jpg", vec!["Artist C".to_string()])
            .with_delay(Duration::from_millis(100)),
    );
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.cancel("cand-1");

    // Drain for a short window after the cancel. The cancelled run does
    // not complete, so no `Settled` snapshot for this key ever arrives.
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
async fn restart_for_same_key_cancels_prior_then_starts_fresh() {
    // Two `start` calls for the same key: the first is cancelled by the
    // second's insert. Cancelled runs don't settle; the completed run
    // does. We assert the last run reaches a `Settled` snapshot and the
    // generation-guarded teardown neither deadlocks nor panics.
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .with_delay(Duration::from_millis(100)),
    );
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        ExtractionSource::Folder(folder.clone()),
    );
    handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

    // Drain the event window; at least one completed run must settle.
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

/// The race we're guarding against: three consecutive `start` calls for
/// the same key. Without per-task generations, the first task's
/// cancellation-teardown could fire *after* the second `start` inserts
/// its token and remove it, leaving the third `start` with nothing to
/// cancel.
///
/// To provoke the race we hold OCR long enough that the first task is
/// still alive when we issue the third `start`. Cancelled runs don't
/// emit a final snapshot, so we can't count them directly; instead we
/// assert the completed run settles and the `(generation, token)`
/// teardown guard neither deadlocks nor panics under the concurrent
/// start/cancel/teardown interleaving.
#[tokio::test(flavor = "multi_thread")]
async fn three_starts_cancel_each_predecessor() {
    let tmp = TempDir::new().unwrap();
    let folder = build_release(&tmp, "Some Folder", &["p1.jpg"], &[]);

    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(
        StubAnalyzer::new()
            .with("p1.jpg", vec!["Artist A".to_string()])
            .with_delay(Duration::from_millis(200)),
    );
    let (handle, mut rx, _lib_tmp) = make_service().await;
    handle.register_analyzer(analyzer);

    handle.start(
        "cand-1".to_string(),
        ExtractionSource::Folder(folder.clone()),
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    handle.start(
        "cand-1".to_string(),
        ExtractionSource::Folder(folder.clone()),
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    handle.start("cand-1".to_string(), ExtractionSource::Folder(folder));

    // Drain until the channel goes quiet. A completed run produces a
    // `Settled` snapshot; cancelled runs tear down silently.
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
