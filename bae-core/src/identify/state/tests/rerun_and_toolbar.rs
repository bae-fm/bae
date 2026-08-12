/// Drive the reducer through triangulation into a conflict: disc-id and
/// barcode each match a single release, but on different groups. Returns
/// the `Conflict` state.
fn driven_conflict() -> IdentifyState {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 7,
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
        },
    );
    assert!(
        matches!(state, IdentifyState::Conflict { .. }),
        "expected Conflict, got {state:?}",
    );
    state
}

/// Excluding the disc-ID signal from a conflict collapses to the barcode
/// side's single coherent group; re-including it restores the conflict.
#[test]
fn toggle_excludes_discid_then_re_includes() {
    let conflict = driven_conflict();

    let (state, effects) = step(
        conflict,
        IdentifyEvent::SignalToggled {
            signal: ExcludedSignal::Disc,
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

    // Re-including the disc-ID signal restores the empty-intersection
    // conflict.
    let (restored, _) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: ExcludedSignal::Disc,
        },
    );
    assert!(
        matches!(restored, IdentifyState::Conflict { .. }),
        "expected Conflict after re-include, got {restored:?}",
    );
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
            signal: ExcludedSignal::Disc,
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
/// settles back to barcode-only `Found` rather than to the original conflict.
#[test]
fn rerun_preserves_exclusions() {
    let conflict = driven_conflict();
    // Exclude the disc-ID signal: lands on barcode-only Found.
    let (excluded, _) = step(
        conflict,
        IdentifyEvent::SignalToggled {
            signal: ExcludedSignal::Disc,
        },
    );
    assert!(matches!(excluded, IdentifyState::Found { .. }));

    // Re-run, then re-settle both lookups. The disc exclusion survives,
    // so the re-derived state is barcode-only Found again, not a conflict.
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
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &["LBL-001"],
        ),
    );
    let toolbar = state.toolbar();
    // disc, barcode, one catalog.
    assert_eq!(toolbar.len(), 3);

    let disc = &toolbar[0];
    assert_eq!(disc.kind, SignalKind::DiscId);
    assert_eq!(disc.role, SignalRole::Identity);
    assert_eq!(disc.value.as_deref(), Some("disc-hash"));
    assert_eq!(disc.origin, SignalOrigin::DiscToc);
    assert_eq!(disc.state, SignalState::LookingUp);

    let barcode = &toolbar[1];
    assert_eq!(barcode.kind, SignalKind::Barcode);
    assert_eq!(barcode.value.as_deref(), Some("012345678905"));
    assert_eq!(barcode.state, SignalState::LookingUp);

    let catalog = &toolbar[2];
    assert_eq!(catalog.kind, SignalKind::Catalog);
    assert_eq!(catalog.role, SignalRole::Filter);
    assert_eq!(catalog.value.as_deref(), Some("LBL-001"));
}

#[test]
fn toolbar_found_reports_counts_and_catalog_confirms() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Absent,
            &["LBL 001", "LBL 999"],
        ),
    );
    let mut r_a = mk_result("rel-a", Some("g-x"));
    r_a.catalog_number = Some("LBL-001".to_string());
    let mut r_b = mk_result("rel-b", Some("g-x"));
    r_b.catalog_number = Some("LBL-002".to_string());
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![(r_a, mk_status("rel-a")), (r_b, mk_status("rel-b"))],
            track_count: 5,
        },
    );
    assert!(matches!(state, IdentifyState::Found { .. }));

    let toolbar = state.toolbar();
    let disc = toolbar
        .iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert_eq!(disc.state, SignalState::Found { count: 2 });

    let barcode = toolbar
        .iter()
        .find(|s| s.kind == SignalKind::Barcode)
        .expect("barcode badge");
    assert_eq!(barcode.state, SignalState::Skipped);
    assert_eq!(barcode.value, None);

    let catalogs: Vec<&ToolbarSignal> = toolbar
        .iter()
        .filter(|s| s.kind == SignalKind::Catalog)
        .collect();
    assert_eq!(catalogs.len(), 2);
    // `LBL 001` confirms rel-a (catno LBL-001 normalizes equal).
    let confirming = catalogs
        .iter()
        .find(|c| c.value.as_deref() == Some("LBL 001"))
        .expect("confirming catalog");
    assert_eq!(confirming.state, SignalState::Confirms { count: 1 });
    // `LBL 999` confirms nothing.
    let noise = catalogs
        .iter()
        .find(|c| c.value.as_deref() == Some("LBL 999"))
        .expect("noise catalog");
    assert_eq!(noise.state, SignalState::Confirms { count: 0 });
}

#[test]
fn toolbar_artwork_catalog_does_not_confirm() {
    let (state, _) = update(
        started(),
        signals_with_catalogs(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Absent,
            vec![SourcedValue::new(
                "LBL 001".to_string(),
                SignalOrigin::Artwork,
            )],
        ),
    );
    let mut r_a = mk_result("rel-a", Some("g-x"));
    r_a.catalog_number = Some("LBL-001".to_string());
    let mut r_b = mk_result("rel-b", Some("g-x"));
    r_b.catalog_number = Some("LBL-002".to_string());
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![(r_a, mk_status("rel-a")), (r_b, mk_status("rel-b"))],
            track_count: 5,
        },
    );
    assert!(matches!(state, IdentifyState::Found { .. }));

    let toolbar = state.toolbar();
    let catalog = toolbar
        .iter()
        .find(|s| s.kind == SignalKind::Catalog)
        .expect("catalog badge");
    assert_eq!(catalog.origin, SignalOrigin::Artwork);
    assert_eq!(catalog.state, SignalState::Confirms { count: 0 });
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
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "012345678905".to_string(),
            failure: failure.clone(),
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
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "012345678905".to_string(),
            failure: failure.clone(),
        },
    );
    assert!(matches!(state, IdentifyState::NotFoundAnywhere { .. }));
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
    assert!(matches!(state, IdentifyState::NotFoundAnywhere { .. }));
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
            signal: ExcludedSignal::Disc,
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
            signal: ExcludedSignal::Disc,
        },
    );
    assert!(
        matches!(restored, IdentifyState::Found { .. }),
        "re-including the disc signal restores Found, got {restored:?}"
    );
}

/// A barcode lookup can fail while the disc-ID lookup is still in flight. The
/// pipeline stays `Triangulating` until the disc settles, then combines over
/// the disc results while retaining the barcode failure in the context (so the
/// barcode badge still reads Failed on the terminal state).
#[test]
fn barcode_failure_before_disc_settles_is_retained_through_combine() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["BAR"]),
            },
            &[],
        ),
    );

    let failure = LookupFailure::Provider { status: Some(500) };
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "BAR".to_string(),
            failure: failure.clone(),
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

    match &state {
        IdentifyState::Found { context, .. } => {
            assert_eq!(context.barcode_failure.as_ref(), Some(&failure));
        }
        other => panic!("expected Found from the disc results, got {other:?}"),
    }
    // The terminal toolbar surfaces the retained barcode failure.
    let barcode = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::Barcode)
        .expect("barcode badge");
    assert_eq!(barcode.state, SignalState::Failed { failure });
}
