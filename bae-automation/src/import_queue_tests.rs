use super::*;
use bae_core::import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
    ScannedFile,
};
use bae_core::import::{ImportEvent, ImportProgress};
use bae_core::library::LibraryManager;
use std::path::PathBuf;

/// An automation surface over a real library in a temporary directory,
/// with the manager and services behind it so a test can write the scan
/// rows and the runtime the surface reads.
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

    /// Record one import event the way the import service would.
    pub(super) fn record(&self, event: ImportEvent) {
        self.services.import_emit_event_for_test(event);
    }

    /// Emit `event` and return once the runtime recorder has taken it in.
    /// The recorder reads the bus on its own task, so a test that asserts
    /// on what it recorded has to wait for it rather than for the bus.
    pub(super) async fn record_and_settle(&self, event: ImportEvent) {
        let mut changes = self.services.subscribe_candidate_runtime().1;
        self.services.import_emit_event_for_test(event);
        tokio::time::timeout(std::time::Duration::from_secs(5), changes.recv())
            .await
            .expect("the recorder takes the event in")
            .expect("the runtime stream stays open");
    }

    /// The watched root under this fixture's temporary directory.
    pub(super) fn root(&self) -> String {
        let root = self.tmp.path().join("watched");
        std::fs::create_dir_all(&root).expect("the watched root exists");
        root.to_string_lossy().into_owned()
    }
}

pub(super) async fn automation_over() -> Fixture {
    let tmp = tempfile::TempDir::new().expect("a temp library dir");
    let library_dir = coven::StoreDir::new(tmp.path());
    let manager = LibraryManager::open(
        bae_test_support::test_config(&library_dir),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        bae_core::diagnostics::Diagnostics::noop(),
        tokio::runtime::Handle::current(),
        None,
        bae_core::import::cover_art::RemoteImageCache::for_test(),
    )
    .expect("the library opens");
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
                .remove_watched_import_folder(&root)
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
            assert_eq!(folders[0]["path"], root);
            assert_eq!(folders[0]["name"], "watched");
        }
    }
}

fn candidate(root: &str, name: &str) -> FolderCandidate {
    FolderCandidate {
        path: PathBuf::from(format!("{root}/{name}")),
        file_root: PathBuf::from(format!("{root}/{name}")),
        name: name.to_string(),
        files: CategorizedFiles {
            files: vec![CandidateFile {
                proposed_audio: true,
                file: {
                    let mut file = ScannedFile::new(
                        PathBuf::from(format!("{root}/{name}/01.flac")),
                        "01.flac".to_string(),
                        1_000,
                        0,
                    );
                    file.source_audio = Some(bae_core::import::folder_scanner::ScannedAudio {
                        content_type: bae_core::util::content_type::ContentType::Flac,
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
        },
        watched_folder_path: root.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: name.to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    }
}

/// Write one scanned candidate under a watched root, and hand back its key.
pub(super) async fn scan(fixture: &Fixture, name: &str) -> String {
    let root = fixture.root();
    write_scan(fixture, &root, |items| {
        items.push(ScanItem::Valid(candidate(&root, name)))
    })
    .await;
    format!("{root}/{name}")
}

/// Write a whole scan generation under a watched root.
async fn write_scan(fixture: &Fixture, root: &str, build: impl FnOnce(&mut Vec<ScanItem>)) {
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

fn importing(key: &str, percent: u8) -> ImportEvent {
    ImportEvent::ImportProgress {
        candidate_key: key.to_string(),
        progress: ImportProgress::Progress {
            id: "release-1".to_string(),
            percent: Some(percent),
            phase: bae_core::import::ImportPhase::MeasuringLoudness,
            import_id: "import-1".to_string(),
        },
    }
}

/// `reidentify:` runtime entries name releases, not scanned folders. They
/// carry runtime but no candidate, so they are not listed — and their
/// updates are not an unknown key to fail on.
#[tokio::test]
async fn runtime_only_entries_are_not_candidates() {
    let fixture = automation_over().await;
    let key = scan(&fixture, "A").await;
    fixture.record(importing("reidentify:release-1", 1));

    assert_eq!(keys(&fixture.list_candidates().await), vec![key.as_str()]);
    assert_eq!(
        fixture
            .get_candidate("reidentify:release-1")
            .await
            .expect_err("a re-identify run is not an import candidate")
            .kind(),
        "not_found"
    );
}

/// Every call is a read of the tables, so a candidate that changed reads
/// changed and one that is gone is gone — with no accumulated state that a
/// missed update could corrupt.
#[tokio::test]
async fn the_tables_are_the_answer() {
    let fixture = automation_over().await;
    let root = fixture.root();
    write_scan(&fixture, &root, |items| {
        items.push(ScanItem::Valid(candidate(&root, "A")));
        items.push(ScanItem::Valid(candidate(&root, "B")));
    })
    .await;

    fixture.skip(&format!("{root}/A")).await;
    write_scan(&fixture, &root, |items| {
        items.push(ScanItem::Valid(candidate(&root, "A")))
    })
    .await;

    assert_eq!(
        keys(&fixture.list_candidates().await),
        vec![format!("{root}/A").as_str()]
    );
    let candidate = fixture
        .get_candidate(&format!("{root}/A"))
        .await
        .expect("still scanned");
    assert!(
        candidate.common().skipped,
        "the stored decision is what is read"
    );
    assert_eq!(
        fixture
            .get_candidate(&format!("{root}/B"))
            .await
            .expect_err("a candidate the scan dropped is gone")
            .kind(),
        "not_found"
    );
}

/// What a candidate carries is the tables joined with what is in flight:
/// the row says an import is running, and the running attempt says how far.
#[tokio::test]
async fn a_candidate_carries_the_import_service_s_runtime() {
    let fixture = automation_over().await;
    let key = scan(&fixture, "A").await;
    fixture.record_and_settle(importing(&key, 42)).await;

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
