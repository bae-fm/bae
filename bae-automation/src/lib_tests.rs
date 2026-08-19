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
    use super::snapshot_mirror::{folder_candidate, snapshot, state_over};
    use super::*;

    fn state_holding(key: &str) -> AutomationState {
        state_over(snapshot(
            vec![folder_candidate(key)],
            Vec::new(),
            Vec::new(),
        ))
        .1
    }

    #[test]
    fn a_known_key_resolves() {
        let state = state_holding("/music/Album");
        let found = state
            .get_candidate("/music/Album")
            .expect("a scanned candidate resolves");
        assert_eq!(found.path(), "/music/Album");
    }

    /// Absence is not "no evidence yet" — it is a key that names nothing.
    #[test]
    fn an_unknown_key_is_not_found_rather_than_empty_evidence() {
        let state = state_holding("/music/Album");
        let error = state
            .get_candidate("/music/Albmu")
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
    #[test]
    fn a_candidate_with_no_identify_evidence_still_resolves() {
        let state = state_holding("/music/Album");
        let found = state.get_candidate("/music/Album").expect("resolves");
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
    use bae_core::identify::combine::{GroupKey, ResultProvenance};
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
            group: GroupKey {
                source: MetadataSource::MusicBrainz,
                source_group_id: "group-1".to_string(),
            },
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
        let pressings = json["group"]["pressings"].as_array().unwrap();
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

    #[test]
    fn conflict_state_splits_results_and_statuses_per_signal() {
        let mut context = empty_context();
        context.discid_results = vec![(
            metadata_result("rel-disc", "g-d"),
            library_status("rel-disc"),
        )];
        context.barcode_results =
            vec![(metadata_result("rel-bar", "g-b"), library_status("rel-bar"))];
        context.matched_barcode = Some("0123456789012".to_string());
        context.track_count = 9;
        let state = IdentifyState::Conflict { context };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "conflict");
        assert_eq!(json["discid_results"][0]["release_id"], "rel-disc");
        assert_eq!(json["discid_library_statuses"][0]["release_id"], "rel-disc");
        assert_eq!(json["barcode_results"][0]["release_id"], "rel-bar");
        assert_eq!(json["barcode_library_statuses"][0]["release_id"], "rel-bar");
        assert_eq!(json["matched_barcode"], "0123456789012");
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
            probed_total_duration_ms: 2_400_000,
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

/// The surface mirrors the import service's published candidate state, so what
/// it lists is whatever that state currently says — and a key the state does not
/// carry is simply absent, never a contradiction to latch on. The event-built
/// index had to classify every update it could not place, and each class it had
/// not thought of (candidates discovered before it subscribed, paths a
/// `FolderReleaseBoundary` hides) took the whole surface down until it was
/// special-cased.
mod snapshot_mirror {
    use super::*;
    use bae_core::identify::IdentifyState;
    use bae_core::import::folder_scanner::{
        CategorizedFiles, FolderCandidate, FolderReleaseBoundary, FolderReleaseDecisionKey,
        InvalidCandidate, InvalidReason, ReleaseFileScope,
    };
    use bae_core::import::{
        CandidateImportStatusSnapshot, CandidateRuntimeSnapshot, FolderImportCandidateSnapshot,
        ImportCandidatesSnapshot, ImportStep, ImportedRelease, RuntimeImportCandidateSnapshot,
    };
    use std::path::PathBuf;

    pub(super) fn idle_runtime() -> CandidateRuntimeSnapshot {
        CandidateRuntimeSnapshot {
            identify_state: IdentifyState::Idle,
            toolbar: Vec::new(),
            signals: None,
            import_status: None,
        }
    }

    pub(super) fn folder_candidate(path: &str) -> FolderImportCandidateSnapshot {
        FolderImportCandidateSnapshot {
            candidate: FolderCandidate {
                path: PathBuf::from(path),
                file_root: PathBuf::from(path),
                name: format!("Candidate {path}"),
                files: CategorizedFiles {
                    files: Vec::new(),
                    format_label: "FLAC".to_string(),
                },
                watched_folder_path: "/music".to_string(),
                scope: ReleaseFileScope::Recursive,
                file_edit_revision: 0,
                display_path: path.trim_start_matches('/').to_string(),
                resolved_boundaries: Vec::new(),
                combine_ancestor_key: None,
            },
            runtime: idle_runtime(),
            actionable: true,
            skipped: false,
            is_added: false,
        }
    }

    fn invalid_candidate(path: &str) -> InvalidCandidate {
        InvalidCandidate {
            path: PathBuf::from(path),
            name: format!("Invalid {path}"),
            watched_folder_path: "/music".to_string(),
            display_path: path.trim_start_matches('/').to_string(),
            resolved_boundaries: Vec::new(),
            reason: InvalidReason::NoValidAudio,
        }
    }

    /// A boundary hiding `candidate_keys`: tentative paths the import service
    /// has withdrawn from its candidate list while the user decides whether the
    /// folder is one release or several. Runtime updates still arrive for them.
    fn boundary(hidden: &[&str]) -> FolderReleaseBoundary {
        FolderReleaseBoundary {
            key: FolderReleaseDecisionKey {
                watched_folder_path: "/music".to_string(),
                relative_folder_path: "Between The Buttons".to_string(),
            },
            name: "Between The Buttons".to_string(),
            display_path: "Between The Buttons".to_string(),
            shared_file_count: 2,
            tree_rows: Vec::new(),
            candidate_keys: hidden.iter().map(|key| key.to_string()).collect(),
        }
    }

    pub(super) fn snapshot(
        folder_candidates: Vec<FolderImportCandidateSnapshot>,
        invalid_candidates: Vec<InvalidCandidate>,
        boundaries: Vec<FolderReleaseBoundary>,
    ) -> ImportCandidatesSnapshot {
        ImportCandidatesSnapshot {
            watched_folders: Vec::new(),
            folder_candidates,
            runtime_candidates: Vec::new(),
            invalid_candidates,
            boundaries,
            folder_scan_statuses: Vec::new(),
        }
    }

    /// A state reading one watch, returned with the sender so a test can publish
    /// the next snapshot — which is how every update reaches this surface.
    pub(super) fn state_over(
        snapshot: ImportCandidatesSnapshot,
    ) -> (watch::Sender<ImportCandidatesSnapshot>, AutomationState) {
        let (tx, rx) = watch::channel(snapshot);
        (tx, AutomationState::new(rx))
    }

    fn keys(candidates: &[AutomationCandidate]) -> Vec<&str> {
        candidates.iter().map(AutomationCandidate::key).collect()
    }

    /// The listing is the snapshot's folder and invalid candidates, in path
    /// order. Paths a boundary hides are not candidates: the import service is
    /// not publishing them, so this surface does not invent them — and does not
    /// treat their existence in `candidate_keys` as an inconsistency either.
    #[test]
    fn a_boundary_s_hidden_keys_are_absent_rather_than_a_contradiction() {
        let hidden = "/music/Between The Buttons US [Polydor P25L 25040]";
        let (_tx, state) = state_over(snapshot(
            vec![folder_candidate("/music/B"), folder_candidate("/music/A")],
            vec![invalid_candidate("/music/C")],
            vec![boundary(&[hidden])],
        ));

        assert_eq!(
            keys(&state.list_candidates()),
            vec!["/music/A", "/music/B", "/music/C"]
        );
        let error = state
            .get_candidate(hidden)
            .expect_err("a hidden path names no candidate a caller can act on");
        assert_eq!(error.kind(), "not_found");
        assert_eq!(
            keys(&state.list_candidates()),
            vec!["/music/A", "/music/B", "/music/C"],
            "the refused request left nothing latched"
        );
    }

    /// `reidentify:` runtime entries name releases, not scanned folders. They
    /// carry runtime but no candidate, so they are not listed — and, unlike the
    /// event index, their updates are not an unknown key to fail on.
    #[test]
    fn runtime_only_entries_are_not_candidates() {
        let mut snapshot = snapshot(vec![folder_candidate("/music/A")], Vec::new(), Vec::new());
        snapshot.runtime_candidates = vec![RuntimeImportCandidateSnapshot {
            key: "reidentify:release-1".to_string(),
            runtime: idle_runtime(),
        }];
        let (_tx, state) = state_over(snapshot);

        assert_eq!(keys(&state.list_candidates()), vec!["/music/A"]);
        assert_eq!(
            state
                .get_candidate("reidentify:release-1")
                .expect_err("a re-identify run is not an import candidate")
                .kind(),
            "not_found"
        );
    }

    /// Updates arrive as a new snapshot value. The surface reads the latest one,
    /// so a candidate that changed reads changed and one that is gone is gone —
    /// with no accumulated state that a missed or unplaceable update could
    /// corrupt.
    #[test]
    fn the_latest_snapshot_is_the_answer() {
        let (tx, state) = state_over(snapshot(
            vec![folder_candidate("/music/A"), folder_candidate("/music/B")],
            Vec::new(),
            Vec::new(),
        ));

        let mut skipped = folder_candidate("/music/A");
        skipped.skipped = true;
        skipped.runtime.import_status = Some(CandidateImportStatusSnapshot::Complete {
            release: ImportedRelease {
                release_id: "release-1".to_string(),
                album_id: "album-1".to_string(),
            },
        });
        tx.send(snapshot(vec![skipped], Vec::new(), Vec::new()))
            .expect("the state holds the receiver");

        assert_eq!(keys(&state.list_candidates()), vec!["/music/A"]);
        let candidate = state.get_candidate("/music/A").expect("still published");
        assert!(candidate.common().skipped, "the new value is what is read");
        assert!(matches!(
            candidate,
            AutomationCandidate::Valid { ref runtime, .. }
                if matches!(
                    runtime.import_status,
                    Some(AutomationImportStatus::Complete { ref release_id, .. })
                        if release_id == "release-1"
                )
        ));
        assert_eq!(
            state
                .get_candidate("/music/B")
                .expect_err("a candidate the service dropped is gone")
                .kind(),
            "not_found"
        );
    }

    /// The runtime a candidate carries is the import service's own, converted
    /// whole: identify state, toolbar, signals, and the candidate's import run.
    #[test]
    fn a_candidate_carries_the_import_service_s_runtime() {
        let mut running = folder_candidate("/music/A");
        running.runtime.import_status = Some(CandidateImportStatusSnapshot::Importing {
            progress_percent: 42,
            step: Some(ImportStep::Running(
                bae_core::import::ImportPhase::MeasuringLoudness,
            )),
        });
        let (_tx, state) = state_over(snapshot(vec![running], Vec::new(), Vec::new()));

        let candidate = state.get_candidate("/music/A").expect("published");
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
