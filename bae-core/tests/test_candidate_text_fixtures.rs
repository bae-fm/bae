#![cfg(feature = "test-utils")]
//! Regression corpus for the signal-extraction service: each fixture in
//! `tests/fixtures/candidate_text/*.json` declares a candidate's sources, is
//! built into a temp folder, run through `ExtractionService` with a stub
//! analyzer, and checked for which catalog numbers and free text survive.

use bae_core::import::ImportEventBus;
use bae_core::signals::service::{ExtractionService, ExtractionServiceHandle, ExtractionSource};
use bae_core::signals::{ArtworkAnalysis, ArtworkAnalyzer, TextSignal};
use bae_test_support as support;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    #[serde(rename = "notes")]
    _notes: Option<String>,
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
    bae_test_support::fixture_dir!("candidate_text")
}

fn load_fixture(path: &Path) -> Fixture {
    let bytes =
        std::fs::read(path).unwrap_or_else(|e| panic!("failed to read fixture {path:?}: {e}"));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("failed to parse fixture {path:?}: {e}"))
}

const PROBEABLE_MP3: &[u8] = include_bytes!("../test-fixtures/audio-format/placeholder-mp3.mp3");

/// Just the JPEG magic, enough to pass as an image.
fn minimal_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]
}

/// Returns the fixture's OCR lines for an absolute path, none for others.
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

/// Build a fixture's release folder in `tmp`, returning it and the OCR lines
/// keyed by each artwork file's absolute path.
fn materialize(fixture: &Fixture, tmp: &TempDir) -> (PathBuf, HashMap<PathBuf, Vec<String>>) {
    // `{tmp}/{parent}/{release}`, from the last two path components.
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

    // Folder brackets go on the release folder's name.
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

    // One audio file so the folder is a candidate; audio file names feed no
    // pool.
    fs::write(folder.join("sentinel.mp3"), PROBEABLE_MP3).unwrap();

    // File names land on images, whose names the pass reads.
    for stem in &fixture.sources.filenames_generic {
        let name = format!("{stem}.jpg");
        fs::write(folder.join(&name), minimal_jpeg()).unwrap();
    }

    // Each CUE field becomes a top-level TITLE.
    if !fixture.sources.cue_fields.is_empty() {
        let cue_body: String = fixture
            .sources
            .cue_fields
            .iter()
            .map(|s| format!("TITLE \"{s}\"\n"))
            .collect();
        fs::write(folder.join("fixture.cue"), cue_body).unwrap();
    }

    // Each text file becomes a `.txt` named after its declared path.
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

    // Each artwork file is a stub JPEG; the OCR map carries its lines.
    let mut ocr_map: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for art in &fixture.sources.artwork {
        let basename = Path::new(&art.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Cover.jpg".to_string());
        let materialized = folder.join(&basename);
        fs::write(&materialized, minimal_jpeg()).unwrap();
        // The analyzer receives the canonical path.
        let canonical = materialized.canonicalize().unwrap_or(materialized.clone());
        ocr_map.insert(canonical, art.lines.clone());
    }

    let canonical_folder = folder.canonicalize().unwrap_or(folder);
    (canonical_folder, ocr_map)
}

/// A throwaway `LibraryManager` over a temp dir, which must outlive it.
async fn make_library_manager() -> (bae_core::library::LibraryManager, TempDir) {
    let tmp = TempDir::new().expect("library temp dir");
    let (manager, _db) = support::open_test_library(tmp.path()).await;
    (manager, tmp)
}

async fn drive_fixture(
    fixture: &Fixture,
    folder: PathBuf,
    ocr_map: HashMap<PathBuf, Vec<String>>,
) -> (Vec<String>, Vec<String>) {
    let candidates = bae_core::import::CandidateRuntime::default();
    let tx = ImportEventBus::new(128, candidates.clone());
    let (library_manager, _lib_tmp) = make_library_manager().await;
    let handle: ExtractionServiceHandle = ExtractionService::start(
        tokio::runtime::Handle::current(),
        tx,
        candidates,
        library_manager,
    );
    let analyzer: Arc<dyn ArtworkAnalyzer> = Arc::new(FixtureAnalyzer::new(ocr_map));
    handle.register_analyzer(analyzer);

    let key = format!("fixture:{}", fixture.name);
    let files = bae_core::import::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        bae_core::import::ReleaseFileScope::Recursive,
        &bae_core::import::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("fixture scan");
    let mut run = handle.start(
        bae_core::identify::IdentifyRunId::for_test(1),
        key,
        ExtractionSource::Candidate {
            candidate: bae_core::import::FolderCandidate {
                name: folder.file_name().unwrap().to_string_lossy().into_owned(),
                display_path: folder.file_name().unwrap().to_string_lossy().into_owned(),
                watched_folder_path: folder.to_string_lossy().into_owned(),
                file_root: folder.clone(),
                path: folder,
                files,
                scope: bae_core::import::ReleaseFileScope::Recursive,
                file_edit_revision: 0,
                grouping: None,
            },
        },
        bae_core::util::rate_limiter::CallPriority::Interactive,
        bae_core::config::IdentificationSteps::default(),
    );

    // The run's watch holds its latest snapshot until the text settles.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(snapshot) = run.borrow_and_update().as_ref() {
                if let TextSignal::Settled { free_text, .. } = &snapshot.signals.text {
                    return (snapshot.signals.text.catalogs().to_vec(), free_text.clone());
                }
            }
            run.changed()
                .await
                .expect("the extraction settles before it ends");
        }
    })
    .await
    .expect("timed out waiting for settled signals")
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
