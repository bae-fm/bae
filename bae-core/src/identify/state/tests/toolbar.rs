// The toolbar projection: what each badge reads while a run is going and
// once it has settled.

/// A run whose disc ID found two releases of one group, settled `Found` with
/// "LBL 001" offered but not chosen.
fn state_with_catalog_offered(providers: Vec<Catalog>) -> IdentifyState {
    let (state, _) = update(
        started_with(providers),
        signals(disc("disc-hash"), BarcodeSignal::Absent, &["LBL 001"]),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-a", Some("g-x")), pair("rel-b", Some("g-x"))],
        },
    );
    state
}

/// The same run started with "LBL 001" chosen, with the effects it dispatched.
fn run_with_catalog_chosen(providers: Vec<Catalog>) -> (IdentifyState, Vec<Effect>) {
    let (state, chosen_effects) = started_with_choices(providers, choosing(&["LBL 001"]));
    let (state, _) = update(
        state,
        signals(disc("disc-hash"), BarcodeSignal::Absent, &["LBL 001"]),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-a", Some("g-x")), pair("rel-b", Some("g-x"))],
        },
    );
    (state, chosen_effects)
}

#[test]
fn toolbar_while_triangulating_shows_spinners() {
    let (state, _) = update(
        started(),
        signals(
            disc("disc-hash"),
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &["LBL-001"],
        ),
    );
    let toolbar = state.toolbar();
    // Three badges, whatever the candidate turned up: disc, barcode, catalog.
    assert_eq!(toolbar.len(), 3);

    let disc = &toolbar[0];
    assert_eq!(disc.kind, SignalKind::DiscId);
    assert_eq!(disc.shown.as_deref(), Some("disc-hash"));
    assert_eq!(disc.state, SignalState::LookingUp);

    let barcode = &toolbar[1];
    assert_eq!(barcode.kind, SignalKind::Barcode);
    assert_eq!(
        barcode.shown.as_deref(),
        Some("012345678905")
    );
    assert_eq!(barcode.state, SignalState::LookingUp);

    // No number is chosen, so the catalog shows none and ran nothing.
    let catalog = &toolbar[2];
    assert_eq!(catalog.kind, SignalKind::Catalog);
    assert_eq!(catalog.shown, None);
    assert_eq!(catalog.state, SignalState::Skipped);
    assert_eq!(
        catalog
            .options
            .iter()
            .map(|o| o.value.as_str())
            .collect::<Vec<_>>(),
        vec!["LBL-001"]
    );
    assert!(catalog.options.iter().all(|o| !o.chosen));
}

/// Thirty extracted catalog numbers are one badge with thirty options behind
/// it, not thirty badges.
#[test]
fn every_extracted_catalog_number_is_an_option_on_the_one_badge() {
    let (state, _) = update(
        started(),
        signals_with_catalogs(
            disc("disc-hash"),
            BarcodeSignal::Absent,
            vec!["LBL 001".to_string(), "LBL 999".to_string()],
        ),
    );
    let toolbar = state.toolbar();
    let catalogs: Vec<&ToolbarSignal> = toolbar
        .iter()
        .filter(|s| s.kind == SignalKind::Catalog)
        .collect();
    assert_eq!(catalogs.len(), 1);
    assert_eq!(
        catalogs[0]
            .options
            .iter()
            .map(|o| o.value.as_str())
            .collect::<Vec<_>>(),
        vec!["LBL 001", "LBL 999"]
    );
}

/// A chosen number is looked up from the start and joins the intersection.
#[test]
fn a_chosen_catalog_number_is_looked_up_from_the_start() {
    let (state, effects) = run_with_catalog_chosen(vec![MB]);
    assert_eq!(
        effects,
        vec![Effect::LookupCatalog {
            source: MB,
            catalog: "LBL 001".to_string(),
        }]
    );
    let catalog_badge = badge(&state, SignalKind::Catalog);
    assert_eq!(
        catalog_badge.shown.as_deref(),
        Some("LBL 001")
    );
    assert_eq!(catalog_badge.state, SignalState::LookingUp);

    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "LBL 001".to_string(),
            outcome: Ok(vec![pair("rel-b", Some("g-x"))]),
        },
    );
    match state {
        IdentifyState::Found {
            findings:
                Findings {
                    ref matches,
                    ref provenance,
                    ..
                },
            ..
        } => {
            assert_eq!(
                matches
                    .iter()
                    .map(|m| m.release_id.as_str())
                    .collect::<Vec<_>>(),
                vec!["rel-b"]
            );
            assert!(provenance[0].by_disc_id && provenance[0].by_catalog);
        }
        other => panic!("expected Found, got {other:?}"),
    }
    let catalog_badge = badge(&state, SignalKind::Catalog);
    assert_eq!(catalog_badge.state, SignalState::Found { count: 1 });
    assert!(catalog_badge
        .options
        .iter()
        .any(|o| o.value == "LBL 001" && o.chosen));
}

/// With no number chosen, none is asked about and the catalog takes no part.
#[test]
fn a_run_with_no_chosen_number_asks_about_none_of_them() {
    let state = state_with_catalog_offered(vec![MB]);
    assert!(matches!(state, IdentifyState::Found { .. }));
    let catalog_badge = badge(&state, SignalKind::Catalog);
    assert_eq!(catalog_badge.shown, None);
    assert_eq!(catalog_badge.state, SignalState::Skipped);
    assert!(catalog_badge.options.iter().all(|option| !option.chosen));
}

/// A chosen number the settled snapshot no longer offers leaves the run.
#[test]
fn a_chosen_number_the_snapshot_does_not_offer_leaves_the_run() {
    let (state, effects) = started_with_choices(vec![MB], choosing(&["GONE 001"]));
    assert_eq!(
        effects,
        vec![Effect::LookupCatalog {
            source: MB,
            catalog: "GONE 001".to_string(),
        }]
    );
    let (state, _) = update(
        state,
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            &["LBL 001"],
        ),
    );
    // Nothing is left in flight to wait on.
    assert!(
        !matches!(state, IdentifyState::Triangulating { .. }),
        "got {state:?}"
    );
    let catalog_badge = badge(&state, SignalKind::Catalog);
    assert_eq!(catalog_badge.state, SignalState::Skipped);
    assert!(catalog_badge.options.iter().all(|option| !option.chosen));
}

/// A chosen number is not dropped against a snapshot still being read.
#[test]
fn a_chosen_number_survives_a_snapshot_still_being_read() {
    let (state, _) = started_with_choices(vec![MB], choosing(&["LBL 001"]));
    let (state, _) = step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals: Signals {
                origin: crate::signals::AudioOrigin::default(),
                disc_id: DiscIdSignal::Absent,
                barcode: BarcodeSignal::Scanning { codes: Vec::new() },
                text: TextSignal::Scanning {
                    catalogs: Vec::new(),
                    free_text: Vec::new(),
                },
                text_pool: Vec::new(),
                registered_in: None,
            },
            audio: crate::signals::AudioFacts::default(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    let catalog_badge = badge(&state, SignalKind::Catalog);
    assert_eq!(
        catalog_badge.state,
        SignalState::LookingUp,
        "the chosen number's lookup is still out"
    );
}

#[test]
fn toolbar_shows_failed_disc_id_lookup() {
    // The disc badge reads Failed while the barcode is still in flight.
    let (state, _) = update(started(), disc_and_codes("disc-hash", &["012345678905"]));
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
        },
    );
    let disc = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert_eq!(
        disc.state,
        SignalState::Failed {
            failure: LookupFailure::Provider { status: Some(503) }
        }
    );
}

#[test]
fn toolbar_shows_failed_barcode_lookup() {
    let (state, _) = update(started(), disc_and_codes("disc-hash", &["012345678905"]));
    let failure = LookupFailure::Diagnostic {
        detail: "provider lookup failed".to_string(),
    };
    let source_failure = SourceFailure {
        source: Catalog::MusicBrainz,
        failure: failure.clone(),
    };
    let (state, _) = step(
        state,
        barcode_failed(MB, "012345678905", source_failure.failure.clone()),
    );
    let barcode = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Barcode)
        .expect("barcode badge");
    assert_eq!(barcode.state, SignalState::Failed { failure });
}

#[test]
fn toolbar_keeps_failed_barcode_lookup_after_settle() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &[],
        ),
    );
    let failure = LookupFailure::Diagnostic {
        detail: "provider lookup failed".to_string(),
    };
    let source_failure = SourceFailure {
        source: Catalog::MusicBrainz,
        failure: failure.clone(),
    };
    let (state, _) = step(
        state,
        barcode_failed(MB, "012345678905", source_failure.failure.clone()),
    );
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let barcode = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Barcode)
        .expect("barcode badge");
    assert_eq!(barcode.state, SignalState::Failed { failure });
}

/// A failed disc-ID lookup still reads Failed once settled, not as a no-match.
#[test]
fn toolbar_keeps_failed_disc_id_lookup_after_settle() {
    let (state, _) = update(
        started(),
        signals(disc("disc-hash"), BarcodeSignal::Absent, &[]),
    );
    let failure = LookupFailure::Provider { status: Some(503) };
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: failure.clone(),
        },
    );
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let disc = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert_eq!(disc.state, SignalState::Failed { failure });
}

#[test]
fn toolbar_skipped_disc_and_barcode_in_manual_only() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            &[],
        ),
    );
    assert!(matches!(state, IdentifyState::ManualOnly { .. }));
    let toolbar = state.toolbar();
    let disc = &toolbar[0];
    assert_eq!(disc.state, SignalState::Skipped);
    assert_eq!(disc.shown, None);
    let barcode = &toolbar[1];
    assert_eq!(barcode.state, SignalState::Skipped);
}

#[test]
fn idle_has_empty_toolbar() {
    assert!(IdentifyState::Idle.toolbar().is_empty());
}

/// A pair whose release and album are already in the library.
fn pair_in_library(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
    (
        mk_result(release_id, group_id),
        LibraryStatus {
            release_id: release_id.to_string(),
            release_in_library: true,
            album_in_library: true,
            album_title: Some("Album".to_string()),
            album_id: Some("9fd7bfa8-3c7c-4026-8559-da66af02f636".to_string()),
        },
    )
}

/// An in-library match keeps its flags through combine into `Found`.
#[test]
fn found_carries_in_library_status_through() {
    let (state, _) = disc_only_started();
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair_in_library(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("g-x"),
            )],
        },
    );
    match state {
        IdentifyState::Found {
            library_statuses,
            findings: Findings { matches, .. },
            ..
        } => {
            assert_eq!(matches.len(), 1);
            let library_statuses = &library_statuses.matches;
            assert_eq!(library_statuses.len(), 1);
            assert!(library_statuses[0].release_in_library);
            assert!(library_statuses[0].album_in_library);
            assert_eq!(
                library_statuses[0].album_id.as_deref(),
                Some("9fd7bfa8-3c7c-4026-8559-da66af02f636")
            );
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

/// A barcode failure while the disc ID is still out waits for it, then settles
/// as failed.
#[test]
fn barcode_failure_before_disc_settles_is_retained_through_combine() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));

    let failure = LookupFailure::Provider { status: Some(500) };
    let source_failure = SourceFailure {
        source: Catalog::MusicBrainz,
        failure: failure.clone(),
    };
    let (state, _) = step(state, barcode_failed(MB, "BAR", failure.clone()));
    // Disc-ID hasn't settled yet — the barcode failure alone can't terminate.
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "barcode failure while disc still looking up stays Triangulating, got {state:?}"
    );

    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
        },
    );

    assert!(matches!(
        &state,
        IdentifyState::Failed { failures, .. }
            if failures == &vec![crate::identify::IdentifyFailure::Barcode(source_failure.clone())]
    ));
    // The terminal toolbar surfaces the retained barcode failure.
    let barcode = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Barcode)
        .expect("barcode badge");
    assert_eq!(barcode.state, SignalState::Failed { failure });
}

/// One provider failing a catalog lookup leaves the other's results standing.
#[test]
fn a_catalog_lookup_keeps_one_provider_s_answer_beside_the_other_s_failure() {
    let (state, effects) = run_with_catalog_chosen(vec![MB, DG]);
    assert_eq!(
        effects,
        vec![
            Effect::LookupCatalog {
                source: MB,
                catalog: "LBL 001".to_string(),
            },
            Effect::LookupCatalog {
                source: DG,
                catalog: "LBL 001".to_string(),
            },
        ]
    );

    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: DG,
            for_catalog: "LBL 001".to_string(),
            outcome: Err(LookupFailure::Timeout),
        },
    );
    // MusicBrainz is still out, so the badge still spins and nothing settles.
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    let catalog_badge = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Catalog)
        .expect("catalog badge");
    assert_eq!(catalog_badge.state, SignalState::LookingUp);

    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "LBL 001".to_string(),
            outcome: Ok(vec![pair("rel-b", Some("g-x"))]),
        },
    );
    match &state {
        IdentifyState::Failed {
            failures,
            findings: Findings { matches, .. },
            ..
        } => {
            assert_eq!(
                failures,
                &vec![crate::identify::IdentifyFailure::Catalog(SourceFailure {
                    source: DG,
                    failure: LookupFailure::Timeout,
                })]
            );
            assert_eq!(
                matches
                    .iter()
                    .map(|m| m.release_id.as_str())
                    .collect::<Vec<_>>(),
                vec!["rel-b"]
            );
        }
        other => panic!("expected Failed with the surviving intersection, got {other:?}"),
    }
    let catalog_badge = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Catalog)
        .expect("catalog badge");
    assert_eq!(catalog_badge.state, SignalState::Found { count: 1 });
}

/// The barcode badge spins until every provider has answered.
#[test]
fn toolbar_barcode_spins_until_every_provider_answers() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        barcode_matched(DG, "012345678905", vec![discogs_pair("dg-1", Some("g-x"))]),
    );
    let barcode = state.toolbar()[1].clone();
    assert_eq!(barcode.state, SignalState::LookingUp);

    let (state, _) = step(state, barcode_missed(MB, "012345678905"));
    let barcode = state.toolbar()[1].clone();
    assert_eq!(barcode.state, SignalState::Found { count: 1 });
}
