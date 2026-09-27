/// A disc ID left out is never asked about, its badge says so, and the answer
/// is the barcode's alone.
#[test]
fn a_run_that_leaves_the_disc_id_out_never_asks_about_it() {
    let (state, effects) = started_with_choices(vec![MB], excluding(true, &[]));
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
    assert_eq!(
        disc.state,
        SignalState::NotAsked {
            reason: crate::identify::NotAskedReason::LeftOut
        }
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
            findings:
                Findings {
                    provenance,
                    matches,
                    ..
                },
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert!(provenance[0].by_barcode && !provenance[0].by_disc_id);
        }
        other => panic!("expected a barcode-only Found, got {other:?}"),
    }
}

/// A barcode left out is never asked about, and the answer is the disc ID's
/// alone.
#[test]
fn a_run_that_leaves_the_barcode_out_never_asks_about_it() {
    let (state, _) = started_with_choices(vec![MB], excluding(false, &["BAR"]));
    let (state, effects) = update(state, disc_and_codes("d", &["BAR"]));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupBarcode { .. })),
        "the barcode was left out, so nothing asks about it: {effects:?}"
    );
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::LookupDiscid { .. })));

    let barcode = badge(&state, SignalKind::Barcode);
    assert!(barcode.excluded, "the barcode badge reads as left out");
    assert_eq!(
        barcode.state,
        SignalState::NotAsked {
            reason: crate::identify::NotAskedReason::LeftOut
        }
    );

    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 5,
        },
    );
    match state {
        IdentifyState::Found {
            findings:
                Findings {
                    provenance,
                    matches,
                    ..
                },
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
        }
        other => panic!("expected a disc-only Found, got {other:?}"),
    }
}

/// A disc ID left out adds no failure, so the run lands on the barcode's
/// answer.
#[test]
fn an_excluded_disc_id_cannot_fail_the_barcode_answer() {
    let (state, _) = started_with_choices(vec![MB], excluding(true, &[]));
    let (state, _) = update(state, disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        barcode_matched(
            MB,
            "BAR",
            vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))],
        ),
    );

    assert!(
        matches!(state, IdentifyState::Found { .. }),
        "got {state:?}"
    );
    assert!(matches!(
        crate::identify::TerminalVerdict::try_from(state),
        Ok(crate::identify::TerminalVerdict::Found { .. })
    ));
}

/// An answer from a provider the run never asked changes nothing.
#[test]
fn an_answer_from_a_source_the_run_never_asked_lands_nowhere() {
    let (state, effects) = update(started_with(vec![DG]), disc_and_codes("d", &["BAR"]));
    assert_eq!(effects, vec![lookup_barcode(DG, "BAR")]);
    let IdentifyState::Triangulating { barcode, .. } = &state else {
        panic!("expected the run looking up, got {state:?}");
    };
    assert_eq!(
        barcode.lookups()[0]
            .providers
            .iter()
            .map(|lookup| lookup.source)
            .collect::<Vec<_>>(),
        vec![DG],
        "only the sources the run asks have a lookup"
    );

    let (after, effects) = step(
        state.clone(),
        barcode_matched(MB, "BAR", vec![pair("rel-a", Some("g-x"))]),
    );

    assert!(effects.is_empty());
    assert_eq!(after, state, "the unasked source's answer changed nothing");
}

/// Leaving one of two codes out asks only about the other, and the badge's
/// options say which.
#[test]
fn leaving_one_of_two_codes_out_asks_only_about_the_other() {
    let (state, _) = started_with_choices(vec![MB], excluding(false, &["BOXSET"]));
    let (state, effects) = update(state, disc_and_codes("d", &["BOXSET", "DISC"]));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::LookupBarcode { .. }))
            .collect::<Vec<_>>(),
        vec![&lookup_barcode(MB, "DISC")],
        "only the code the run still asks about is asked: {effects:?}"
    );

    let barcode = badge(&state, SignalKind::Barcode);
    assert!(
        !barcode.excluded,
        "one code left out of two is not the signal left out"
    );
    assert_eq!(
        barcode
            .options
            .iter()
            .map(|option| (option.value.as_str(), option.chosen))
            .collect::<Vec<_>>(),
        vec![("BOXSET", false), ("DISC", true)]
    );
}

/// Leaving every code out asks about none, keeps the codes, and the badge says
/// they were left out.
#[test]
fn leaving_every_code_out_asks_about_none_of_them() {
    let (state, _) = started_with_choices(vec![MB], excluding(false, &["BOXSET", "DISC"]));
    let (state, effects) = update(state, disc_and_codes("d", &["BOXSET", "DISC"]));
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupBarcode { .. })),
        "nothing asks about a code the candidate says to leave out: {effects:?}"
    );
    match &state {
        IdentifyState::Triangulating { barcode, .. } => assert_eq!(
            barcode,
            &BarcodeProgress::NotAsked {
                codes: vec!["BOXSET".to_string(), "DISC".to_string()],
                reason: crate::identify::NotAskedReason::LeftOut,
            }
        ),
        other => panic!("expected the barcode pipe settled unasked, got {other:?}"),
    }

    let barcode = badge(&state, SignalKind::Barcode);
    assert!(
        barcode.excluded,
        "no code is asked about, so the badge says so"
    );
    assert_eq!(
        barcode.state,
        SignalState::NotAsked {
            reason: crate::identify::NotAskedReason::LeftOut
        }
    );
    assert!(barcode.options.iter().all(|option| !option.chosen));
}
