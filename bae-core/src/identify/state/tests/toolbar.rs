// The toolbar projection: what each badge reads while a run is going and
// once it has settled.

#[test]
fn toolbar_while_triangulating_shows_spinners() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
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
    assert_eq!(disc.value.as_deref(), Some("disc-hash"));
    assert_eq!(disc.origin, SignalOrigin::DiscToc);
    assert_eq!(disc.state, SignalState::LookingUp);

    let barcode = &toolbar[1];
    assert_eq!(barcode.kind, SignalKind::Barcode);
    assert_eq!(barcode.value.as_deref(), Some("012345678905"));
    assert_eq!(barcode.state, SignalState::LookingUp);

    // Nothing is chosen for the catalog until the user chooses, so it names no
    // value and nothing ran for it — the extracted numbers are its list.
    let catalog = &toolbar[2];
    assert_eq!(catalog.kind, SignalKind::Catalog);
    assert_eq!(catalog.value, None);
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
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            vec![
                SourcedValue::new("LBL 001".to_string(), SignalOrigin::FolderName),
                SourcedValue::new("LBL 999".to_string(), SignalOrigin::Artwork),
            ],
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
            .map(|o| (o.value.as_str(), o.origin))
            .collect::<Vec<_>>(),
        vec![
            ("LBL 001", SignalOrigin::FolderName),
            ("LBL 999", SignalOrigin::Artwork),
        ]
    );
}

/// Checking a catalog number runs its own lookup, and its results join the
/// intersection the other signals are already in.
#[test]
fn choosing_a_catalog_number_looks_it_up_and_intersects() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &["LBL 001"],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-a", Some("g-x")), pair("rel-b", Some("g-x"))],
            track_count: 5,
        },
    );
    assert!(matches!(state, IdentifyState::Found { .. }));

    let (state, effects) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Catalog("LBL 001".to_string()),
        },
    );
    assert_eq!(
        effects,
        vec![Effect::LookupCatalog {
            source: MB,
            catalog: "LBL 001".to_string(),
        }]
    );
    let catalog_badge = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Catalog)
        .expect("catalog badge");
    assert_eq!(catalog_badge.value.as_deref(), Some("LBL 001"));
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
            ref matches,
            ref provenance,
            ..
        } => {
            assert_eq!(
                matches.iter().map(|m| m.release_id.as_str()).collect::<Vec<_>>(),
                vec!["rel-b"]
            );
            assert!(provenance[0].by_disc_id && provenance[0].by_catalog);
        }
        other => panic!("expected Found, got {other:?}"),
    }
    let catalog_badge = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Catalog)
        .expect("catalog badge");
    assert_eq!(catalog_badge.state, SignalState::Found { count: 1 });
    assert!(catalog_badge
        .options
        .iter()
        .any(|o| o.value == "LBL 001" && o.chosen));
}

/// Checking the number already checked clears the choice, and the catalog
/// leaves the combine again.
#[test]
fn checking_the_chosen_catalog_number_again_clears_it() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &["LBL 001"],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-a", Some("g-x")), pair("rel-b", Some("g-x"))],
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Catalog("LBL 001".to_string()),
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupAnswered {
            source: MB,
            for_catalog: "LBL 001".to_string(),
            outcome: Ok(vec![pair("rel-b", Some("g-x"))]),
        },
    );
    let (state, effects) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Catalog("LBL 001".to_string()),
        },
    );
    assert!(effects.is_empty(), "clearing the choice looks nothing up");
    match state {
        IdentifyState::Found { ref matches, .. } => assert_eq!(matches.len(), 2),
        ref other => panic!("expected Found, got {other:?}"),
    }
    let catalog_badge = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Catalog)
        .expect("catalog badge");
    assert_eq!(catalog_badge.value, None);
    assert_eq!(catalog_badge.state, SignalState::Skipped);
}

#[test]
fn toolbar_shows_failed_disc_id_lookup() {
    // The disc-ID lookup fails while the barcode is still in flight, so the badge
    // must read Failed rather than keep spinning.
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
            track_count: 5,
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
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
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
        source: MetadataSource::MusicBrainz,
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
            DiscIdSignal::Absent { track_count: 5 },
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
        source: MetadataSource::MusicBrainz,
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

/// Mirrors `toolbar_keeps_failed_barcode_lookup_after_settle` for the disc-ID
/// side. Before `discid_failure` existed on `SignalsContext`, the settled badge
/// read off `context.disc_id` (still `Computed` — that only reports whether a
/// disc ID could be derived, not whether its lookup succeeded) and the empty
/// `discid_results`, landing on `NoMatch` — indistinguishable from a lookup
/// that ran cleanly and found nothing. This is the case `identify::verdict`
/// depends on being distinguishable, since a `NotFoundAnywhere` masking a
/// failure must not be persisted as a permanent verdict.
#[test]
fn toolbar_keeps_failed_disc_id_lookup_after_settle() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    let failure = LookupFailure::Provider { status: Some(503) };
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: failure.clone(),
            track_count: 5,
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
            DiscIdSignal::Absent { track_count: 7 },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    assert!(matches!(state, IdentifyState::ManualOnly { .. }));
    let toolbar = state.toolbar();
    let disc = &toolbar[0];
    assert_eq!(disc.state, SignalState::Skipped);
    assert_eq!(disc.value, None);
    let barcode = &toolbar[1];
    assert_eq!(barcode.state, SignalState::Skipped);
}

#[test]
fn idle_has_empty_toolbar() {
    assert!(IdentifyState::Idle.toolbar().is_empty());
}

/// A pair whose `LibraryStatus` reports the release and its album already in the
/// library — the flags every other fixture here leaves false.
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

/// An in-library match keeps its flags through combine into `Found`, index-aligned
/// with `matches`.
#[test]
fn found_carries_in_library_status_through() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair_in_library(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("g-x"),
            )],
            track_count: 5,
        },
    );
    match state {
        IdentifyState::Found {
            matches,
            library_statuses,
            ..
        } => {
            assert_eq!(matches.len(), 1);
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

/// `SignalToggled` from a terminal `Found` state re-derives in place: excluding
/// the only signal that produced results drops to `NotFoundAnywhere`, and
/// re-including it restores the `Found`.
#[test]
fn toggle_from_found_re_derives_terminal_state() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    let (found, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 5,
        },
    );
    assert!(matches!(found, IdentifyState::Found { .. }));

    let (excluded, effects) = step(
        found,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Disc,
        },
    );
    assert!(effects.is_empty(), "toggle re-combines in place");
    assert!(
        matches!(excluded, IdentifyState::NotFoundAnywhere { .. }),
        "excluding the only signal with results leaves nothing found, got {excluded:?}"
    );

    let (restored, _) = step(
        excluded,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Disc,
        },
    );
    assert!(
        matches!(restored, IdentifyState::Found { .. }),
        "re-including the disc signal restores Found, got {restored:?}"
    );
}

/// A barcode lookup can fail while the disc-ID lookup is still in flight. The
/// pipeline stays `Triangulating` until the disc settles, then reports the
/// failed automatic lookup instead of presenting the disc's partial answer.
#[test]
fn barcode_failure_before_disc_settles_is_retained_through_combine() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["BAR"]),
            },
            &[],
        ),
    );

    let failure = LookupFailure::Provider { status: Some(500) };
    let source_failure = SourceFailure {
        source: MetadataSource::MusicBrainz,
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
            track_count: 5,
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

/// The chosen catalog number is asked of every provider, and each answers on
/// its own: one failing leaves the other's results in the combine, named
/// beside them.
#[test]
fn a_catalog_lookup_keeps_one_provider_s_answer_beside_the_other_s_failure() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &["LBL 001"],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-a", Some("g-x")), pair("rel-b", Some("g-x"))],
            track_count: 5,
        },
    );
    let (state, effects) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Catalog("LBL 001".to_string()),
        },
    );
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
            failures, matches, ..
        } => {
            assert_eq!(
                failures,
                &vec![crate::identify::IdentifyFailure::Catalog(SourceFailure {
                    source: DG,
                    failure: LookupFailure::Timeout,
                })]
            );
            assert_eq!(
                matches.iter().map(|m| m.release_id.as_str()).collect::<Vec<_>>(),
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

/// The barcode badge spins while any provider is still walking, and settles
/// on the count once every provider has answered.
#[test]
fn toolbar_barcode_spins_until_every_provider_answers() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 5 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        barcode_matched(
            DG,
            "012345678905",
            vec![discogs_pair("dg-1", Some("g-x"))],
        ),
    );
    let barcode = state.toolbar()[1].clone();
    assert_eq!(barcode.state, SignalState::LookingUp);

    let (state, _) = step(state, barcode_missed(MB, "012345678905"));
    let barcode = state.toolbar()[1].clone();
    assert_eq!(barcode.state, SignalState::Found { count: 1 });
}
