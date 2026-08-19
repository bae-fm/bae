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
    use super::*;

    fn candidate(key: &str) -> AutomationCandidate {
        AutomationCandidate::Valid {
            common: AutomationCandidateCommon {
                key: key.to_string(),
                path: key.to_string(),
                name: "Album".to_string(),
                watched_folder_path: "/music".to_string(),
                skipped: false,
                is_added: false,
                runtime: None,
            },
            track_count: 11,
            format_label: "FLAC".to_string(),
            content_hash: "hash".to_string(),
        }
    }

    fn state_holding(key: &str) -> AutomationState {
        let state = AutomationState::new();
        state
            .candidates
            .write()
            .expect("candidate index poisoned")
            .insert(key.to_string(), candidate(key));
        state
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

    /// A candidate with no runtime at all still resolves: its identify
    /// pipeline simply hasn't run, which is a state a caller may legitimately
    /// prefetch against.
    #[test]
    fn a_candidate_with_no_identify_evidence_still_resolves() {
        let state = state_holding("/music/Album");
        let found = state.get_candidate("/music/Album").expect("resolves");
        assert!(matches!(
            found,
            AutomationCandidate::Valid { ref common, .. } if common.runtime.is_none()
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
fn import_progress_step_and_phase_serialize_snake_case() {
    let preparing = automation_import_progress(ImportProgress::Preparing {
        import_id: "imp-1".to_string(),
        step: PrepareStep::ReadingFolder,
        album_title: "Album Title".to_string(),
        artist_name: "Artist Name".to_string(),
    });
    let json = serde_json::to_value(preparing).unwrap();
    assert_eq!(json["kind"], "preparing");
    assert_eq!(json["step"], "reading_folder");

    let progress = automation_import_progress(ImportProgress::Progress {
        id: "id-1".to_string(),
        percent: 50,
        phase: ImportPhase::ReadingFiles,
        import_id: "imp-1".to_string(),
    });
    let json = serde_json::to_value(progress).unwrap();
    assert_eq!(json["kind"], "progress");
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

/// The candidate index serves state, so it must start FROM the state: the
/// import scanner runs from bootstrap, while event indexing starts with the
/// automation surface — every candidate discovered in between is unknown to a
/// purely event-built index, and its first update latched the whole surface
/// `Failed`. Seeding from the candidate snapshot before pumping events makes
/// updates to pre-existing candidates ordinary; an unknown key after the seed
/// stays a loud failure, because then it is a real contradiction.
mod seeded_index {
    use super::*;
    use bae_core::import::folder_scanner::CategorizedFiles;
    use bae_core::import::ImportCandidatesSnapshot;
    use std::path::PathBuf;

    fn snapshot_with(path: &str) -> ImportCandidatesSnapshot {
        ImportCandidatesSnapshot {
            watched_folders: Vec::new(),
            folder_candidates: vec![bae_core::import::FolderImportCandidateSnapshot {
                candidate: bae_core::import::folder_scanner::FolderCandidate {
                    path: PathBuf::from(path),
                    file_root: PathBuf::from(path),
                    name: format!("Candidate {path}"),
                    files: CategorizedFiles {
                        files: Vec::new(),
                        format_label: "FLAC".to_string(),
                    },
                    watched_folder_path: "/music".to_string(),
                    scope: bae_core::import::folder_scanner::ReleaseFileScope::Recursive,
                    file_edit_revision: 0,
                    display_path: path.trim_start_matches('/').to_string(),
                    resolved_boundaries: Vec::new(),
                    combine_ancestor_key: None,
                },
                runtime: bae_core::import::CandidateRuntimeSnapshot {
                    identify_state: bae_core::identify::IdentifyState::Idle,
                    toolbar: Vec::new(),
                    signals: None,
                    import_status: None,
                },
                actionable: true,
                skipped: false,
                is_added: false,
            }],
            runtime_candidates: Vec::new(),
            invalid_candidates: Vec::new(),
            boundaries: Vec::new(),
            folder_scan_statuses: Vec::new(),
        }
    }

    #[test]
    fn an_update_to_a_seeded_candidate_keeps_the_surface_available() {
        let state = AutomationState::new();
        assert!(state.start_event_indexing());
        state.seed_candidates(snapshot_with("/music/A"));

        state.apply_event(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
            candidate_key: "/music/A".to_string(),
            skipped: true,
        }));

        assert!(
            matches!(state.event_indexing(), AutomationEventIndexing::Started),
            "an update to a seeded candidate must not fail the surface"
        );
        let candidate = state
            .get_candidate("/music/A")
            .expect("seeded candidate resolves");
        assert!(candidate.common().skipped, "the update landed on the seed");
    }

    #[test]
    fn an_update_to_an_unknown_candidate_still_fails_loud() {
        let state = AutomationState::new();
        assert!(state.start_event_indexing());
        state.seed_candidates(snapshot_with("/music/A"));

        state.apply_event(ImportEvent::Scan(ScanEvent::CandidateSkipChanged {
            candidate_key: "/music/NEVER-SEEN".to_string(),
            skipped: true,
        }));

        assert!(
            matches!(
                state.event_indexing(),
                AutomationEventIndexing::Failed { .. }
            ),
            "an unknown key after the seed is a real contradiction"
        );
    }
}
