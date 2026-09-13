/// A run started with the disc ID left out never asks about it: no disc-ID
/// lookup goes out, the badge says the person took it out, and the answer is
/// the barcode's alone.
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
    let (state, _) = started_with_choices(vec![MB], excluding(false, &["BAR"]));
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

    assert!(matches!(state, IdentifyState::Found { .. }), "got {state:?}");
    assert!(matches!(
        crate::identify::TerminalVerdict::try_from(state),
        Ok(crate::identify::TerminalVerdict::Found { .. })
    ));
}

/// The run asks the providers it started with and no others. An answer from a
/// source this run never asked lands on no cell, so it changes nothing — the
/// run is not reopened on a provider it is not asking.
#[test]
fn an_answer_from_a_source_the_run_never_asked_lands_nowhere() {
    let (state, effects) = update(started_with(vec![DG]), disc_and_codes("d", &["BAR"]));
    assert_eq!(effects, vec![lookup_barcode(DG, "BAR")]);
    assert_eq!(
        barcode_walks(&state)
            .iter()
            .map(|walk| walk.source)
            .collect::<Vec<_>>(),
        vec![DG],
        "only the sources the run asks have a walk"
    );

    let (after, effects) = step(
        state.clone(),
        barcode_matched(MB, "BAR", vec![pair("rel-a", Some("g-x"))]),
    );

    assert!(effects.is_empty());
    assert_eq!(after, state, "the unasked source's answer changed nothing");
}

/// A sleeve prints the box set's code beside the disc's. Leaving one of the two
/// out asks only about the other: the walks start on the code the run still
/// asks about, and the badge's options say which of the two that is.
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

/// Leaving every code out asks about none of them. The codes are still the
/// run's, so the pipe settles carrying them with nothing run against them, and
/// the badge reads as left out.
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
            }
        ),
        other => panic!("expected the barcode pipe settled unasked, got {other:?}"),
    }

    let barcode = badge(&state, SignalKind::Barcode);
    assert!(barcode.excluded, "no code is asked about, so the badge says so");
    assert_eq!(barcode.state, SignalState::Skipped);
    assert!(barcode.options.iter().all(|option| !option.chosen));
}
