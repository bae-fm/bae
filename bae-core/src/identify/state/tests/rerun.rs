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
        barcode_matched(
            MB,
            "BAR",
            vec![
                pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y")),
                pair("rel-3", Some("g-y")),
            ],
        ),
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
        barcode_matched(
            MB,
            "BAR",
            vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))],
        ),
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
        barcode_matched(
            MB,
            "BAR",
            vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))],
        ),
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
        barcode_matched(
            MB,
            "BAR",
            vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
        ),
    );
    assert!(matches!(state, IdentifyState::Found { .. }));

    let (rerun_state, effects) = step(state, IdentifyEvent::ReRun { providers: vec![MB] });
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
        effects.contains(&lookup_barcode(MB, "BAR")),
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

    let (rerun_state, rerun_effects) = step(settled, IdentifyEvent::ReRun { providers: vec![MB] });
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

    let (rerun_state, rerun_effects) = step(settled, IdentifyEvent::ReRun { providers: vec![MB] });
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
    let (state, _) = step(excluded, IdentifyEvent::ReRun { providers: vec![MB] });
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 7,
        },
    );
    let (state, _) = step(
        state,
        barcode_matched(
            MB,
            "BAR",
            vec![
                pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y")),
                pair("rel-3", Some("g-y")),
            ],
        ),
    );
    match state {
        IdentifyState::Found { provenance, .. } => {
            assert!(provenance.iter().all(|p| p.by_barcode && !p.by_disc_id));
        }
        other => panic!("expected barcode-only Found after re-run, got {other:?}"),
    }
}

/// A re-run reads the provider list again, so a provider configured since the
/// last run joins it.
#[test]
fn a_rerun_picks_up_a_newly_configured_provider() {
    let (state, effects) = update(started(), disc_and_codes("d", &["BAR"]));
    assert_eq!(
        effects
            .iter()
            .filter(|e| matches!(e, Effect::LookupBarcode { .. }))
            .count(),
        1,
        "one provider in the run asks once"
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-a", Some("g-x"))],
            track_count: 5,
        },
    );
    let (found, _) = step(
        state,
        barcode_matched(MB, "BAR", vec![pair("rel-a", Some("g-x"))]),
    );
    assert!(matches!(found, IdentifyState::Found { .. }));

    let (state, effects) = step(
        found,
        IdentifyEvent::ReRun {
            providers: vec![MB, DG],
        },
    );
    assert!(effects.contains(&lookup_barcode(MB, "BAR")));
    assert!(effects.contains(&lookup_barcode(DG, "BAR")));
    assert_eq!(barcode_walks(&state).len(), 2);
}
