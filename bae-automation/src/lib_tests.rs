use super::*;

#[test]
fn every_tool_input_schema_has_root_object_type() {
    for tool in AutomationTool::all() {
        let schema = tool.input_schema();
        assert_eq!(
            schema.get("type").and_then(Value::as_str),
            Some("object"),
            "tool {} inputSchema must have root type object",
            tool.name(),
        );
    }
}

/// A candidate key is resolved before anything is fetched for it, so a key
/// that names nothing fails as `not_found` instead of being answered.
///
/// This matters most on `import_release_prefetch`, which reads the claim
/// line's evidence off the named candidate. Core reads a key it has
/// recorded nothing against as "the pipeline hasn't run" — correct for a
/// scanned candidate awaiting identification, and indistinguishable from a
/// typo. Answered rather than refused, a typo returns a claim line that
/// reads as if the release had been found by searching: a wrong answer
/// that looks like a right one.
mod candidate_lookup {
    use super::import_queue::{automation_over, scan};
    use super::*;

    #[tokio::test]
    async fn a_known_key_resolves() {
        let fixture = automation_over().await;
        let key = scan(&fixture, "Album").await;

        let found = fixture
            .get_candidate(&key)
            .await
            .expect("a scanned candidate resolves");
        assert_eq!(found.path(), key);
    }

    /// Absence is not "no evidence yet" — it is a key that names nothing.
    #[tokio::test]
    async fn an_unknown_key_is_not_found_rather_than_empty_evidence() {
        let fixture = automation_over().await;
        scan(&fixture, "Album").await;

        let error = fixture
            .get_candidate("/music/Albmu")
            .await
            .expect_err("a key naming nothing must not be answered");
        assert_eq!(error.kind(), "not_found");
        assert!(
            error.message().contains("/music/Albmu"),
            "the error names the key that missed: {}",
            error.message()
        );
    }

    /// A candidate whose identify pipeline hasn't run still resolves: idle is a
    /// state a caller may legitimately prefetch against, distinct from a key
    /// that names nothing.
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

/// The MCP tool hands its edit over field-for-field — no editor, no shaping.
/// The rule the desktop's Save button enforces has to reach this path too, so
/// `release_metadata_update` can't write what the editor would refuse.
mod release_metadata_update_input {
    use super::*;

    fn edit(album_title: &str, album_artist_names: &[&str]) -> AutomationReleaseUserEdit {
        AutomationReleaseUserEdit {
            album_title: album_title.to_string(),
            album_artist_names: album_artist_names.iter().map(|s| s.to_string()).collect(),
            pressing: AutomationPressingEdit {
                year: None,
                format: None,
                label: None,
                catalog_number: None,
                country: None,
                barcode: None,
            },
            tracks: Vec::new(),
        }
    }

    #[test]
    fn an_empty_album_title_fails_the_edit_rule() {
        let wire = release_user_edit(edit("", &["The Beatles"]));
        assert_eq!(
            wire.validate(),
            Err(bae_core::import::EditValidationError::EmptyAlbumTitle),
        );
    }

    #[test]
    fn an_artist_less_edit_fails_the_edit_rule() {
        let wire = release_user_edit(edit("Abbey Road", &[]));
        assert_eq!(
            wire.validate(),
            Err(bae_core::import::EditValidationError::NoAlbumArtist),
        );
    }

    /// An untrimmed title is normalized, not refused — the desktop editor
    /// trims the same input rather than erroring on it.
    #[test]
    fn an_untrimmed_album_title_normalizes() {
        let wire = release_user_edit(edit("  Abbey Road  ", &["  The Beatles  "])).normalized();
        assert_eq!(wire.validate(), Ok(()));
        assert_eq!(wire.album_title, "Abbey Road");
        assert_eq!(wire.album_artist_names, vec!["The Beatles".to_string()]);
    }

    /// A refused edit reaches the client as `validation` — input it can fix —
    /// not as an opaque `import` failure.
    #[test]
    fn a_refused_edit_crosses_as_a_validation_error() {
        let error = AutomationError::from(LibraryError::Edit(
            bae_core::import::EditValidationError::EmptyAlbumTitle,
        ));
        assert_eq!(error.kind(), "validation");
        assert_eq!(error.message(), "Album title is required");
    }
}

/// The storage fields were open-world strings; they are now closed enums. The
/// JSON a client reads must not have moved.
#[test]
fn release_storage_state_and_actions_serialize_snake_case() {
    let summary = automation_release_summary(bae_core::album_detail::ReleaseSummary {
        id: "rel-1".to_string(),
        album_id: "alb-1".to_string(),
        format: Some("FLAC".to_string()),
        storage_state: ReleaseStorageState::Remote,
        pinned: true,
        storage_actions: vec![
            ReleaseStorageAction::Unpin,
            ReleaseStorageAction::MakeLocal,
            ReleaseStorageAction::MakeRemote,
            ReleaseStorageAction::Pin,
        ],
        transfer_action: Some(ReleaseStorageAction::MakeLocal),
        file_count: 2,
        total_size: 100,
        cover: None,
    });
    let json = serde_json::to_value(summary).unwrap();

    assert_eq!(json["storage_state"], "remote");
    assert_eq!(
        json["storage_actions"],
        serde_json::json!(["unpin", "make_local", "make_remote", "pin"]),
    );
    // The in-flight transition the desktop shows and MCP used to lose.
    assert_eq!(json["transfer_action"], "make_local");
}

#[test]
fn a_local_release_serializes_its_state_and_absent_transfer() {
    let summary = automation_release_summary(bae_core::album_detail::ReleaseSummary {
        id: "rel-2".to_string(),
        album_id: "alb-1".to_string(),
        format: None,
        storage_state: ReleaseStorageState::Local,
        pinned: false,
        storage_actions: Vec::new(),
        transfer_action: None,
        file_count: 0,
        total_size: 0,
        cover: None,
    });
    let json = serde_json::to_value(summary).unwrap();

    assert_eq!(json["storage_state"], "local");
    assert_eq!(json["transfer_action"], serde_json::Value::Null);
}

#[test]
fn import_step_and_phase_serialize_snake_case() {
    let preparing = automation_import_step(ImportStep::Preparing(PrepareStep::ReadingFolder));
    let json = serde_json::to_value(preparing).unwrap();
    assert_eq!(json["kind"], "preparing");
    assert_eq!(json["step"], "reading_folder");

    let running = automation_import_step(ImportStep::Running(ImportPhase::ReadingFiles));
    let json = serde_json::to_value(running).unwrap();
    assert_eq!(json["kind"], "running");
    assert_eq!(json["phase"], "reading_files");
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod identify_mirrors {
    use super::*;
    use bae_core::db::LibraryStatus;
    use bae_core::identify::combine::ResultProvenance;
    use bae_core::identify::state::SignalsContext;
    use bae_core::identify::{
        BarcodeProgress, DiscidProgress, IdentifyState, SignalKind, SignalRole, SignalState,
        ToolbarSignal,
    };
    use bae_core::import::search::MetadataResult;
    use bae_core::import::MetadataSource;
    use bae_core::signals::{
        BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue, TextSignal,
    };
    use std::collections::HashSet;

    fn metadata_result(release_id: &str, group_id: &str) -> MetadataResult {
        MetadataResult {
            source: MetadataSource::MusicBrainz,
            release_id: release_id.to_string(),
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1999),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-1".to_string()),
            country: Some("US".to_string()),
            cover_art: None,
            source_group_id: Some(group_id.to_string()),
            // Nobody asked the source for its tracklist: these fixtures
            // exercise provenance and pressing alignment, not the Ready rule.
            source_tracks: None,
        }
    }

    fn library_status(release_id: &str) -> LibraryStatus {
        LibraryStatus {
            release_id: release_id.to_string(),
            release_in_library: false,
            album_in_library: false,
            album_title: None,
            album_id: None,
        }
    }

    fn empty_context() -> SignalsContext {
        SignalsContext {
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            barcode_codes: Vec::new(),
            had_barcode_source: false,
            catalogs: Vec::new(),
            excluded: HashSet::new(),
            discid_results: Vec::new(),
            barcode_results: Vec::new(),
            discid_failure: None,
            barcode_failure: None,
            matched_barcode: None,
            track_count: 0,
        }
    }

    #[test]
    fn found_state_aligns_provenance_and_pressings_by_release_id() {
        let matches = vec![
            metadata_result("rel-1", "group-1"),
            metadata_result("rel-2", "group-1"),
        ];
        let state = IdentifyState::Found {
            matches: matches.clone(),
            library_statuses: vec![library_status("rel-1"), library_status("rel-2")],
            track_count: 12,
            provenance: vec![
                ResultProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    matches_catalog: false,
                },
                ResultProvenance {
                    by_disc_id: false,
                    by_barcode: true,
                    matches_catalog: true,
                },
            ],
            context: empty_context(),
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "found");
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 1, "both matches share one release group");
        let pressings = groups[0]["pressings"].as_array().unwrap();
        assert_eq!(pressings[0]["release_id"], "rel-1");
        assert_eq!(pressings[1]["release_id"], "rel-2");
        let provenance = json["provenance"].as_array().unwrap();
        assert_eq!(provenance[0]["release_id"], "rel-1");
        assert_eq!(provenance[0]["by_disc_id"], true);
        assert_eq!(provenance[1]["release_id"], "rel-2");
        assert_eq!(provenance[1]["by_barcode"], true);
        assert_eq!(provenance[1]["matches_catalog"], true);
        let statuses = json["library_statuses"].as_array().unwrap();
        assert_eq!(statuses[0]["release_id"], "rel-1");
        assert_eq!(statuses[1]["release_id"], "rel-2");
    }

    /// Signals that share no result still settle as one `Found`; the releases
    /// they each named land in their own group cards, and every row keeps the
    /// provenance saying which signal produced it.
    #[test]
    fn disagreeing_signals_become_one_found_over_several_groups() {
        let state = IdentifyState::Found {
            matches: vec![
                metadata_result("rel-disc", "g-d"),
                metadata_result("rel-bar", "g-b"),
            ],
            library_statuses: vec![library_status("rel-disc"), library_status("rel-bar")],
            track_count: 9,
            provenance: vec![
                ResultProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    matches_catalog: false,
                },
                ResultProvenance {
                    by_disc_id: false,
                    by_barcode: true,
                    matches_catalog: false,
                },
            ],
            context: empty_context(),
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "found");
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "the two releases are two release groups");
        assert_eq!(groups[0]["pressings"][0]["release_id"], "rel-disc");
        assert_eq!(groups[1]["pressings"][0]["release_id"], "rel-bar");
        let provenance = json["provenance"].as_array().unwrap();
        assert_eq!(provenance[0]["by_disc_id"], true);
        assert_eq!(provenance[1]["by_barcode"], true);
        assert_eq!(json["track_count"], 9);
    }

    #[test]
    fn triangulating_barcode_looking_up_drops_remaining() {
        let state = IdentifyState::Triangulating {
            discid: DiscidProgress::LookingUp,
            barcode: BarcodeProgress::LookingUp {
                current: "0123456789012".to_string(),
                position: 2,
                total: 3,
                remaining: vec!["9999999999999".to_string()],
            },
            context: empty_context(),
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "triangulating");
        assert_eq!(json["discid"]["kind"], "looking_up");
        assert_eq!(json["barcode"]["kind"], "looking_up");
        assert_eq!(json["barcode"]["current"], "0123456789012");
        assert_eq!(json["barcode"]["position"], 2);
        assert_eq!(json["barcode"]["total"], 3);
        assert!(json["barcode"].get("remaining").is_none());
    }

    #[test]
    fn toolbar_signal_maps_snake_case_and_structured_failure() {
        let signal = ToolbarSignal {
            kind: SignalKind::DiscId,
            role: SignalRole::Identity,
            value: Some("disc-hash".to_string()),
            origin: SignalOrigin::DiscToc,
            state: SignalState::Failed {
                failure: LookupFailure::Provider { status: Some(503) },
            },
            excluded: false,
        };

        let json = serde_json::to_value(automation_toolbar_signal(signal)).unwrap();
        assert_eq!(json["kind"], "disc_id");
        assert_eq!(json["role"], "identity");
        assert_eq!(json["origin"], "disc_toc");
        assert_eq!(json["state"]["kind"], "failed");
        assert_eq!(json["state"]["failure"]["kind"], "provider");
        assert_eq!(json["state"]["failure"]["status"], 503);
    }

    #[test]
    fn signals_map_all_three_subsignals() {
        let signals = Signals {
            disc_id: DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 10,
            },
            barcode: BarcodeSignal::Settled {
                codes: vec![SourcedValue::new(
                    "0123456789012".to_string(),
                    SignalOrigin::Artwork,
                )],
            },
            text: TextSignal::Settled {
                catalogs: vec![SourcedValue::new(
                    "CAT-1".to_string(),
                    SignalOrigin::CueSheet,
                )],
                free_text: vec!["Album Title".to_string()],
            },
            // A plausible total for the ten tracks above. Not zero, which
            // would claim the audio could not be probed.
            durations: bae_core::import::probe::ProbedDurations::totalling(2_400_000),
        };

        let json = serde_json::to_value(automation_signals(signals)).unwrap();
        assert_eq!(json["disc_id"]["kind"], "computed");
        assert_eq!(json["disc_id"]["disc_id"], "disc-hash");
        assert_eq!(json["disc_id"]["track_count"], 10);
        assert_eq!(json["barcode"]["kind"], "settled");
        assert_eq!(json["barcode"]["codes"][0]["value"], "0123456789012");
        assert_eq!(json["barcode"]["codes"][0]["origin"], "artwork");
        assert_eq!(json["text"]["kind"], "settled");
        assert_eq!(json["text"]["catalogs"][0]["value"], "CAT-1");
        assert_eq!(json["text"]["catalogs"][0]["origin"], "cue_sheet");
        assert_eq!(json["text"]["free_text"][0], "Album Title");
    }
}
/// The automation surface reads the import tables by key on every call: there
/// is no accumulated index behind it, so a key it has recorded nothing against
/// is a key that names nothing, and every class of key that used to latch an
/// index dead — candidates a boundary withdrew, `reidentify:` runs that name no
/// candidate at all — is simply a read that finds no row.
mod import_queue {
    use super::*;
    use bae_core::import::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, FolderCandidate, FolderReleaseBoundary,
        FolderReleaseDecisionKey, ReleaseFileScope, ScanItem, ScannedFile,
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

    fn candidate(root: &str, name: &str) -> FolderCandidate {
        FolderCandidate {
            path: PathBuf::from(format!("{root}/{name}")),
            file_root: PathBuf::from(format!("{root}/{name}")),
            name: name.to_string(),
            files: CategorizedFiles {
                files: vec![CandidateFile {
                    proposed_audio: true,
                    file: ScannedFile::new(
                        PathBuf::from(format!("{root}/{name}/01.flac")),
                        "01.flac".to_string(),
                        1_000,
                    ),
                    role: FileRole::Audio,
                }],
                format_label: "FLAC".to_string(),
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
                percent,
                phase: bae_core::import::ImportPhase::MeasuringLoudness,
                import_id: "import-1".to_string(),
            },
        }
    }

    /// The listing is what the import tab holds, in path order. A path a
    /// boundary withdrew is not a candidate: the boundary's write deleted the
    /// tentative row, so nothing this surface can act on stands at that key.
    #[tokio::test]
    async fn a_boundary_s_hidden_keys_are_absent_rather_than_a_contradiction() {
        let fixture = automation_over().await;
        let root = fixture.root();
        let hidden = format!("{root}/Box/CD1");
        write_scan(&fixture, &root, |items| {
            items.push(ScanItem::Valid(candidate(&root, "A")));
            items.push(ScanItem::Valid(candidate(&root, "B")));
            items.push(ScanItem::Discovered(candidate(&root, "Box/CD1")));
            items.push(ScanItem::Boundary(FolderReleaseBoundary {
                key: FolderReleaseDecisionKey {
                    watched_folder_path: root.clone(),
                    relative_folder_path: "Box".to_string(),
                },
                name: "Box".to_string(),
                display_path: "Box".to_string(),
                shared_file_count: 2,
                tree_rows: Vec::new(),
                candidate_keys: vec![hidden.clone()],
            }));
        })
        .await;

        let listed = fixture.list_candidates().await;
        assert_eq!(
            keys(&listed),
            [format!("{root}/A"), format!("{root}/B")]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
        let error = fixture
            .get_candidate(&hidden)
            .await
            .expect_err("a hidden path names no candidate a caller can act on");
        assert_eq!(error.kind(), "not_found");
        assert_eq!(
            fixture.list_candidates().await.len(),
            2,
            "the refused request left nothing latched"
        );
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
}

/// The storage tool is the scripted equivalent of the Storage Manager's row
/// menu, so what it accepts and what it refuses have to match that menu: the
/// wire shapes a caller sends, and core's own answer about which transitions a
/// release currently offers.
mod release_storage_action {
    use super::*;

    fn summary(actions: Vec<AutomationReleaseStorageAction>) -> AutomationReleaseSummary {
        AutomationReleaseSummary {
            id: "release-1".to_string(),
            album_id: "album-1".to_string(),
            format: Some("FLAC".to_string()),
            storage_state: AutomationReleaseStorageState::Remote,
            pinned: false,
            storage_actions: actions,
            transfer_action: None,
            file_count: 11,
            total_size: 320_000_000,
            cover: None,
        }
    }

    fn parse(args: Value) -> ReleaseStorageActionInput {
        from_value::<ReleaseStorageActionInput>(args).expect("the tool's own input shape")
    }

    /// Every action a caller can ask for arrives as its Storage Manager name,
    /// carrying whatever that transition needs — the pin choice, the folder.
    #[test]
    fn each_action_parses_from_its_wire_shape() {
        let moved = parse(serde_json::json!({
            "release_id": "release-1",
            "action": { "kind": "move_to_cloud", "pin": true },
        }));
        assert_eq!(moved.release_id, "release-1");
        assert!(matches!(
            moved.action,
            AutomationStorageAction::MoveToCloud { pin: true }
        ));

        assert!(matches!(
            parse(serde_json::json!({
                "release_id": "release-1",
                "action": { "kind": "make_local", "destination_dir": "/music/out" },
            }))
            .action,
            AutomationStorageAction::MakeLocal { destination_dir } if destination_dir == "/music/out"
        ));

        for (kind, expected) in [
            ("pin", AutomationStorageAction::Pin),
            ("unpin", AutomationStorageAction::Unpin),
            ("cancel", AutomationStorageAction::Cancel),
        ] {
            let parsed = parse(serde_json::json!({
                "release_id": "release-1",
                "action": { "kind": kind },
            }));
            assert_eq!(
                std::mem::discriminant(&parsed.action),
                std::mem::discriminant(&expected),
                "'{kind}' names its action"
            );
        }
    }

    /// Moving to the cloud needs the pin choice and making local needs a folder;
    /// neither has a default this tool is entitled to invent.
    #[test]
    fn an_action_missing_what_it_needs_is_refused() {
        for args in [
            serde_json::json!({
                "release_id": "release-1",
                "action": { "kind": "move_to_cloud" },
            }),
            serde_json::json!({
                "release_id": "release-1",
                "action": { "kind": "make_local" },
            }),
        ] {
            let error = from_value::<ReleaseStorageActionInput>(args)
                .expect_err("an action without its required field is not an action");
            assert_eq!(error.kind(), "validation");
        }
    }

    /// The gate is core's list, not this tool's opinion: a transition core
    /// offers runs, and one it doesn't is refused before any transfer starts.
    #[test]
    fn only_the_transitions_core_offers_are_run() {
        let pinnable = summary(vec![
            AutomationReleaseStorageAction::Pin,
            AutomationReleaseStorageAction::MakeLocal,
        ]);

        require_action(&pinnable, AutomationReleaseStorageAction::Pin, "pin")
            .expect("core offers the pin");

        let error = require_action(&pinnable, AutomationReleaseStorageAction::Unpin, "unpin")
            .expect_err("core does not offer the unpin");
        assert_eq!(error.kind(), "validation");
        assert!(
            error.message().contains("pin, make_local"),
            "the refusal names what the release does offer: {}",
            error.message()
        );
    }

    /// A library with no cloud home offers no transitions at all. The refusal
    /// says so rather than listing an empty set.
    #[test]
    fn a_release_with_no_transitions_says_why() {
        let error = require_action(
            &summary(Vec::new()),
            AutomationReleaseStorageAction::MakeRemote,
            "move to cloud",
        )
        .expect_err("a library with no cloud home cannot move anything to it");
        assert!(
            error.message().contains("no cloud home"),
            "unexpected refusal: {}",
            error.message()
        );
    }

    /// A move to the cloud reports the durable revision its uploads were queued
    /// at — the thing a caller waits on — not a bare acknowledgement.
    #[test]
    fn the_outcome_carries_what_the_transition_produced() {
        let json = serde_json::to_value(AutomationStorageActionOutcome::CloudUploadQueued {
            release_id: "release-1".to_string(),
            outbox_revision: 42,
        })
        .unwrap();
        assert_eq!(json["kind"], "cloud_upload_queued");
        assert_eq!(json["release_id"], "release-1");
        assert_eq!(json["outbox_revision"], 42);

        let json = serde_json::to_value(AutomationStorageActionOutcome::PinQueued {
            release_id: "release-1".to_string(),
        })
        .unwrap();
        assert_eq!(json["kind"], "pin_queued");
    }

    /// The tool is reachable by the name its schema is published under.
    #[test]
    fn the_tool_dispatches_by_name() {
        assert_eq!(
            AutomationTool::from_name("release_storage_action"),
            Some(AutomationTool::ReleaseStorageAction)
        );
        assert!(!AutomationTool::ReleaseStorageAction.accepts_missing_arguments());
    }
}
