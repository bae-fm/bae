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

/// A candidate key that names nothing fails as `not_found` before anything is
/// fetched, so a typo never reads as a candidate awaiting identification.
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

/// `release_metadata_update` enforces the same rule as the desktop editor's
/// Save, so it can't write what the editor would refuse.
mod release_metadata_update_input {
    use super::*;

    fn edit(album_title: &str, album_artist_credit_names: &[&str]) -> AutomationReleaseUserEdit {
        AutomationReleaseUserEdit {
            album_title: album_title.to_string(),
            album_artist_assignments: album_artist_credit_names
                .iter()
                .map(|name| AutomationArtistAssignment::Credit {
                    credit: AutomationArtistCredit {
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
                labels: Vec::new(),
                barcode: None,
                facts: Default::default(),
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

    /// An untrimmed title is trimmed, as the desktop editor does, not refused.
    #[test]
    fn an_untrimmed_album_title_normalizes() {
        let wire = edit("  Album Alpha  ", &["  Artist Alpha  "])
            .into_core()
            .normalized();
        assert_eq!(wire.validate(), Ok(()));
        assert_eq!(wire.album_title, "Album Alpha");
        assert_eq!(
            wire.album_artist_assignments,
            vec![bae_core::import::ArtistAssignment::named("Artist Alpha")]
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
        edit.album_artist_assignments = vec![AutomationArtistAssignment::Picked {
            artist: artist.clone(),
        }];

        let round_trip = AutomationReleaseUserEdit::from_core(edit.into_core());
        let AutomationArtistAssignment::Picked {
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

    /// A refused edit reaches the client as `validation`, not `import`.
    #[test]
    fn a_refused_edit_crosses_as_a_validation_error() {
        let error = AutomationError::from(LibraryError::Edit(
            bae_core::import::EditValidationError::EmptyAlbumTitle,
        ));
        assert_eq!(error.kind(), "validation");
        assert_eq!(error.message(), "Album title is required");
    }
}

/// The storage state and actions serialize as snake_case strings.
#[test]
fn release_storage_state_and_actions_serialize_snake_case() {
    let summary = AutomationReleaseSummary::from_core(bae_core::album_detail::ReleaseSummary {
        id: "rel-1".to_string(),
        album_id: "alb-1".to_string(),
        media: vec![bae_core::pressing::MediaCount {
            medium: bae_core::pressing::Medium::Digital,
            count: 1,
        }],
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
    // The in-flight transition the desktop shows.
    assert_eq!(json["transfer_action"], "make_local");
}

#[test]
fn a_local_release_serializes_its_state_and_absent_transfer() {
    let summary = AutomationReleaseSummary::from_core(bae_core::album_detail::ReleaseSummary {
        id: "rel-2".to_string(),
        album_id: "alb-1".to_string(),
        media: Vec::new(),
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
        None,
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
        BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState, LookupState,
        ProviderLookup, SignalKind, SignalState, ToolbarSignal, ValueLookup,
    };
    use bae_core::import::search::MetadataResult;
    use bae_core::import::Catalog;
    use bae_core::signals::{
        BarcodeSignal, DiscIdSignal, LookupFailure, Signals, SourcedValue, TextSignal,
    };
    /// A result with every field the mirrors render filled in.
    fn metadata_result(release_id: &str, group_id: &str) -> MetadataResult {
        MetadataResult {
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1999),
            labels: vec![bae_core::pressing::ReleaseLabel::of(
                Some("Label A"),
                Some("AB 100"),
            )],
            area: Some(bae_core::pressing::ReleaseArea::Country(
                bae_core::pressing::Country::from_code("US").unwrap(),
            )),
            media: bae_core::pressing::StatedMedia::PerMedium(vec![Some(
                bae_core::pressing::Medium::Cd,
            )]),
            ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some(group_id))
        }
    }

    fn empty_context() -> SignalsContext {
        SignalsContext {
            providers: Vec::new(),
            steps: bae_core::config::IdentificationSteps::default(),
            artwork: bae_core::signals::ArtworkScan::Absent,
            origin: bae_core::signals::AudioOrigin::default(),
            disc: Default::default(),
            barcode: Default::default(),
            catalog: Default::default(),
            search: Default::default(),
            text: Default::default(),
            text_settled: true,
            audio: Default::default(),
            isrc: Default::default(),
            album_links: bae_core::identify::state::AlbumLinkReading::Pending,
            documents: bae_core::identify::documents::DocumentReading::Pending,
        }
    }

    #[test]
    fn found_state_aligns_agreements_and_pressings_by_release_id() {
        let matches = vec![
            metadata_result("rel-1", "group-1"),
            metadata_result("rel-2", "group-1"),
        ];
        let state = IdentifyState::Found {
            library_statuses: bae_core::identify::LibraryStatuses {
                matches: vec![
                    LibraryStatus::absent("rel-1"),
                    LibraryStatus::absent("rel-2"),
                ],
                narrowed_out: Vec::new(),
            },
            track_count: 12,
            findings: bae_core::identify::Findings {
                matches: matches.clone(),
                provenance: vec![
                    LookupProvenance {
                        by_disc_id: true,
                        by_barcode: false,
                        by_catalog: false,
                        by_isrc: false,
                        by_search: false,
                        named_by: None,
                    },
                    LookupProvenance {
                        by_disc_id: true,
                        by_barcode: true,
                        by_catalog: false,
                        by_isrc: false,
                        by_search: false,
                        named_by: None,
                    },
                ],
                // Two pressings of one album: each release is its own row.
                pressings: vec![0, 1],
                narrowed_out: Default::default(),
                medium_conflict: None,
            },
            ledger: None,
            context: empty_context(),
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "found");
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 1, "both matches share one release group");
        // Two lookups stand behind `rel-2` and one behind `rel-1`, so the
        // rows come back with `rel-2` on top.
        let pressings = groups[0]["sections"][0]["pressings"].as_array().unwrap();
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
        assert_eq!(second["disc_id"], true);
        assert_eq!(second["barcode"], true);
        by_release("library_statuses", "rel-1");
        by_release("library_statuses", "rel-2");
    }

    /// Signals that share no result settle as one `Found`, each release on its
    /// own card with its badges.
    #[test]
    fn disagreeing_signals_become_one_found_over_several_groups() {
        let state = IdentifyState::Found {
            library_statuses: bae_core::identify::LibraryStatuses {
                matches: vec![
                    LibraryStatus::absent("rel-disc"),
                    LibraryStatus::absent("rel-bar"),
                ],
                narrowed_out: Vec::new(),
            },
            track_count: 9,
            findings: bae_core::identify::Findings {
                matches: vec![
                    metadata_result("rel-disc", "g-d"),
                    metadata_result("rel-bar", "g-b"),
                ],
                provenance: vec![
                    LookupProvenance {
                        by_disc_id: true,
                        by_barcode: false,
                        by_catalog: false,
                        by_isrc: false,
                        by_search: false,
                        named_by: None,
                    },
                    LookupProvenance {
                        by_disc_id: false,
                        by_barcode: true,
                        by_catalog: false,
                        by_isrc: false,
                        by_search: false,
                        named_by: None,
                    },
                ],
                // One release each, so each is its own row.
                pressings: vec![0, 1],
                narrowed_out: Default::default(),
                medium_conflict: None,
            },
            ledger: None,
            context: empty_context(),
        };

        let json = serde_json::to_value(automation_identify_state(state)).unwrap();
        assert_eq!(json["kind"], "found");
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 2, "the two releases are two release groups");
        assert_eq!(
            groups[0]["sections"][0]["pressings"][0]["releases"][0]["release_id"],
            "rel-disc"
        );
        assert_eq!(
            groups[1]["sections"][0]["pressings"][0]["releases"][0]["release_id"],
            "rel-bar"
        );
        let agreements = json["agreements"].as_array().unwrap();
        assert_eq!(agreements[0]["disc_id"], true);
        assert_eq!(agreements[1]["barcode"], true);
        assert_eq!(json["track_count"], 9);
    }

    /// A run in flight crosses as its ledger: one row per code, one cell per
    /// provider.
    #[test]
    fn triangulating_lists_each_code_with_one_cell_per_provider() {
        let state = IdentifyState::Triangulating {
            discid: DiscidProgress::LookingUp,
            barcode: BarcodeProgress::Lookups {
                codes: vec![
                    ValueLookup {
                        value: "0123456789012".to_string(),
                        providers: vec![
                            ProviderLookup {
                                source: Catalog::MusicBrainz,
                                state: LookupState::Done {
                                    results: Vec::new(),
                                },
                            },
                            ProviderLookup {
                                source: Catalog::Discogs,
                                state: LookupState::Done {
                                    results: vec![(
                                        metadata_result("rel-dg", "group-1"),
                                        LibraryStatus::absent("rel-dg"),
                                    )],
                                },
                            },
                        ],
                    },
                    ValueLookup {
                        value: "9999999999999".to_string(),
                        providers: vec![
                            ProviderLookup {
                                source: Catalog::MusicBrainz,
                                state: LookupState::LookingUp,
                            },
                            ProviderLookup {
                                source: Catalog::Discogs,
                                state: LookupState::Done {
                                    results: Vec::new(),
                                },
                            },
                        ],
                    },
                ],
            },
            catalog: CatalogProgress::Skipped,
            isrc: bae_core::identify::IsrcProgress::Skipped,
            search: bae_core::identify::SearchProgress::Pending,
            context: SignalsContext {
                providers: vec![Catalog::MusicBrainz, Catalog::Discogs],
                disc: DiscIdEvidence {
                    signal: DiscIdSignal::Computed {
                        disc_id: "disc-hash".to_string(),
                        source_file: Some("rip.log".to_string()),
                    },
                    ..Default::default()
                },
                barcode: bae_core::identify::state::BarcodeEvidence {
                    codes: vec![
                        SourcedValue::in_file("0123456789012".to_string(), "back.jpg".to_string()),
                        SourcedValue::in_file("9999999999999".to_string(), "inlay.jpg".to_string()),
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
        assert_eq!(run["disc_id"]["lookup"]["kind"], "looking_up");
        assert_eq!(run["barcode"]["kind"], "rows");
        assert_eq!(run["barcode"]["scanning"], false);
        let rows = run["barcode"]["rows"].as_array().unwrap();
        assert_eq!(rows[0]["value"], "0123456789012");
        assert_eq!(rows[0]["cells"][0]["source"], "music_brainz");
        assert_eq!(rows[0]["cells"][0]["lookup"]["kind"], "no_match");
        assert_eq!(rows[0]["cells"][1]["source"], "discogs");
        assert_eq!(rows[0]["cells"][1]["lookup"]["kind"], "found");
        assert_eq!(rows[0]["cells"][1]["lookup"]["count"], 1);
        assert_eq!(
            rows[0]["cells"][1]["lookup"]["groups"][0]["sections"][0]["pressings"][0]["releases"]
                [0]["release_id"],
            "rel-dg"
        );
        assert_eq!(rows[1]["value"], "9999999999999");
        assert_eq!(rows[1]["cells"][0]["lookup"]["kind"], "looking_up");
        assert_eq!(rows[1]["cells"][1]["lookup"]["kind"], "no_match");
        assert_eq!(run["catalog"]["kind"], "none_found");
        // Discogs's match is already on the list while MusicBrainz is out.
        let groups = json["groups"].as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]["sections"][0]["pressings"][0]["releases"][0]["release_id"],
            "rel-dg"
        );
        assert_eq!(json["agreements"][0]["release_id"], "rel-dg");
        assert_eq!(json["agreements"][0]["barcode"], true);
    }

    #[test]
    fn toolbar_signal_maps_snake_case_and_structured_failure() {
        let signal = ToolbarSignal {
            kind: SignalKind::DiscId,
            shown: Some("disc-hash".to_string()),
            state: SignalState::Failed {
                failure: LookupFailure::Provider { status: Some(503) },
            },
            excluded: false,
            options: Vec::new(),
        };

        let json = serde_json::to_value(AutomationToolbarSignal::from_core(signal)).unwrap();
        assert_eq!(json["kind"], "disc_id");
        assert_eq!(json["shown"], "disc-hash");
        assert_eq!(json["state"]["kind"], "failed");
        assert_eq!(json["state"]["failure"]["kind"], "provider");
        assert_eq!(json["state"]["failure"]["status"], 503);
    }

    #[test]
    fn signals_map_all_three_subsignals() {
        let signals = Signals {
            origin: bae_core::signals::AudioOrigin {
                source: Some(bae_core::signals::AudioSource::CdRip {
                    proof: bae_core::signals::CdProof::RipLog,
                    file: Some("Album.log".to_string()),
                }),
                not_cd_rate: None,
            },
            disc_id: DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                source_file: None,
            },
            barcode: BarcodeSignal::Settled {
                codes: vec![SourcedValue::new("0123456789012".to_string())],
            },
            text: TextSignal::Settled {
                catalogs: vec!["CAT-1".to_string()],
                free_text: vec!["Album Title".to_string()],
            },
            text_pool: Vec::new(),
            isrcs: Vec::new(),
        };

        let json = serde_json::to_value(AutomationSignals::from_core(signals)).unwrap();
        assert_eq!(json["origin"]["source"]["kind"], "cd_rip");
        assert_eq!(json["origin"]["source"]["proof"], "rip_log");
        assert_eq!(json["origin"]["source"]["file"], "Album.log");
        assert_eq!(json["origin"]["not_cd_rate"], serde_json::Value::Null);
        assert_eq!(json["disc_id"]["kind"], "computed");
        assert_eq!(json["disc_id"]["disc_id"], "disc-hash");
        assert_eq!(json["barcode"]["kind"], "settled");
        assert_eq!(json["barcode"]["codes"][0], "0123456789012");
        assert_eq!(json["text"]["kind"], "settled");
        assert_eq!(json["text"]["catalogs"][0], "CAT-1");
        assert_eq!(json["text"]["free_text"][0], "Album Title");
    }
}
/// Every call reads the import tables by key, so a key with nothing recorded
/// names nothing.
#[path = "import_queue_tests.rs"]
mod import_queue;

/// The storage tool accepts and refuses what the Storage Manager's row menu
/// does.
mod release_storage_action {
    use super::*;

    fn summary(actions: Vec<AutomationReleaseStorageAction>) -> AutomationReleaseSummary {
        AutomationReleaseSummary {
            id: "release-1".to_string(),
            album_id: "album-1".to_string(),
            media: Vec::new(),
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

    /// Every action arrives under its Storage Manager name with what it needs.
    #[test]
    fn each_action_parses_from_its_wire_shape() {
        let moved = parse(serde_json::json!({
            "release_id": "release-1",
            "action": { "kind": "move_to_cloud" },
        }));
        assert_eq!(moved.release_id, "release-1");
        assert!(matches!(moved.action, AutomationStorageAction::MoveToCloud));

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

    /// Making local needs a folder; there is no default for it.
    #[test]
    fn an_action_missing_what_it_needs_is_refused() {
        let error = from_value::<ReleaseStorageActionInput>(serde_json::json!({
            "release_id": "release-1",
            "action": { "kind": "make_local" },
        }))
        .expect_err("an action without its required field is not an action");
        assert_eq!(error.kind(), "validation");
    }

    /// A transition core doesn't offer is refused before any transfer starts.
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

    /// A library with no cloud home says so rather than listing no transitions.
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

    /// A move to the cloud reports the revision its uploads were queued at.
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
