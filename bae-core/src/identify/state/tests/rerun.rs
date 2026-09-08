/// A run started with the disc ID left out never asks about it: no disc-ID
/// lookup goes out, the badge says the person took it out, and the answer is
/// the barcode's alone.
#[test]
fn a_run_that_leaves_the_disc_id_out_never_asks_about_it() {
    let (state, effects) = started_with_choices(vec![MB], excluding(true, false));
    let (state, effects) = {
        assert!(effects.is_empty());
        update(state, disc_and_codes("d", &["BAR"]))
    };
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupDiscid { .. })),
        "the disc ID was left out, so nothing asks about it: {effects:?}"
    );
    assert!(effects.contains(&lookup_barcode(MB, "BAR")));

    let disc = badge(&state, SignalKind::DiscId);
    assert!(disc.excluded, "the disc badge reads as left out");
    assert_eq!(disc.state, SignalState::Skipped);

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
            assert_eq!(matches.len(), 1);
            assert!(provenance[0].by_barcode && !provenance[0].by_disc_id);
        }
        other => panic!("expected a barcode-only Found, got {other:?}"),
    }
}

/// The same for the barcode: nothing is asked about the codes, and the disc
/// ID's answer stands alone.
#[test]
fn a_run_that_leaves_the_barcode_out_never_asks_about_it() {
    let (state, _) = started_with_choices(vec![MB], excluding(false, true));
    let (state, effects) = update(state, disc_and_codes("d", &["BAR"]));
    assert!(
        !effects.iter().any(|effect| matches!(
            effect,
            Effect::LookupBarcode { .. }
        )),
        "the barcode was left out, so nothing asks about it: {effects:?}"
    );
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::LookupDiscid { .. })));

    let barcode = badge(&state, SignalKind::Barcode);
    assert!(barcode.excluded, "the barcode badge reads as left out");
    assert_eq!(barcode.state, SignalState::Skipped);

    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 5,
        },
    );
    match state {
        IdentifyState::Found {
            provenance,
            matches,
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
        }
        other => panic!("expected a disc-only Found, got {other:?}"),
    }
}

/// A signal the run left out contributes no failure either: the barcode
/// answered, the disc ID's excluded lookup could not have run, and the run
/// lands on the barcode's answer rather than on a failure.
#[test]
fn an_excluded_disc_id_cannot_fail_the_barcode_answer() {
    let (state, _) = started_with_choices(vec![MB], excluding(true, false));
    let (state, _) = update(state, disc_and_codes("d", &["BAR"]));
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
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
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

/// A re-run replays the run the person asked for, which is the run they chose
/// what to ask about: the disc ID they left out is still left out afterwards,
/// so the re-derived set is the barcode's two results rather than all three.
#[test]
fn a_rerun_keeps_the_choices_the_run_started_with() {
    let (state, _) = started_with_choices(vec![MB], excluding(true, false));
    let (state, _) = update(
        state,
        signals(
            disc("d", 7),
            BarcodeSignal::Settled {
                codes: artwork_codes(&["BAR"]),
            },
            &[],
        ),
    );
    let (excluded, _) = step(
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
    assert!(matches!(excluded, IdentifyState::Found { .. }));

    let (state, effects) = step(excluded, IdentifyEvent::ReRun { providers: vec![MB] });
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupDiscid { .. })),
        "the re-run leaves out what the run left out: {effects:?}"
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

/// A source switched off mid-run is what the run has to notice. `ReRun` is
/// ignored while the lookups are in flight — a person replaying a settled run
/// is not asking to restart what is already out — but `ProvidersChanged` is
/// exactly a statement about the list those lookups were dispatched against,
/// so the run re-lays itself over what is left rather than waiting on a source
/// nobody is asking.
#[test]
fn a_changed_provider_list_re_lays_a_run_still_in_flight() {
    let (state, effects) = update(started_with(vec![MB, DG]), disc_and_codes("d", &["BAR"]));
    assert_eq!(barcode_walks(&state).len(), 2);
    assert!(effects.contains(&lookup_barcode(MB, "BAR")));
    assert!(effects.contains(&lookup_barcode(DG, "BAR")));
    assert!(matches!(state, IdentifyState::Triangulating { .. }));

    // A person's replay is turned down while the lookups are out.
    let (state, replay_effects) = step(
        state,
        IdentifyEvent::ReRun {
            providers: vec![DG],
        },
    );
    assert!(replay_effects.is_empty());
    assert_eq!(barcode_walks(&state).len(), 2);

    // The list changing is not.
    let (state, effects) = step(
        state,
        IdentifyEvent::ProvidersChanged {
            providers: vec![DG],
        },
    );
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    assert_eq!(
        barcode_walks(&state)
            .iter()
            .map(|walk| walk.source)
            .collect::<Vec<_>>(),
        vec![DG],
        "only the sources still being asked have a walk"
    );
    assert_eq!(effects, vec![lookup_barcode(DG, "BAR")]);
}

/// The dropped source's lookup was already out. It lands on a run that has no
/// cell for it, so it changes nothing — the run is not reopened on a source
/// nobody is asking.
#[test]
fn a_dropped_source_s_late_answer_lands_nowhere() {
    let (state, _) = update(started_with(vec![MB, DG]), disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        IdentifyEvent::ProvidersChanged {
            providers: vec![DG],
        },
    );

    let (after, effects) = step(
        state.clone(),
        barcode_matched(MB, "BAR", vec![pair("rel-a", Some("g-x"))]),
    );

    assert!(effects.is_empty());
    assert_eq!(after, state, "the dropped source's answer changed nothing");
}
