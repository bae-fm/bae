use super::*;
use bae_core::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
    ScannedFile,
};
use bae_core::import::{ImportEvent, ImportProgress};
use bae_core::library::LibraryManager;
use std::path::PathBuf;

/// An `Automation` over a real library in a temporary directory, keeping the
/// manager and services so tests can write the scan rows and runtime it reads.
pub(super) struct Fixture {
    automation: Automation,
    manager: LibraryManager,
    services: AppServices,
    tmp: tempfile::TempDir,
}

impl Fixture {
    pub(super) async fn list_candidates(&self) -> Vec<AutomationCandidate> {
        self.automation
            .list_candidates()
            .await
            .expect("the list reads")
    }

    pub(super) async fn get_candidate(
        &self,
        key: &str,
    ) -> Result<AutomationCandidate, AutomationError> {
        self.automation.get_candidate(key.to_string()).await
    }

    pub(super) async fn skip(&self, key: &str) {
        self.automation
            .set_candidate_skipped(key.to_string(), true)
            .await
            .expect("the skip persists");
    }

    /// Claim `key` for [`IMPORT_ID`] as starting an import does; the runtime
    /// only takes progress from the import holding the key.
    pub(super) async fn claim(&self, key: &str) {
        self.services
            .claim_candidate_for_import_for_test(&candidate_key_named(key).unwrap(), IMPORT_ID)
            .await;
    }

    /// Emit one import event as the import service would; the runtime has
    /// recorded it by the time this returns.
    pub(super) fn record(&self, event: ImportEvent) {
        self.services.import_emit_event_for_test(event);
    }

    /// The watched root under the temporary directory, created if missing.
    pub(super) fn root(&self) -> PathBuf {
        let root = self.tmp.path().join("watched");
        std::fs::create_dir_all(&root).expect("the watched root exists");
        root
    }
}

pub(super) async fn automation_over() -> Fixture {
    let tmp = tempfile::TempDir::new().expect("a temp library dir");
    let (manager, _) = bae_test_support::open_test_library(tmp.path()).await;
    let services = AppServices::for_test(manager.clone())
        .await
        .expect("the services start");
    let automation = Automation::new(services.clone(), &tokio::runtime::Handle::current());
    Fixture {
        automation,
        manager,
        services,
        tmp,
    }
}

#[tokio::test]
async fn watched_folder_tool_reads_current_store_rows() {
    let fixture = automation_over().await;
    let root = fixture.root();
    for watched in [false, true, false] {
        if watched {
            fixture
                .manager
                .add_watched_import_folder(&root)
                .await
                .unwrap();
        } else {
            fixture
                .manager
                .remove_watched_import_folders(vec![root.clone()], None)
                .await
                .unwrap();
        }
        let response = fixture
            .automation
            .call_tool(AutomationTool::WatchedFoldersList, serde_json::json!({}))
            .await
            .unwrap();
        let folders = response["watched_folders"].as_array().unwrap();
        assert_eq!(folders.len(), usize::from(watched));
        if watched {
            assert_eq!(folders[0]["path"], coven::path_text(&root));
            assert_eq!(folders[0]["name"], "watched");
        }
    }
}

fn candidate(root: &std::path::Path, name: &str) -> FolderCandidate {
    FolderCandidate {
        path: root.join(name),
        file_root: root.join(name),
        name: name.to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                file: {
                    let mut file = ScannedFile::new(
                        root.join(name).join("01.flac"),
                        "01.flac".to_string(),
                        1_000,
                        0,
                    );
                    file.source_audio = Some(bae_core::import::folder_scanner::ScannedAudio {
                        content_type: bae_util::content_type::ContentType::Flac,
                        duration_ms: 1_000,
                        format: bae_core::album_detail::AudioFormat {
                            codec: "FLAC".to_string(),
                            sample_rate_hz: 44_100,
                            bits_per_sample: Some(16),
                            bitrate_kbps: None,
                            channels: 2,
                        },
                    });
                    file
                },
                role: FileRole::Audio,
            }],
            parts: Vec::new(),
        },
        watched_folder_path: root.to_path_buf(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: bae_core::import::RelativeFolder::of(std::path::Path::new(name)).unwrap(),
        grouping: None,
    }
}

/// Store a scan of the watched root holding only the candidate `name`, and
/// return the key automation names it by.
pub(super) async fn scan(fixture: &Fixture, name: &str) -> String {
    let root = fixture.root();
    write_scan(fixture, &root, |items| {
        items.push(ScanItem::Valid(candidate(&root, name)))
    })
    .await;
    key_of(&root, name)
}

/// The key automation names the candidate at `name` below `root` by.
fn key_of(root: &std::path::Path, name: &str) -> String {
    candidate_key_name(&CandidateKey::folder(&root.join(name)))
}

/// Watch `root` and store one complete scan of it.
async fn write_scan(
    fixture: &Fixture,
    root: &std::path::Path,
    build: impl FnOnce(&mut Vec<ScanItem>),
) {
    let manager = &fixture.manager;
    manager
        .add_watched_import_folder(root)
        .await
        .expect("the root is watched");
    let generation = manager
        .begin_folder_scan(root)
        .await
        .expect("a scan generation opens");
    let mut items = Vec::new();
    build(&mut items);
    for item in &items {
        manager
            .save_folder_scan_item(root, generation, item)
            .await
            .expect("the scan item persists");
    }
    manager
        .finish_folder_scan(root, generation, None)
        .await
        .expect("the scan finishes");
}

fn keys(candidates: &[AutomationCandidate]) -> Vec<&str> {
    candidates.iter().map(AutomationCandidate::key).collect()
}

/// The import `importing` reports progress for.
const IMPORT_ID: &str = "import-1";

fn importing(key: &str, percent: u8) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: candidate_key_named(key).unwrap(),
        progress: ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: Some(percent),
            phase: bae_core::import::ImportPhase::MeasuringLoudness,
            import_id: IMPORT_ID.to_string(),
        },
    }
}

/// A `reidentify:` runtime entry names a release, not a folder: it is not
/// listed or found as a candidate, and its progress is not an error.
#[tokio::test]
async fn runtime_only_entries_are_not_candidates() {
    let fixture = automation_over().await;
    let key = scan(&fixture, "A").await;
    let reidentify = candidate_key_name(&CandidateKey::reidentify("release-1"));
    fixture.claim(&reidentify).await;
    fixture.record(importing(&reidentify, 1));

    assert_eq!(keys(&fixture.list_candidates().await), vec![key.as_str()]);
    assert_eq!(
        fixture
            .get_candidate(&reidentify)
            .await
            .expect_err("a re-identify run is not an import candidate")
            .kind(),
        "not_found"
    );
}

/// Each call reads the tables: a skip survives a rescan, and a folder the
/// rescan dropped is gone.
#[tokio::test]
async fn the_tables_are_the_answer() {
    let fixture = automation_over().await;
    let root = fixture.root();
    write_scan(&fixture, &root, |items| {
        items.push(ScanItem::Valid(candidate(&root, "A")));
        items.push(ScanItem::Valid(candidate(&root, "B")));
    })
    .await;

    fixture.skip(&key_of(&root, "A")).await;
    write_scan(&fixture, &root, |items| {
        items.push(ScanItem::Valid(candidate(&root, "A")))
    })
    .await;

    assert_eq!(
        keys(&fixture.list_candidates().await),
        vec![key_of(&root, "A").as_str()]
    );
    let candidate = fixture
        .get_candidate(&key_of(&root, "A"))
        .await
        .expect("still scanned");
    assert!(
        candidate.common().skipped,
        "the stored decision is what is read"
    );
    assert_eq!(
        fixture
            .get_candidate(&key_of(&root, "B"))
            .await
            .expect_err("a candidate the scan dropped is gone")
            .kind(),
        "not_found"
    );
}

/// A candidate's runtime carries the progress of the import holding it.
#[tokio::test]
async fn a_candidate_carries_the_import_service_s_runtime() {
    let fixture = automation_over().await;
    let key = scan(&fixture, "A").await;
    fixture.claim(&key).await;
    // The worker takes the import up, then reports how far it has got.
    fixture
        .services
        .move_candidate_import_for_test(
            &candidate_key_named(&key).unwrap(),
            IMPORT_ID,
            bae_core::import::ImportStanding::Running,
        )
        .await;
    fixture.record(importing(&key, 42));

    let candidate = fixture.get_candidate(&key).await.expect("published");
    let json = serde_json::to_value(&candidate).unwrap();
    assert_eq!(json["runtime"]["import_status"]["kind"], "importing");
    assert_eq!(json["runtime"]["import_status"]["progress_percent"], 42);
    assert_eq!(json["runtime"]["import_status"]["step"]["kind"], "running");
    assert_eq!(
        json["runtime"]["import_status"]["step"]["phase"],
        "measuring_loudness"
    );
    assert_eq!(json["runtime"]["identify_state"]["kind"], "idle");
}

/// Waiting for a scan answers once every watched folder's read has ended, with
/// what the reads found.
#[tokio::test(flavor = "multi_thread")]
async fn a_scan_waited_on_answers_with_what_every_folder_read_found() {
    let fixture = automation_over().await;
    let first = fixture.tmp.path().join("First");
    let second = fixture.tmp.path().join("Second");
    for (root, album) in [(&first, "Album One"), (&second, "Album Two")] {
        let album = root.join(album);
        std::fs::create_dir_all(&album).unwrap();
        bae_test_support::write_tagged_flac(&album, "01 Track.flac", "Track");
        fixture
            .automation
            .add_watched_folder(root.to_string_lossy().into_owned())
            .await
            .expect("the folder is watched");
    }

    let result = fixture
        .automation
        .scan_watched_folders(ScanWait::UntilFinished { timeout_ms: 30_000 })
        .await
        .expect("both reads end");

    assert_eq!(result.watched_folders.len(), 2);
    assert_eq!(
        result.candidates.len(),
        2,
        "each folder's album is read by the time the wait answers"
    );
}

/// A folder whose read fails is named in the error the wait answers with.
#[tokio::test(flavor = "multi_thread")]
async fn a_scan_waited_on_names_a_folder_whose_read_failed() {
    let fixture = automation_over().await;
    let root = fixture.tmp.path().join("Gone");
    std::fs::create_dir_all(root.join("Album")).unwrap();
    let path = root.canonicalize().unwrap().to_string_lossy().into_owned();
    fixture
        .automation
        .add_watched_folder(path.clone())
        .await
        .expect("the folder is watched");
    std::fs::remove_dir_all(&root).unwrap();

    let error = fixture
        .automation
        .scan_watched_folders(ScanWait::UntilFinished { timeout_ms: 30_000 })
        .await
        .expect_err("the read of a folder that is gone fails");
    assert!(error.message().contains(&path), "{}", error.message());
}
