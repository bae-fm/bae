//! The automation calls against a real library in a temporary directory.
//! Every call reads the import tables by key, so a key with nothing recorded
//! names nothing.

use super::*;
use bae_import::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderCandidate, ReleaseFileScope, ScanItem,
    ScannedFile,
};
use bae_import::{ImportEvent, ImportProgress};
use std::path::PathBuf;

/// The scan wait's limit: only a read that never ends reaches it.
fn hang_guard_ms() -> u64 {
    u64::try_from(bae_util::test_channels::HANG_GUARD.as_millis()).unwrap()
}

/// An `Automation` over a real library in a temporary directory, keeping the
/// import tables and services so tests can write the scan rows and runtime it
/// reads.
pub(super) struct Fixture {
    automation: Automation,
    tables: bae_import::tables::ImportTables,
    services: AppServices,
    tmp: tempfile::TempDir,
}

impl Fixture {
    pub(super) fn automation(&self) -> &Automation {
        &self.automation
    }

    pub(super) async fn list_candidates(&self) -> Vec<AutomationCandidate> {
        self.automation.list_candidates().await.expect("the list reads")
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
    let library = bae_library::test_libraries::open_test_library(tmp.path()).await;
    let tables = bae_import::tables::ImportTables::new(library.store().clone());
    let services = bae_app::test_services::start_services(&library)
        .await
        .expect("the test library's services start");
    let automation = Automation::new(services.clone());
    Fixture { automation, tables, services, tmp }
}

#[tokio::test]
async fn watched_folder_tool_reads_current_store_rows() {
    let fixture = automation_over().await;
    let root = fixture.root();
    for watched in [false, true, false] {
        if watched {
            fixture.tables.add_watched_import_folder(&root).await.unwrap();
        } else {
            fixture.tables.remove_watched_import_folders(vec![root.clone()], None).await.unwrap();
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

/// The folders `watched_folders_list` names, as `(key, path)`.
async fn listed_watched_folders(automation: &Automation) -> Vec<(String, String)> {
    let response = automation
        .call_tool(AutomationTool::WatchedFoldersList, serde_json::json!({}))
        .await
        .expect("the list reads");
    response["watched_folders"]
        .as_array()
        .expect("the list holds the folders")
        .iter()
        .map(|folder| {
            (
                folder["key"].as_str().unwrap().to_string(),
                folder["path"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// A watched folder is listed with the hex of its path's bytes as its key,
/// and `watched_folder_remove` takes that key.
#[tokio::test]
async fn a_watched_folder_is_removed_by_the_key_its_listing_gives() {
    let fixture = automation_over().await;
    let root = fixture.root();
    fixture.tables.add_watched_import_folder(&root).await.unwrap();

    let listed = listed_watched_folders(&fixture.automation).await;
    let [(key, path)] = listed.as_slice() else {
        panic!("the list names the folder: {listed:?}");
    };
    assert_eq!(*key, hex::encode(coven::path_bytes(&root)));
    assert_eq!(*path, coven::path_text(&root));

    let response = fixture
        .automation
        .call_tool(AutomationTool::WatchedFolderRemove, serde_json::json!({ "key": key }))
        .await
        .expect("the key names the folder");
    assert_eq!(response["watched_folders"], serde_json::json!([]));
    assert!(listed_watched_folders(&fixture.automation).await.is_empty());
}

/// A key that isn't the hex of a path's bytes is refused before anything is
/// removed.
#[tokio::test]
async fn a_watched_folder_key_that_is_not_hex_is_refused() {
    let fixture = automation_over().await;
    let root = fixture.root();
    fixture.tables.add_watched_import_folder(&root).await.unwrap();

    let error = fixture
        .automation
        .call_tool(AutomationTool::WatchedFolderRemove, serde_json::json!({ "key": "not hex" }))
        .await
        .expect_err("the key names no path");
    assert_eq!(error.kind(), "validation", "{}", error.message());
    assert_eq!(listed_watched_folders(&fixture.automation).await.len(), 1);
}

/// A folder whose name isn't UTF-8 is listed under its escaped name, which
/// names no folder, and removed by its key, which does.
#[cfg(unix)]
#[tokio::test]
async fn a_watched_folder_whose_name_is_not_utf8_is_removed_by_its_key() {
    use std::os::unix::ffi::OsStrExt;

    let fixture = automation_over().await;
    let root = fixture.tmp.path().join(std::ffi::OsStr::from_bytes(b"Caf\xE9"));
    fixture.tables.add_watched_import_folder(&root).await.unwrap();

    let listed = listed_watched_folders(&fixture.automation).await;
    let [(key, path)] = listed.as_slice() else {
        panic!("the list names the folder: {listed:?}");
    };
    assert!(path.ends_with(r"/Caf\xE9"), "{path}");
    assert_eq!(*key, hex::encode(root.as_os_str().as_bytes()));

    fixture
        .automation
        .call_tool(AutomationTool::WatchedFolderRemove, serde_json::json!({ "key": key }))
        .await
        .expect("the key names the folder");
    assert!(listed_watched_folders(&fixture.automation).await.is_empty());
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
                    file.source_audio = Some(bae_import::folder_scanner::ScannedAudio {
                        content_type: bae_util::content_type::ContentType::Flac,
                        container: bae_util::audio_container::AudioContainer::Flac,
                        duration_ms: 1_000,
                        format: bae_types::audio_format::AudioFormat {
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
        relative_folder: bae_import::RelativeFolder::of(std::path::Path::new(name)).unwrap(),
        grouping: None,
    }
}

/// Store a scan of the watched root holding only the candidate `name`, and
/// return the key automation names it by.
pub(super) async fn scan(fixture: &Fixture, name: &str) -> String {
    let root = fixture.root();
    write_scan(fixture, &root, |items| items.push(ScanItem::Valid(candidate(&root, name)))).await;
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
    let tables = &fixture.tables;
    tables.add_watched_import_folder(root).await.expect("the root is watched");
    let generation = tables
        .begin_folder_scan(root, bae_import::VolumeKind::Local)
        .await
        .expect("a scan generation opens");
    let mut items = Vec::new();
    build(&mut items);
    for item in &items {
        bae_import::store_scan_item(
            tables,
            &bae_diagnostics::Diagnostics::noop(),
            root,
            generation,
            item,
        )
        .await
        .expect("the scan item persists");
    }
    tables.finish_folder_scan(root, generation, None).await.expect("the scan finishes");
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
            phase: bae_import::ImportPhase::MeasuringLoudness,
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
    write_scan(&fixture, &root, |items| items.push(ScanItem::Valid(candidate(&root, "A")))).await;

    assert_eq!(keys(&fixture.list_candidates().await), vec![key_of(&root, "A").as_str()]);
    let candidate = fixture.get_candidate(&key_of(&root, "A")).await.expect("still scanned");
    assert!(candidate.common().skipped, "the stored decision is what is read");
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
    // The worker takes the import up and reports its preparation, writes the
    // step it starts, then reports how far through it it has got.
    fixture.record(ImportEvent::ImportProgress {
        candidate_key: candidate_key_named(&key).unwrap(),
        progress: ImportProgress::Preparing {
            import_id: IMPORT_ID.to_string(),
            step: bae_import::PrepareStep::ValidatingSourceFiles,
        },
    });
    fixture
        .services
        .move_candidate_import_for_test(
            &candidate_key_named(&key).unwrap(),
            IMPORT_ID,
            bae_import::ImportStep::Running(bae_import::ImportPhase::MeasuringLoudness),
        )
        .await;
    fixture.record(importing(&key, 42));

    let candidate = fixture.get_candidate(&key).await.expect("published");
    let json = serde_json::to_value(&candidate).unwrap();
    assert_eq!(json["runtime"]["import_status"]["kind"], "importing");
    assert_eq!(json["runtime"]["import_status"]["progress_percent"], 42);
    assert_eq!(json["runtime"]["import_status"]["step"]["kind"], "running");
    assert_eq!(json["runtime"]["import_status"]["step"]["phase"], "measuring_loudness");
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
        bae_import::test_fixtures::write_tagged_flac(&album, "01 Track.flac", "Track");
        fixture
            .automation
            .add_watched_folder(root.to_string_lossy().into_owned())
            .await
            .expect("the folder is watched");
    }

    let result = fixture
        .automation
        .scan_watched_folders(ScanWait::UntilFinished { timeout_ms: hang_guard_ms() })
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
    fixture.automation.add_watched_folder(path.clone()).await.expect("the folder is watched");
    std::fs::remove_dir_all(&root).unwrap();

    let error = fixture
        .automation
        .scan_watched_folders(ScanWait::UntilFinished { timeout_ms: hang_guard_ms() })
        .await
        .expect_err("the read of a folder that is gone fails");
    assert!(error.message().contains(&path), "{}", error.message());
}

/// A candidate key that names nothing fails as `not_found` before anything is
/// fetched, so a typo never reads as a candidate awaiting identification.
mod candidate_lookup {
    use super::*;

    #[tokio::test]
    async fn a_known_key_resolves() {
        let fixture = automation_over().await;
        let key = scan(&fixture, "Album").await;

        let found = fixture.get_candidate(&key).await.expect("a scanned candidate resolves");
        assert_eq!(found.key(), key);
        assert_eq!(
            found.common().source_folders,
            vec![coven::path_text(&fixture.root().join("Album"))]
        );
    }

    /// Absence is not "no evidence yet" — it is a key that names nothing.
    #[tokio::test]
    async fn an_unknown_key_is_not_found_rather_than_empty_evidence() {
        let fixture = automation_over().await;
        scan(&fixture, "Album").await;

        let missing = candidate_key_name(&CandidateKey::folder(&fixture.root().join("Albmu")));
        let error = fixture
            .get_candidate(&missing)
            .await
            .expect_err("a key naming nothing must not be answered");
        assert_eq!(error.kind(), "not_found");
        assert!(
            error.message().contains(&missing),
            "the error names the key that missed: {}",
            error.message()
        );
    }

    /// A name that is not the hex of a key's bytes names no key at all.
    #[tokio::test]
    async fn a_name_that_is_not_hex_is_refused() {
        let fixture = automation_over().await;
        let error =
            fixture.get_candidate("/music/Album").await.expect_err("a path is not a key's name");
        assert_eq!(error.kind(), "validation");
    }

    /// A candidate whose identification hasn't run still resolves.
    #[tokio::test]
    async fn a_candidate_with_no_identify_evidence_still_resolves() {
        let fixture = automation_over().await;
        let key = scan(&fixture, "Album").await;

        let found = fixture.get_candidate(&key).await.expect("resolves");
        assert!(matches!(
            found,
            AutomationCandidate::Valid { ref runtime, .. }
                if matches!(runtime.identify_state, AutomationIdentifyState::Idle)
                    && runtime.import_status.is_none()
        ));
    }
}

/// MCP files a library release under a library album as the editor's album
/// picker does: the release moves in and the album keeps its own title.
mod release_album {
    use super::*;

    /// Write one tagged fixture track into `dir`.
    fn one_track(dir: &std::path::Path) {
        bae_import::test_fixtures::write_tagged_flac(dir, "01 Track One.flac", "Track One");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn release_album_set_moves_the_release_into_the_album_it_names() {
        let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
        let (library, imported) = bae_library::test_libraries::imported_release_setup_with(
            &fakes,
            bae_catalogs::test_releases::discogs_test_release(
                "album-set-moved",
                "Moved Album",
                &[("Track One", "0:10")],
            ),
            "moved",
            "album-set-moved-import".to_string(),
            one_track,
        )
        .await
        .expect("import the release that moves");
        let picked_dir = imported.temp_dir.path().join("picked");
        std::fs::create_dir_all(&picked_dir).expect("the second album's folder");
        one_track(&picked_dir);
        let picked_key =
            fakes.seed_discogs_test_release(bae_catalogs::test_releases::discogs_test_release(
                "album-set-picked",
                "Picked Album",
                &[("Track One", "0:10")],
            ));
        let picked_release = library.import_folder(&picked_dir, picked_key).await;
        let picked_album = library
            .manager()
            .get_album_id_for_release(&picked_release)
            .await
            .expect("the picked album");
        let moved_from = library
            .manager()
            .get_album_id_for_release(&imported.release_id)
            .await
            .expect("the release's own album");
        assert_ne!(moved_from, picked_album);
        let services = bae_app::test_services::start_services(&library)
            .await
            .expect("the test library's services start");
        let automation = Automation::new(services);

        automation
            .call_tool(
                AutomationTool::from_name("release_album_set").expect("the tool is published"),
                serde_json::json!({
                    "release_id": imported.release_id,
                    "album_id": picked_album,
                }),
            )
            .await
            .expect("the release moves");

        assert_eq!(
            library.manager().get_album_id_for_release(&imported.release_id).await.unwrap(),
            picked_album
        );
        assert_eq!(
            library
                .manager()
                .get_album_by_id(&picked_album)
                .await
                .unwrap()
                .expect("the picked album stays")
                .title,
            "Picked Album",
            "the album keeps its own title"
        );
        let refused = automation
            .call_tool(
                AutomationTool::ReleaseAlbumSet,
                serde_json::json!({
                    "release_id": imported.release_id,
                    "album_id": "no-such-album",
                }),
            )
            .await
            .expect_err("an album the library does not hold cannot be picked");
        assert_eq!(refused.kind(), "not_found");
    }
}

/// `import_search` asks one source, and only a catalog that answers searches
/// can be named as it.
mod import_search_source {
    use super::*;

    #[tokio::test]
    async fn a_catalog_that_answers_no_searches_is_refused() {
        let fixture = automation_over().await;
        let refused = fixture
            .automation()
            .call_tool(
                AutomationTool::ImportSearch,
                serde_json::json!({
                    "kind": "general",
                    "artist": "Artist Name",
                    "album": "Album Title",
                    "source": "spotify",
                }),
            )
            .await
            .expect_err("a catalog that answers no searches cannot be asked one");
        assert_eq!(refused.kind(), "validation");
    }

    /// The tool's schema offers the catalogs a search can be asked of, and
    /// no other.
    #[test]
    fn the_schema_offers_only_the_catalogs_that_answer_searches() {
        let schema = Value::Object(AutomationTool::ImportSearch.input_schema()).to_string();
        for asked in ["music_brainz", "discogs"] {
            assert!(schema.contains(&format!("\"{asked}\"")), "{asked} is offered: {schema}");
        }
        for linked in ["spotify", "wikidata", "all_music"] {
            assert!(
                !schema.contains(&format!("\"{linked}\"")),
                "{linked} is not offered: {schema}"
            );
        }
    }
}

/// A pick names the release a candidate's draft is read from, and only a
/// catalog bae fetches releases from can be named as it.
mod pick_record_catalog {
    use super::*;

    #[tokio::test]
    async fn a_release_on_a_catalog_bae_does_not_fetch_is_refused() {
        let fixture = automation_over().await;
        let key = scan(&fixture, "Album").await;
        let refused = fixture
            .automation()
            .call_tool(
                AutomationTool::ImportCandidateReleaseSelect,
                serde_json::json!({
                    "candidate_key": key,
                    "link": {
                        "record": { "catalog": "spotify", "key": "4242424242424242424242" },
                        "partners": [],
                    },
                }),
            )
            .await
            .expect_err("a release bae cannot fetch cannot be picked");
        assert_eq!(refused.kind(), "validation");
    }
}
