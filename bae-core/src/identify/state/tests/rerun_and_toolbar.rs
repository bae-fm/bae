/// Drive the reducer through triangulation into a disagreement: the disc ID
/// matches one release and the barcode two others, so nothing intersects and
/// the settled `Found` holds all three.
fn driven_disagreement() -> IdentifyState {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 7,
                source_file: None,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["BAR"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 7,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![
                pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y")),
                pair("rel-3", Some("g-y")),
            ],
            failures: Vec::new(),
        },
    );
    // Nothing intersects, so the set is the union: one disc-ID result and two
    // barcode ones.
    let IdentifyState::Found { matches, .. } = &state else {
        panic!("expected Found, got {state:?}");
    };
    assert_eq!(matches.len(), 3);
    state
}

/// Excluding the disc-ID signal leaves the barcode side's two results alone;
/// re-including it puts the disc-ID result back in the set.
#[test]
fn toggle_excludes_discid_then_re_includes() {
    let disagreeing = driven_disagreement();

    let (state, effects) = step(
        disagreeing,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Disc,
        },
    );
    assert!(
        effects.is_empty(),
        "toggle re-combines in place, no lookups"
    );
    match &state {
        IdentifyState::Found {
            matches,
            provenance,
            ..
        } => {
            assert_eq!(matches.len(), 2);
            assert!(provenance.iter().all(|p| p.by_barcode && !p.by_disc_id));
        }
        other => panic!("expected Found, got {other:?}"),
    }
    // The disc-ID badge reports its exclusion.
    let disc = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert!(disc.excluded);

    // Re-including the disc-ID signal puts its result back in the set.
    let (restored, _) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Disc,
        },
    );
    let IdentifyState::Found { matches, .. } = &restored else {
        panic!("expected Found after re-include, got {restored:?}");
    };
    assert_eq!(matches.len(), 3);
}

/// Toggling mid-triangulation records the exclusion without collapsing the
/// in-flight lookups: the state stays `Triangulating`, the badge reads excluded,
/// and the exclusion is honored once the lookups settle.
#[test]
fn toggle_during_triangulation_keeps_looking_up() {
    let (state, effects) = update(
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
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupDiscid { .. })));

    // Toggling mid-lookup must not collapse to a terminal state or dispatch.
    let (state, effects) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Disc,
        },
    );
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "toggling mid-lookup must stay Triangulating, got {state:?}",
    );
    assert!(effects.is_empty(), "toggle dispatches no lookups");
    let disc = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert!(disc.excluded, "disc badge reads excluded after toggle");

    // Both settle; the exclusion holds, so the disc results don't count.
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))],
            failures: Vec::new(),
        },
    );
    match state {
        IdentifyState::Found {
            provenance,
            matches,
            ..
        } => {
            assert!(provenance[0].by_barcode && !provenance[0].by_disc_id);
            assert_eq!(matches.len(), 1);
            assert_eq!(
                matches[0].release_id,
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
            );
        }
        other => panic!("expected barcode-only Found, got {other:?}"),
    }
}

/// A failure from a signal the user excluded while it was in flight is not
/// part of the active evidence and cannot invalidate the remaining answer.
#[test]
fn excluded_in_flight_disc_failure_does_not_fail_the_barcode_answer() {
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
    let (state, _) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Disc,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Network,
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))],
            failures: Vec::new(),
        },
    );

    assert!(matches!(state, IdentifyState::Found { .. }), "got {state:?}");
    assert!(matches!(
        crate::identify::TerminalVerdict::try_from(state),
        Ok(crate::identify::TerminalVerdict::Found { .. })
    ));
}

/// `ReRun` from a settled state resets to `Triangulating` and re-dispatches both
/// lookups from the retained signals.
#[test]
fn rerun_re_dispatches_lookups() {
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
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            failures: Vec::new(),
        },
    );
    assert!(matches!(state, IdentifyState::Found { .. }));

    let (rerun_state, effects) = step(state, IdentifyEvent::ReRun);
    assert!(
        matches!(rerun_state, IdentifyState::Triangulating { .. }),
        "expected Triangulating after re-run, got {rerun_state:?}",
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::LookupDiscid { disc_id, .. } if disc_id == "d")),
        "re-run re-dispatches the disc-ID lookup, got {effects:?}",
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "BAR")),
        "re-run re-dispatches the barcode lookup, got {effects:?}",
    );
}

/// A skip-only candidate (disc `Absent`, no barcodes) dispatches no lookups, so a
/// re-run has to settle immediately rather than park in `Triangulating` waiting for
/// results that will never arrive.
#[test]
fn rerun_with_nothing_to_look_up_settles() {
    let (settled, effects) = update(
        started(),
        signals(
            DiscIdSignal::Absent { track_count: 5 },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    assert!(effects.is_empty(), "skip-only signals dispatch no lookups");
    assert!(
        !matches!(settled, IdentifyState::Triangulating { .. }),
        "skip-only signals settle on first snapshot, got {settled:?}",
    );
    let settled_kind = std::mem::discriminant(&settled);

    let (rerun_state, rerun_effects) = step(settled, IdentifyEvent::ReRun);
    assert!(
        rerun_effects.is_empty(),
        "re-run of a skip-only candidate dispatches no lookups, got {rerun_effects:?}",
    );
    assert!(
        !matches!(rerun_state, IdentifyState::Triangulating { .. }),
        "re-run must settle, not park in Triangulating, got {rerun_state:?}",
    );
    assert_eq!(
        std::mem::discriminant(&rerun_state),
        settled_kind,
        "re-run re-settles to the same terminal state",
    );
}

/// Artwork scanned that held no barcode is `Settled { codes: [] }`, which is a
/// different thing from `Absent` (nothing to scan) — the signal type says so. Both
/// carry an empty code vec, so a re-run that re-derives the barcode pipe from the
/// codes alone cannot tell them apart, and used to settle the scanned-nothing case
/// as `Skipped` on re-run where the first pass settled it as a no-match. Same
/// inputs, different answer depending on whether the user pressed Re-run.
#[test]
fn rerun_of_scanned_but_empty_barcode_settles_the_same_as_the_first_pass() {
    let (settled, effects) = update(
        started(),
        signals(
            DiscIdSignal::Absent { track_count: 5 },
            BarcodeSignal::Settled { codes: Vec::new() },
            &[],
        ),
    );
    assert!(effects.is_empty(), "no codes to look up");
    let first_pass = std::mem::discriminant(&settled);

    let (rerun_state, rerun_effects) = step(settled, IdentifyEvent::ReRun);
    assert!(rerun_effects.is_empty(), "still nothing to look up");
    assert_eq!(
        std::mem::discriminant(&rerun_state),
        first_pass,
        "re-run must settle where the first pass did, got {rerun_state:?}",
    );
}

/// A re-run replays both lookups regardless of exclusions, but the re-derive that
/// follows still masks the excluded side: exclude the disc, re-run, and the state
/// settles back to the barcode's two results rather than all three.
#[test]
fn rerun_preserves_exclusions() {
    let disagreeing = driven_disagreement();
    // Exclude the disc-ID signal: leaves the barcode's own results.
    let (excluded, _) = step(
        disagreeing,
        IdentifyEvent::SignalToggled {
            signal: SignalToggle::Disc,
        },
    );
    assert!(matches!(excluded, IdentifyState::Found { .. }));

    // Re-run, then re-settle both lookups. The disc exclusion survives, so the
    // re-derived set is the barcode's two results again, not all three.
    let (state, _) = step(excluded, IdentifyEvent::ReRun);
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 7,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![
                pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y")),
                pair("rel-3", Some("g-y")),
            ],
            failures: Vec::new(),
        },
    );
    match state {
        IdentifyState::Found { provenance, .. } => {
            assert!(provenance.iter().all(|p| p.by_barcode && !p.by_disc_id));
        }
        other => panic!("expected barcode-only Found after re-run, got {other:?}"),
    }
}

// MARK: - Toolbar projection

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
    assert!(matches!(
        effects.as_slice(),
        [Effect::LookupCatalog { catalog }] if catalog == "LBL 001"
    ));
    let catalog_badge = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Catalog)
        .expect("catalog badge");
    assert_eq!(catalog_badge.value.as_deref(), Some("LBL 001"));
    assert_eq!(catalog_badge.state, SignalState::LookingUp);

    let (state, _) = step(
        state,
        IdentifyEvent::CatalogLookupCompleted {
            for_catalog: "LBL 001".to_string(),
            results: vec![pair("rel-b", Some("g-x"))],
            failures: Vec::new(),
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
        IdentifyEvent::CatalogLookupCompleted {
            for_catalog: "LBL 001".to_string(),
            results: vec![pair("rel-b", Some("g-x"))],
            failures: Vec::new(),
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
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "012345678905".to_string(),
            failures: vec![source_failure.clone()],
        },
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
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "012345678905".to_string(),
            failures: vec![source_failure.clone()],
        },
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
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "BAR".to_string(),
            failures: vec![source_failure.clone()],
        },
    );
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
