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
        assert_eq!(found.common().source_folders, vec![key]);
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

    fn edit(album_title: &str, album_artist_seed_names: &[&str]) -> AutomationReleaseUserEdit {
        AutomationReleaseUserEdit {
            album_title: album_title.to_string(),
            album_artist_assignments: album_artist_seed_names
                .iter()
                .map(|name| AutomationArtistAssignment::New {
                    seed: AutomationNewArtistSeed {
                        name: (*name).to_string(),
                        sort_name: None,
                        musicbrainz_artist_id: None,
                        discogs_artist_id: None,
                    },
                })
                .collect(),
            album_year: None,
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
        let wire = edit("", &["Artist Alpha"]).into_core();
        assert_eq!(
            wire.validate(),
            Err(bae_core::import::EditValidationError::EmptyAlbumTitle),
        );
    }

    #[test]
    fn an_artist_less_edit_fails_the_edit_rule() {
        let wire = edit("Album Alpha", &[]).into_core();
        assert_eq!(
            wire.validate(),
            Err(bae_core::import::EditValidationError::NoAlbumArtist),
        );
    }

    /// An untrimmed title is normalized, not refused — the desktop editor
    /// trims the same input rather than erroring on it.
    #[test]
    fn an_untrimmed_album_title_normalizes() {
        let wire = edit("  Album Alpha  ", &["  Artist Alpha  "])
            .into_core()
            .normalized();
        assert_eq!(wire.validate(), Ok(()));
        assert_eq!(wire.album_title, "Album Alpha");
        assert_eq!(
            wire.album_artist_assignments,
            vec![bae_core::import::ArtistAssignment::new("Artist Alpha")]
        );
    }

    #[test]
    fn an_existing_artist_keeps_its_library_identity() {
        let artist = AutomationExistingArtist {
            artist_id: "00000000-0000-0000-0000-000000000001".to_string(),
            name: "Artist Alpha".to_string(),
            sort_name: Some("Alpha, Artist".to_string()),
            musicbrainz_artist_id: Some("musicbrainz-artist".to_string()),
            discogs_artist_id: Some("discogs-artist".to_string()),
        };
        let mut edit = edit("Album Alpha", &[]);
        edit.album_artist_assignments = vec![AutomationArtistAssignment::Existing {
            artist: artist.clone(),
        }];

        let round_trip = AutomationReleaseUserEdit::from_core(edit.into_core());
        let AutomationArtistAssignment::Existing {
            artist: round_trip_artist,
        } = &round_trip.album_artist_assignments[0]
        else {
            panic!("the existing artist remains an existing artist");
        };
        assert_eq!(round_trip_artist.artist_id, artist.artist_id);
        assert_eq!(round_trip_artist.name, artist.name);
        assert_eq!(round_trip_artist.sort_name, artist.sort_name);
        assert_eq!(
            round_trip_artist.musicbrainz_artist_id,
            artist.musicbrainz_artist_id
        );
        assert_eq!(
            round_trip_artist.discogs_artist_id,
            artist.discogs_artist_id
        );
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
    let summary = AutomationReleaseSummary::from_core(bae_core::album_detail::ReleaseSummary {
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
    let summary = AutomationReleaseSummary::from_core(bae_core::album_detail::ReleaseSummary {
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
    let preparing =
        AutomationImportStep::from_core(ImportStep::Preparing(PrepareStep::ValidatingSourceFiles));
    let json = serde_json::to_value(preparing).unwrap();
    assert_eq!(json["kind"], "preparing");
    assert_eq!(json["step"], "validating_source_files");

    let running = AutomationImportStep::from_core(ImportStep::Running(ImportPhase::ReadingFiles));
    let json = serde_json::to_value(running).unwrap();
    assert_eq!(json["kind"], "running");
    assert_eq!(json["phase"], "reading_files");
}

#[test]
fn indeterminate_import_progress_serializes_without_a_fraction() {
    let status = automation_import_status(
        Some(&TriageImportStatus::Importing),
        Some(&ImportInFlight {
            progress_percent: None,
            step: Some(ImportStep::Preparing(PrepareStep::ValidatingSourceFiles)),
        }),
    )
    .expect("the importing row has a status");

    let json = serde_json::to_value(status).unwrap();
    assert_eq!(json["progress_percent"], serde_json::Value::Null);
    assert_eq!(json["step"]["step"], "validating_source_files");
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod identify_mirrors {
    use super::*;
    use bae_core::db::LibraryStatus;
    use bae_core::identify::combine::LookupProvenance;
    use bae_core::identify::state::{DiscIdEvidence, SignalsContext};
    use bae_core::identify::{
        BarcodeLookupState, BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState,
        ProviderBarcodeLookup, SignalKind, SignalState, ToolbarSignal,
    };
    use bae_core::import::search::MetadataResult;
    use bae_core::import::MetadataSource;
    use bae_core::signals::{
        BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue, TextSignal,
    };
    /// The mirrors render every populated field, so this fills the ones the
    /// placeholder leaves empty. `source_tracks` stays unasked: these fixtures
    /// exercise provenance and pressing alignment, not the Ready rule.
    fn metadata_result(release_id: &str, group_id: &str) -> MetadataResult {
        MetadataResult {
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1999),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-1".to_string()),
            country: Some("US".to_string()),
            ..MetadataResult::for_test(MetadataSource::MusicBrainz, release_id, Some(group_id))
        }
    }

    fn empty_context() -> SignalsContext {
        SignalsContext {
            providers: Vec::new(),
            artwork: bae_core::signals::ArtworkScan::Absent,
            disc: Default::default(),
            barcode: Default::default(),
            catalog: Default::default(),
            text: Default::default(),
            track_count: 0,
        }
    }

    #[test]
    fn found_state_aligns_agreements_and_pressings_by_release_id() {
        let matches = vec![
            metadata_result("rel-1", "group-1"),
            metadata_result("rel-2", "group-1"),
        ];
        let state = IdentifyState::Found {
            matches: matches.clone(),
            library_statuses: vec![
                LibraryStatus::absent("rel-1"),
                LibraryStatus::absent("rel-2"),
            ],
            track_count: 12,
            provenance: vec![
                LookupProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                },
                LookupProvenance {
                    by_disc_id: false,
                    by_barcode: true,
                    by_catalog: true,
                },
            ],
            narrowed_out: Default::default(),
            ledger: None,
            context: empty_context(),
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "found");
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 1, "both matches share one release group");
        // Two lookups stand behind `rel-2` and one behind `rel-1`, so the
        // rows come back with `rel-2` on top.
        let pressings = groups[0]["pressings"].as_array().unwrap();
        assert_eq!(pressings[0]["releases"][0]["release_id"], "rel-2");
        assert_eq!(pressings[1]["releases"][0]["release_id"], "rel-1");
        // Agreements and statuses are keyed by release id and travel in the
        // rows' order, so a reader aligns them by id, never by position.
        let by_release = |field: &str, release_id: &str| {
            json[field]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["release_id"] == release_id)
                .cloned()
                .unwrap_or_else(|| panic!("{field} names {release_id}"))
        };
        assert_eq!(by_release("agreements", "rel-1")["disc_id"], true);
        let second = by_release("agreements", "rel-2");
        assert_eq!(second["barcode"], true);
        assert_eq!(second["catalog"], true);
        by_release("library_statuses", "rel-1");
        by_release("library_statuses", "rel-2");
    }

    /// Signals that share no result still settle as one `Found`; the releases
    /// they each named land in their own group cards, and every row keeps its
    /// badges saying what stands behind it.
    #[test]
    fn disagreeing_signals_become_one_found_over_several_groups() {
        let state = IdentifyState::Found {
            matches: vec![
                metadata_result("rel-disc", "g-d"),
                metadata_result("rel-bar", "g-b"),
            ],
            library_statuses: vec![
                LibraryStatus::absent("rel-disc"),
                LibraryStatus::absent("rel-bar"),
            ],
            track_count: 9,
            provenance: vec![
                LookupProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                },
                LookupProvenance {
                    by_disc_id: false,
                    by_barcode: true,
                    by_catalog: false,
                },
            ],
            narrowed_out: Default::default(),
            ledger: None,
            context: empty_context(),
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "found");
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "the two releases are two release groups");
        assert_eq!(
            groups[0]["pressings"][0]["releases"][0]["release_id"],
            "rel-disc"
        );
        assert_eq!(
            groups[1]["pressings"][0]["releases"][0]["release_id"],
            "rel-bar"
        );
        let agreements = json["agreements"].as_array().unwrap();
        assert_eq!(agreements[0]["disc_id"], true);
        assert_eq!(agreements[1]["barcode"], true);
        assert_eq!(json["track_count"], 9);
    }

    /// A run in flight crosses as its ledger: one row per code, each with one
    /// cell per provider — MusicBrainz still trying the second code while
    /// Discogs has already matched the first.
    #[test]
    fn triangulating_lists_each_code_with_one_cell_per_provider() {
        let state = IdentifyState::Triangulating {
            discid: DiscidProgress::LookingUp,
            barcode: BarcodeProgress::Lookups {
                codes: vec!["0123456789012".to_string(), "9999999999999".to_string()],
                providers: vec![
                    ProviderBarcodeLookup {
                        source: MetadataSource::MusicBrainz,
                        state: BarcodeLookupState::Trying { index: 1 },
                    },
                    ProviderBarcodeLookup {
                        source: MetadataSource::Discogs,
                        state: BarcodeLookupState::Matched {
                            code: "0123456789012".to_string(),
                            results: vec![(
                                metadata_result("rel-dg", "group-1"),
                                LibraryStatus::absent("rel-dg"),
                            )],
                        },
                    },
                ],
            },
            catalog: CatalogProgress::Skipped,
            context: SignalsContext {
                providers: vec![MetadataSource::MusicBrainz, MetadataSource::Discogs],
                disc: DiscIdEvidence {
                    signal: DiscIdSignal::Computed {
                        disc_id: "disc-hash".to_string(),
                        track_count: 9,
                        source_file: Some("rip.log".to_string()),
                    },
                    ..Default::default()
                },
                barcode: bae_core::identify::state::BarcodeEvidence {
                    codes: vec![
                        SourcedValue::in_file(
                            "0123456789012".to_string(),
                            SignalOrigin::Artwork,
                            "back.jpg".to_string(),
                        ),
                        SourcedValue::in_file(
                            "9999999999999".to_string(),
                            SignalOrigin::Artwork,
                            "inlay.jpg".to_string(),
                        ),
                    ],
                    had_source: true,
                    ..Default::default()
                },
                ..empty_context()
            },
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "triangulating");
        let run = &json["run"];
        assert_eq!(
            run["providers"],
            serde_json::json!(["music_brainz", "discogs"])
        );
        assert_eq!(run["disc_id"]["kind"], "read");
        assert_eq!(run["disc_id"]["disc_id"], "disc-hash");
        assert_eq!(run["disc_id"]["source"]["kind"], "log");
        assert_eq!(run["disc_id"]["source"]["file"], "rip.log");
        assert_eq!(run["disc_id"]["lookup"]["kind"], "looking_up");
        assert_eq!(run["barcode"]["kind"], "rows");
        assert_eq!(run["barcode"]["scanning"], false);
        let rows = run["barcode"]["rows"].as_array().unwrap();
        assert_eq!(rows[0]["value"], "0123456789012");
        assert_eq!(rows[0]["sources"][0]["origin"], "artwork");
        assert_eq!(rows[0]["sources"][0]["file"], "back.jpg");
        assert_eq!(rows[0]["cells"][0]["source"], "music_brainz");
        assert_eq!(rows[0]["cells"][0]["lookup"]["kind"], "no_match");
        assert_eq!(rows[0]["cells"][1]["source"], "discogs");
        assert_eq!(rows[0]["cells"][1]["lookup"]["kind"], "found");
        assert_eq!(rows[0]["cells"][1]["lookup"]["count"], 1);
        assert_eq!(
            rows[0]["cells"][1]["lookup"]["groups"][0]["pressings"][0]["releases"][0]["release_id"],
            "rel-dg"
        );
        assert_eq!(rows[1]["value"], "9999999999999");
        assert_eq!(rows[1]["cells"][0]["lookup"]["kind"], "looking_up");
        assert_eq!(rows[1]["cells"][1]["lookup"]["kind"], "not_asked");
        assert_eq!(run["catalog"]["kind"], "none_found");
        // Discogs's match is already on the list while MusicBrainz is out.
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]["pressings"][0]["releases"][0]["release_id"],
            "rel-dg"
        );
        assert_eq!(json["agreements"][0]["release_id"], "rel-dg");
        assert_eq!(json["agreements"][0]["barcode"], true);
    }

    #[test]
    fn toolbar_signal_maps_snake_case_and_structured_failure() {
        let signal = ToolbarSignal {
            kind: SignalKind::DiscId,
            value: Some("disc-hash".to_string()),
            origin: SignalOrigin::DiscToc,
            state: SignalState::Failed {
                failure: LookupFailure::Provider { status: Some(503) },
            },
            excluded: false,
            options: Vec::new(),
        };

        let json = serde_json::to_value(AutomationToolbarSignal::from_core(signal)).unwrap();
        assert_eq!(json["kind"], "disc_id");
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
                source_file: None,
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
            text_pool: Vec::new(),
            durations: bae_core::import::probe::SourceDurations::totalling(2_400_000),
        };

        let json = serde_json::to_value(AutomationSignals::from_core(signals)).unwrap();
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
#[path = "import_queue_tests.rs"]
mod import_queue;

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
