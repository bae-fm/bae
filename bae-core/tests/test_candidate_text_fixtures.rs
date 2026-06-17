#![cfg(feature = "test-utils")]
//! Regression corpus for the signal-extraction service.
//!
//! Each fixture declares the per-source inputs the classifier would
//! receive for a realistic candidate shape — path components, folder
//! brackets, filenames, CUE strings, text-file lines, and artwork OCR.
//! The harness materializes those declarations into a temp release
//! folder, drives the full `ExtractionService` end-to-end via a stub
//! `ArtworkAnalyzer`, and asserts against the terminal settled `Signals`.
//!
//! Expected output is a loose shape check: "contains" / "not contains"
//! for the free-text and catalog pools. The classifier's exact rank
//! order is tunable starting-value territory, so fixtures only pin the
//! "it did / didn't survive" contract.
//!
//! Fixtures live under `tests/fixtures/candidate_text/*.json`. Adding a
//! new fixture just requires dropping another file in that directory —
//! the loop below picks them up automatically.

use bae_core::identify::{ArtworkAnalysis, ArtworkAnalyzer};
use bae_core::import::ImportEvent;
use bae_core::signals::service::{ExtractionService, ExtractionServiceHandle, ExtractionSource};
use bae_core::signals::TextSignal;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::broadcast;

#[derive(Debug, Deserialize)]
struct Fixture {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    notes: Option<String>,
    sources: Sources,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct Sources {
    path_components: Vec<String>,
    folder_brackets: Vec<String>,
    filenames_generic: Vec<String>,
    cue_fields: Vec<String>,
    text_files: Vec<TextFile>,
    artwork: Vec<Artwork>,
}

#[derive(Debug, Deserialize)]
struct Artwork {
    path: String,
    lines: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TextFile {
    path: String,
    lines: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Expected {
    catalogs: Option<Vec<String>>,
    catalogs_contains: Option<Vec<String>>,
    catalogs_not_contains: Option<Vec<String>>,
    free_text: Option<Vec<String>>,
    free_text_contains: Option<Vec<String>>,
    free_text_not_contains: Option<Vec<String>>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/candidate_text")
}

fn load_fixture(path: &Path) -> Fixture {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("failed to read fixture {path:?}: {e}"));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("failed to parse fixture {path:?}: {e}"))
}

/// Minimal valid MP3: ID3v2 header + padding. The folder scanner's audio
/// validator only checks the first bytes against known magic numbers.
fn minimal_mp3() -> Vec<u8> {
    let mut v = Vec::with_capacity(32);
    v.extend_from_slice(b"ID3");
    v.resize(32, 0);
    v
}

/// Minimal JPEG magic — `is_valid_image` dispatches by extension and
/// checks the first three bytes.
fn minimal_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
}

/// Stub analyzer keyed by absolute path. Returns the fixture's declared
/// OCR lines for that path; empty for any unknown path.
struct FixtureAnalyzer {
    responses: Mutex<HashMap<PathBuf, Vec<String>>>,
}

impl FixtureAnalyzer {
    fn new(responses: HashMap<PathBuf, Vec<String>>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl ArtworkAnalyzer for FixtureAnalyzer {
    fn analyze(&self, path: &Path) -> ArtworkAnalysis {
        let text_lines = self
            .responses
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_default();
        ArtworkAnalysis {
            barcodes: Vec::new(),
            text_lines,
        }
    }
}

/// Materialize a fixture into `tmp`: build a release folder whose path
/// components reproduce the declared `path_components` and `folder_brackets`,
/// and drop files inside for each declared source.
///
/// Returns `(release_folder, ocr_map)`. The OCR map is keyed by the
/// materialized absolute artwork paths so the stub analyzer can look them
/// up when the service fans out OCR calls.
fn materialize(fixture: &Fixture, tmp: &TempDir) -> (PathBuf, HashMap<PathBuf, Vec<String>>) {
    // Folder layout: `{tmp}/{parent}/{release}`. If path_components has only
    // one entry, it becomes the release folder name and the parent is tmp
    // directly. Empty path_components just put the release at tmp with a
    // generic name.
    let (parent_name, release_base) = match fixture.sources.path_components.as_slice() {
        [] => (None, "release".to_string()),
        [single] => (None, single.clone()),
        components => {
            let len = components.len();
            (
                Some(components[len - 2].clone()),
                components[len - 1].clone(),
            )
        }
    };

    // Append folder-bracket tags so `extract_folder_brackets` can pick them
    // up from the release folder name and `strip_path_component` can strip
    // them back to the declared path component.
    let mut release_folder_name = release_base.clone();
    for bracket in &fixture.sources.folder_brackets {
        release_folder_name.push_str(&format!(" [{bracket}]"));
    }

    let parent_dir = match parent_name {
        Some(p) => tmp.path().join(p),
        None => tmp.path().to_path_buf(),
    };
    fs::create_dir_all(&parent_dir).unwrap();
    let folder = parent_dir.join(&release_folder_name);
    fs::create_dir_all(&folder).unwrap();

    // Sentinel audio file so `collect_release_candidate_files` always
    // detects at least one track and runs the full categorization. Audio
    // filenames don't feed the suggestion pool — their stems are track
    // titles, the wrong bucket for Artist / Album autocomplete — so
    // fixtures don't need to declare specific audio files.
    fs::write(folder.join("sentinel.mp3"), minimal_mp3()).unwrap();

    // Generic filenames land on image files so they're tagged as artwork
    // by the categorizer and the service tags them FilenameGeneric(.jpg).
    for stem in &fixture.sources.filenames_generic {
        let name = format!("{stem}.jpg");
        fs::write(folder.join(&name), minimal_jpeg()).unwrap();
    }

    // CUE file: emit each declared field as a top-level PERFORMER or
    // TITLE. `parse_cue_strings` accepts both unconditionally, so the
    // choice of keyword doesn't matter for the extraction.
    if !fixture.sources.cue_fields.is_empty() {
        let cue_body: String = fixture
            .sources
            .cue_fields
            .iter()
            .map(|s| format!("TITLE \"{s}\"\n"))
            .collect();
        fs::write(folder.join("fixture.cue"), cue_body).unwrap();
    }

    // Text files: each entry becomes a `.txt` under the release folder.
    // File name is derived from the last component of the declared path so
    // tests can distinguish between multiple text files.
    for (idx, tf) in fixture.sources.text_files.iter().enumerate() {
        let basename = Path::new(&tf.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("fixture-text-{idx}.txt"));
        let name = if Path::new(&basename).extension().is_some() {
            basename
        } else {
            format!("{basename}.txt")
        };
        fs::write(folder.join(&name), tf.lines.join("\n")).unwrap();
    }

    // Artwork: materialize each declared path as an empty JPEG in the
    // release folder. The stub analyzer is keyed by the materialized path,
    // so the file's content doesn't matter — the OCR map carries the lines.
    let mut ocr_map: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for art in &fixture.sources.artwork {
        let basename = Path::new(&art.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Cover.jpg".to_string());
        let materialized = folder.join(&basename);
        fs::write(&materialized, minimal_jpeg()).unwrap();
        // Use the canonicalized path because the folder scanner resolves
        // symlinks / `.`/`..` while enumerating, and the analyzer receives
        // the canonical form.
        let canonical = materialized.canonicalize().unwrap_or(materialized.clone());
        ocr_map.insert(canonical, art.lines.clone());
    }

    let canonical_folder = folder.canonicalize().unwrap_or(folder);
    (canonical_folder, ocr_map)
}

/// Build a throwaway `LibraryManager` over a temp dir. The fixture's
/// folder-extraction path doesn't read from the library, but
/// `ExtractionService::start` requires one. The returned `TempDir` must
/// outlive the manager.
async fn make_library_manager() -> (bae_core::library::LibraryManager, TempDir) {
    let tmp = TempDir::new().expect("library temp dir");
    let clock: bae_core::clock::ClockRef = Arc::new(bae_core::clock::SystemClock);
    let database = bae_core::db::Database::new_test(
        tmp.path().join("test.db").to_str().unwrap(),
        clock.clone(),
    )
    .await
    .unwrap();
    let library_dir = bae_core::library_dir::LibraryDir::new(tmp.path());
    // Unique id per test so keyring entries don't collide in the shared
    // process-global mock store (see `install_test_keyring`).
    let library_id = format!("test-{}", uuid::Uuid::new_v4());
    let config = bae_core::config::Config::with_defaults(
        library_id.clone(),
        "test-device".to_string(),
        library_dir.clone(),
        "Test Library".to_string(),
    );
    let config_handle = Arc::new(bae_core::config::ConfigHandle::new(config));
    bae_core::config::install_test_keyring();
    let key_service = bae_core::keys::KeyService::new(library_id);
    let manager = bae_core::library::LibraryManager::new(
        database,
        library_dir,
        config_handle,
        key_service,
        clock,
        Arc::new(bae_core::id_provider::UuidProvider),
        tokio::runtime::Handle::current(),
        None,
    );
    (manager, tmp)
}

async fn drive_fixture(
    fixture: &Fixture,
    folder: PathBuf,
    ocr_map: HashMap<PathBuf, Vec<String>>,
) -> (Vec<String>, Vec<String>) {
    let (tx, mut rx) = broadcast::channel::<ImportEvent>(128);
    let (library_manager, _lib_tmp) = make_library_manager().await;
    let handle: ExtractionServiceHandle = ExtractionService::start(
        tokio::runtime::Handle::current(),
        tx,
        std::sync::Arc::new(bae_core::clock::SystemClock),
        library_manager,
    );
    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(FixtureAnalyzer::new(ocr_map));
    handle.register_analyzer(analyzer);

    let key = format!("fixture:{}", fixture.name);
    handle.start(key, ExtractionSource::Folder(folder));

    // Pull snapshots until the text signal settles. The fixture pipeline is
    // synchronous under the hood — everything finishes in well under a
    // second — so a 10s ceiling is just a safety net.
    let deadline = Duration::from_secs(10);
    loop {
        let event = tokio::time::timeout(deadline, rx.recv())
            .await
            .expect("timed out waiting for settled signals")
            .expect("event channel closed unexpectedly");

        if let ImportEvent::SignalsUpdated { signals, .. } = event {
            if let TextSignal::Settled { free_text, .. } = &signals.text {
                // Catalogs carry their origin now; the fixtures assert on the
                // values, so project to the value strings.
                let catalog_values = signals
                    .text
                    .catalogs()
                    .iter()
                    .map(|c| c.value.clone())
                    .collect();
                return (catalog_values, free_text.clone());
            }
        }
    }
}

async fn assert_fixture(fixture_path: &Path) {
    let fixture = load_fixture(fixture_path);
    let tmp = TempDir::new().expect("temp dir");
    let (folder, ocr_map) = materialize(&fixture, &tmp);
    let (catalogs, free_text) = drive_fixture(&fixture, folder, ocr_map).await;

    let fixture_name = fixture_path.file_name().unwrap().to_string_lossy();

    // Exact-match expectations override contains-only checks.
    if let Some(expected) = &fixture.expected.catalogs {
        assert_eq!(
            catalogs,
            expected.clone(),
            "[{fixture_name}] catalogs mismatch — got {catalogs:?}, expected {expected:?}",
        );
    }
    if let Some(expected) = &fixture.expected.free_text {
        assert_eq!(
            free_text,
            expected.clone(),
            "[{fixture_name}] free_text mismatch — got {free_text:?}, expected {expected:?}",
        );
    }

    // Loose-shape contains / not-contains checks.
    if let Some(required) = &fixture.expected.catalogs_contains {
        for s in required {
            assert!(
                catalogs.contains(s),
                "[{fixture_name}] expected catalog {s:?} to survive, got {catalogs:?}",
            );
        }
    }
    if let Some(banned) = &fixture.expected.catalogs_not_contains {
        for s in banned {
            assert!(
                !catalogs.contains(s),
                "[{fixture_name}] catalog {s:?} should have been filtered, got {catalogs:?}",
            );
        }
    }
    if let Some(required) = &fixture.expected.free_text_contains {
        for s in required {
            assert!(
                free_text.contains(s),
                "[{fixture_name}] expected free_text {s:?} to survive, got {free_text:?}",
            );
        }
    }
    if let Some(banned) = &fixture.expected.free_text_not_contains {
        for s in banned {
            assert!(
                !free_text.contains(s),
                "[{fixture_name}] free_text {s:?} should have been filtered, got {free_text:?}",
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn run_all_candidate_text_fixtures() {
    let dir = fixtures_dir();
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("failed to list fixtures at {dir:?}: {e}"));

    let mut fixture_count = 0;
    for entry in entries {
        let entry = entry.expect("fixture entry");
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        fixture_count += 1;
        assert_fixture(&path).await;
    }

    assert!(fixture_count > 0, "no JSON fixtures found under {dir:?}");
}
