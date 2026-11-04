//! How each key's lookups go out and settle: the disc IDs, the barcodes and
//! the ISRCs, and what a run leaves out.

use super::*;
use crate::state::tests::step;
use crate::state::tests::*;
use crate::state::*;
use crate::IdentifyFailure;
use crate::{IdentifyStateView, IsrcStepView, LookupView, NotAskedReason};

// The ISRC step: every code the audio's tags carry, asked of MusicBrainz in
// one search.

/// Each code is asked once, in one search, however many files carry it and
/// however many snapshots stream; the run waits on the answer, and what it
/// returns is found by the ISRCs.
#[test]
fn the_tags_isrcs_are_asked_once_in_one_search() {
    let codes = ["XX0000000002", "XX0000000001", "XX0000000002"];
    let (state, effects) = update(started(), tagged(&codes));
    assert_eq!(effects, vec![lookup_isrcs(&["XX0000000002", "XX0000000001"])]);
    let (state, effects) = update(state, tagged(&codes));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(!state.is_terminal(), "the run waits on the ISRCs");

    let (state, _) = step(
        state,
        IdentifyEvent::IsrcLookupAnswered { outcome: Ok(vec![pair("rel-a", Some("g-x"))]) },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    assert_eq!(findings.matches[0].release_id, "rel-a");
}

/// With MusicBrainz not asked, the codes are laid out and nobody is asked
/// about them, so the run offers a manual search.
#[test]
fn a_run_without_musicbrainz_asks_nobody_about_the_isrcs() {
    let (state, effects) = update(started_with(vec![DG]), tagged(&["XX0000000001"]));
    assert!(effects.is_empty(), "{effects:?}");
    let run = match IdentifyStateView::from(state) {
        IdentifyStateView::ManualOnly { run, .. } => run.expect("the codes are laid out"),
        other => panic!("expected ManualOnly, got {other:?}"),
    };
    assert_eq!(
        run.isrc,
        IsrcStepView::Read {
            isrcs: vec!["XX0000000001".to_string()],
            lookup: LookupView::NotAsked { reason: NotAskedReason::NoCatalog },
        }
    );
}

/// The search failing fails the run with the ISRCs named, and the ledger says
/// so; tags with no code lay out no lookup.
#[test]
fn a_failed_isrc_search_fails_the_run() {
    let (state, _) = update(started(), tagged(&["XX0000000001"]));
    let (state, _) =
        step(state, IdentifyEvent::IsrcLookupAnswered { outcome: Err(LookupFailure::Timeout) });
    let IdentifyState::Failed { failures, ledger, .. } = &state else {
        panic!("expected Failed, got {state:?}");
    };
    assert_eq!(failures, &vec![IdentifyFailure::Isrc(LookupFailure::Timeout)]);
    assert!(matches!(
        ledger.as_ref().map(|run| &run.isrc),
        Some(IsrcStepView::Read {
            lookup: LookupView::Failed { failure: LookupFailure::Timeout },
            ..
        })
    ));

    let (state, _) = update(started(), one_code("012345678905"));
    let IdentifyStateView::Triangulating { run, .. } = IdentifyStateView::from(state) else {
        panic!("expected Triangulating");
    };
    assert_eq!(run.isrc, IsrcStepView::Absent);
}

/// A disc ID left out is never asked about, its ledger says so, and the answer
/// is the barcode's alone.
#[test]
fn a_run_that_leaves_the_disc_id_out_never_asks_about_it() {
    let (state, effects) = started_with_choices(vec![MB], excluding(true, &[]));
    let (state, effects) = {
        assert!(effects.is_empty());
        update(state, disc_and_codes("d", &["BAR"]))
    };
    assert!(
        !effects.iter().any(|effect| matches!(effect, Effect::LookupDiscid { .. })),
        "the disc ID was left out, so nothing asks about it: {effects:?}"
    );
    assert!(effects.contains(&lookup_barcode(MB, "BAR")));

    assert_eq!(
        disc_lookups(&state),
        [LookupView::NotAsked { reason: NotAskedReason::LeftOut }],
        "the ledger shows the disc ID left out"
    );

    let (state, _) = step(
        state,
        barcode_matched(MB, "BAR", vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))]),
    );
    match state {
        IdentifyState::Found { findings: Findings { matches, .. }, ledger, .. } => {
            assert_eq!(matches.len(), 1);
            assert!(!found_by_disc_id(ledger.as_ref(), &matches[0]));
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
        !effects.iter().any(|effect| matches!(effect, Effect::LookupBarcode { .. })),
        "the barcode was left out, so nothing asks about it: {effects:?}"
    );
    assert!(effects.iter().any(|effect| matches!(effect, Effect::LookupDiscid { .. })));

    let rows = barcode_rows(&state);
    assert_eq!(left_out(&rows), [("BAR", true)], "the ledger shows the barcode left out");
    assert_eq!(cell(&rows[0], MB), &LookupView::NotAsked { reason: NotAskedReason::LeftOut });

    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![pair(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("g-x"),
            )])),
        },
    );
    match state {
        IdentifyState::Found { findings: Findings { matches, .. }, ledger, .. } => {
            assert_eq!(matches.len(), 1);
            assert!(found_by_disc_id(ledger.as_ref(), &matches[0]));
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
        barcode_matched(MB, "BAR", vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))]),
    );

    assert!(matches!(state, IdentifyState::Found { .. }), "got {state:?}");
    assert!(matches!(
        crate::TerminalVerdict::try_from(state),
        Ok(crate::TerminalVerdict::Found { .. })
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
        barcode.lookups()[0].providers.iter().map(|lookup| lookup.source).collect::<Vec<_>>(),
        vec![DG],
        "only the sources the run asks have a lookup"
    );

    let (after, effects) =
        step(state.clone(), barcode_matched(MB, "BAR", vec![pair("rel-a", Some("g-x"))]));

    assert!(effects.is_empty());
    assert_eq!(after, state, "the unasked source's answer changed nothing");
}

/// Leaving one of two codes out asks only about the other, and the ledger
/// says which.
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

    assert_eq!(left_out(&barcode_rows(&state)), [("BOXSET", true), ("DISC", false)]);
}

/// Leaving every code out asks about none, keeps the codes, and the ledger says
/// they were left out.
#[test]
fn leaving_every_code_out_asks_about_none_of_them() {
    let (state, _) = started_with_choices(vec![MB], excluding(false, &["BOXSET", "DISC"]));
    let (state, effects) = update(state, disc_and_codes("d", &["BOXSET", "DISC"]));
    assert!(
        !effects.iter().any(|effect| matches!(effect, Effect::LookupBarcode { .. })),
        "nothing asks about a code the candidate says to leave out: {effects:?}"
    );
    match &state {
        IdentifyState::Triangulating { barcode, .. } => assert_eq!(
            barcode,
            &BarcodeProgress::NotAsked {
                codes: vec!["BOXSET".to_string(), "DISC".to_string()],
                reason: crate::NotAskedReason::LeftOut,
            }
        ),
        other => panic!("expected the barcode pipe settled unasked, got {other:?}"),
    }

    assert_eq!(left_out(&barcode_rows(&state)), [("BOXSET", true), ("DISC", true)]);
}

/// A folder of two discs asks about each disc's ID, settles once both have
/// answered, and offers the release that holds both discs over one that
/// holds only the first.
#[test]
fn every_disc_id_is_asked_and_a_release_holding_every_disc_ranks_first() {
    let mut folder = signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &[]);
    folder.disc_id = DiscIdSignal::Computed {
        discs: ["disc-one", "disc-two"]
            .into_iter()
            .map(|disc_id| crate::signals::ComputedDiscId {
                disc_id: disc_id.to_string(),
                source_file: None,
            })
            .collect(),
    };
    let (state, effects) = update(started(), folder);
    assert_eq!(
        effects,
        vec![
            Effect::LookupDiscid { disc_id: "disc-one".into() },
            Effect::LookupDiscid { disc_id: "disc-two".into() },
        ],
        "each disc's ID is looked up"
    );

    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-one".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                pair("both-discs", Some("g")),
                pair("first-disc-only", Some("g")),
            ])),
        },
    );
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "the run waits for the second disc's answer: {state:?}"
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-two".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![pair("both-discs", Some("g"))])),
        },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("both answers settle the run: {state:?}");
    };
    assert_eq!(
        findings.matches.iter().map(|release| release.release_id.as_str()).collect::<Vec<_>>(),
        vec!["both-discs"]
    );
    assert_eq!(
        findings
            .narrowed_out
            .matches
            .iter()
            .map(|release| release.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["first-disc-only"]
    );
}

/// The disc-ID lookup is dispatched exactly once, even as snapshots stream.
#[test]
fn disc_computed_dispatches_lookup_idempotently() {
    let snapshot = || signals(disc("d"), BarcodeSignal::Scanning { codes: vec![] }, &[]);
    let (state, effects) = update(started(), snapshot());
    assert!(effects.iter().any(|e| matches!(e, Effect::LookupDiscid { .. })));
    // A repeated snapshot must not re-dispatch the lookup.
    let (_, effects) = update(state, snapshot());
    assert!(effects.is_empty(), "disc-id lookup dispatched only once, got {effects:?}");
}

/// Barcode lookups go out only once the codes have settled, never while
/// scanning.
#[test]
fn barcode_lookups_start_only_from_settled() {
    let (state, effects) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Scanning { codes: artwork_codes(&["A"]) },
            &[],
        ),
    );
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::LookupBarcode { .. })),
        "no barcode lookup while still scanning"
    );
    let (_, effects) = update(
        state,
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled { codes: artwork_codes(&["A", "B"]) },
            &[],
        ),
    );
    // Every provider is asked about every code at once.
    assert_eq!(
        effects,
        vec![
            lookup_barcode(MB, "A"),
            lookup_barcode(DG, "A"),
            lookup_barcode(MB, "B"),
            lookup_barcode(DG, "B"),
        ]
    );
}

/// Every code is asked of every provider at once, and a match on one stops
/// nothing.
#[test]
fn every_code_is_asked_of_every_provider() {
    let (state, effects) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled { codes: artwork_codes(&["A", "B"]) },
            &[],
        ),
    );
    assert_eq!(
        effects,
        vec![
            lookup_barcode(MB, "A"),
            lookup_barcode(DG, "A"),
            lookup_barcode(MB, "B"),
            lookup_barcode(DG, "B"),
        ]
    );

    // A match asks nothing more, and the run waits for the other answers.
    let (state, effects) =
        step(state, barcode_matched(DG, "A", vec![discogs_pair("dg-a", Some("g-x"))]));
    assert!(effects.is_empty());
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, _) = step(state, barcode_missed(DG, "B"));
    // The last answer settles the lookups.
    let (state, _) = step(state, barcode_matched(MB, "B", vec![pair("mb-b", Some("g-y"))]));
    let IdentifyState::Found { findings: Findings { matches, .. }, .. } = state else {
        panic!("expected Found");
    };
    // Both codes' answers compete in the one ranking.
    let mut found: Vec<&str> = matches.iter().map(|m| m.release_id.as_str()).collect();
    found.sort_unstable();
    assert_eq!(found, vec!["dg-a", "mb-b"]);
}

/// A second answer from the same provider about the same code changes nothing.
#[test]
fn a_repeated_barcode_answer_is_ignored() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled { codes: artwork_codes(&["A", "B"]) },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, effects) =
        step(state, barcode_failed(MB, "A", LookupFailure::Provider { status: Some(500) }));
    assert!(effects.is_empty());
    match &state {
        IdentifyState::Triangulating { barcode, .. } => {
            assert!(barcode.failures().is_empty(), "the late answer is dropped");
            assert!(!barcode.is_settled(), "B is still out");
        }
        other => panic!("expected the run still looking up, got {other:?}"),
    }
}

/// A provider failing one code settles the barcode lookup failed once every
/// code has answered.
#[test]
fn barcode_lookup_failure_settles_failed() {
    let (state, effects) = update(started(), disc_and_codes("d", &["A", "B"]));
    assert!(effects.contains(&lookup_barcode(MB, "A")));
    assert!(effects.contains(&lookup_barcode(MB, "B")));

    let failure = LookupFailure::Provider { status: Some(500) };
    let source_failure = SourceFailure { source: MB.catalog(), failure: failure.clone() };
    let (state, effects) = step(state, barcode_failed(MB, "A", failure));
    assert!(effects.is_empty());
    let (state, _) = step(state, barcode_missed(MB, "B"));
    match &state {
        IdentifyState::Triangulating { barcode, .. } => {
            assert!(barcode.is_settled());
            assert_eq!(barcode.failures(), vec![source_failure]);
            assert!(barcode.results().is_empty());
        }
        other => panic!("expected the barcode pipe settled failed, got {other:?}"),
    }
}

/// One provider failing does not hide what the other found.
#[test]
fn a_failed_provider_does_not_hide_the_other_s_answer() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled { codes: artwork_codes(&["A", "B"]) },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_failed(DG, "A", LookupFailure::Timeout));
    let (state, _) = step(state, barcode_missed(DG, "B"));
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, _) = step(state, barcode_matched(MB, "B", vec![pair("mb-b", Some("g-y"))]));
    match state {
        IdentifyState::Failed { failures, findings: Findings { matches, .. }, .. } => {
            assert_eq!(
                failures,
                vec![crate::IdentifyFailure::Barcode(SourceFailure {
                    source: DG.catalog(),
                    failure: LookupFailure::Timeout,
                })]
            );
            assert_eq!(matches.len(), 1, "MusicBrainz's match still stands");
        }
        other => panic!("expected Failed with the surviving match, got {other:?}"),
    }
}

/// A run not asking MusicBrainz asks nobody about the disc ID, says why, and
/// with nothing else to go on offers manual search.
#[test]
fn a_run_without_the_disc_id_source_dispatches_no_disc_id_lookup() {
    let state = started_with(vec![LookupCatalog::Discogs]);
    let (next, effects) = update(state, disc_only(&[]));

    assert!(
        !effects.iter().any(|effect| matches!(effect, Effect::LookupDiscid { .. })),
        "no source was asked about the disc ID: {effects:?}"
    );
    let context = next.context().expect("a started run carries its context");
    assert_eq!(context.providers, vec![LookupCatalog::Discogs]);
    assert!(
        matches!(next, IdentifyState::ManualOnly { .. }),
        "nobody was asked anything, got {next:?}"
    );
    let no_catalog = crate::NotAskedReason::NoCatalog;
    assert!(matches!(
        ledger_of(&next).disc_id,
        crate::DiscIdStepView::Read { discs }
            if discs.iter().all(|disc| disc.lookup
                == crate::LookupView::NotAsked { reason: no_catalog })
    ));
}

// What the ledger shows of each lookup while the run is out and once it has
// settled.

/// A run in flight lays out each signal: the disc ID and the barcode asked,
/// the catalog number offered and asked of nobody, and no ISRC to ask.
#[test]
fn a_run_in_flight_lays_out_each_signal() {
    let (state, _) = update(
        started(),
        signals(
            disc("disc-hash"),
            BarcodeSignal::Settled { codes: artwork_codes(&["012345678905"]) },
            &["LBL-001"],
        ),
    );
    let ledger = ledger_of(&state);

    assert_eq!(disc_lookups(&state), [LookupView::LookingUp]);
    let barcodes = barcode_rows(&state);
    assert_eq!(left_out(&barcodes), [("012345678905", false)]);
    assert_eq!(cell(&barcodes[0], MB), &LookupView::LookingUp);
    let (numbers, offered) = catalog_rows(&state);
    assert!(numbers.is_empty(), "no number is in effect: {numbers:?}");
    assert_eq!(offered, ["LBL-001"]);
    assert_eq!(ledger.isrc, IsrcStepView::Absent);
}

/// Every catalog number extraction finds is offered.
#[test]
fn every_extracted_catalog_number_is_offered() {
    let (state, _) = update(
        started(),
        signals_with_catalogs(
            disc("disc-hash"),
            BarcodeSignal::Absent,
            vec!["LBL 001".to_string(), "LBL 999".to_string()],
        ),
    );
    assert_eq!(catalog_rows(&state).1, ["LBL 001", "LBL 999"]);
}

/// A failed disc-ID lookup reads as failed while the barcode is still out.
#[test]
fn a_failed_disc_id_lookup_reads_failed_while_the_run_is_out() {
    let (state, _) = update(started(), disc_and_codes("disc-hash", &["012345678905"]));
    let failure = LookupFailure::Provider { status: Some(503) };
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-hash".into(),
            outcome: Err(failure.clone()),
        },
    );
    assert!(matches!(state, IdentifyState::Triangulating { .. }), "got {state:?}");
    assert_eq!(disc_lookups(&state), [LookupView::Failed { failure }]);
}

/// A failed barcode lookup reads as failed.
#[test]
fn a_failed_barcode_lookup_reads_failed() {
    let (state, _) = update(started(), disc_and_codes("disc-hash", &["012345678905"]));
    let failure = LookupFailure::Provider { status: Some(500) };
    let (state, _) = step(state, barcode_failed(MB, "012345678905", failure.clone()));
    assert_eq!(cell(&barcode_rows(&state)[0], MB), &LookupView::Failed { failure });
}

/// A failed barcode lookup still reads as failed once the run settles.
#[test]
fn a_failed_barcode_lookup_reads_failed_once_settled() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled { codes: artwork_codes(&["012345678905"]) },
            &[],
        ),
    );
    let failure = LookupFailure::Provider { status: Some(500) };
    let (state, _) = step(state, barcode_failed(MB, "012345678905", failure.clone()));
    assert!(matches!(state, IdentifyState::Failed { .. }), "got {state:?}");
    assert_eq!(cell(&barcode_rows(&state)[0], MB), &LookupView::Failed { failure });
}

/// A failed disc-ID lookup still reads as failed once settled, not as a
/// no-match.
#[test]
fn a_failed_disc_id_lookup_reads_failed_once_settled() {
    let (state, _) = update(started(), signals(disc("disc-hash"), BarcodeSignal::Absent, &[]));
    let failure = LookupFailure::Provider { status: Some(503) };
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-hash".into(),
            outcome: Err(failure.clone()),
        },
    );
    assert!(matches!(state, IdentifyState::Failed { .. }), "got {state:?}");
    assert_eq!(disc_lookups(&state), [LookupView::Failed { failure }]);
}

/// A folder with neither a disc ID nor a barcode source asks nothing of
/// either.
#[test]
fn a_folder_with_no_disc_id_or_barcode_asks_neither() {
    let (state, _) = update(started(), signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &[]));
    assert!(matches!(state, IdentifyState::ManualOnly { .. }), "got {state:?}");
    let context = state.context().expect("a settled run keeps its context");
    assert_eq!(context.disc.signal, DiscIdSignal::Absent);
    assert!(!context.barcode.had_source);
}

/// A barcode failure while the disc ID is still out waits for it, then settles
/// as failed, and the ledger still shows it.
#[test]
fn barcode_failure_before_disc_settles_is_retained_through_combine() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));

    let failure = LookupFailure::Provider { status: Some(500) };
    let source_failure = SourceFailure { source: MB.catalog(), failure: failure.clone() };
    let (state, _) = step(state, barcode_failed(MB, "BAR", failure.clone()));
    // Disc-ID hasn't settled yet — the barcode failure alone can't terminate.
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "barcode failure while disc still looking up stays Triangulating, got {state:?}"
    );

    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![pair(
                "e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e",
                Some("g-x"),
            )])),
        },
    );

    assert!(matches!(
        &state,
        IdentifyState::Failed { failures, .. }
            if failures == &vec![IdentifyFailure::Barcode(source_failure.clone())]
    ));
    assert_eq!(cell(&barcode_rows(&state)[0], MB), &LookupView::Failed { failure });
}

/// The barcode lookup settles only once every provider has answered.
#[test]
fn the_barcode_lookup_waits_for_every_provider() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled { codes: artwork_codes(&["012345678905"]) },
            &[],
        ),
    );
    let (state, _) =
        step(state, barcode_matched(DG, "012345678905", vec![discogs_pair("dg-1", Some("g-x"))]));
    assert!(matches!(state, IdentifyState::Triangulating { .. }), "got {state:?}");
    let row = &barcode_rows(&state)[0];
    assert_eq!(cell(row, MB), &LookupView::LookingUp);
    assert!(matches!(cell(row, DG), LookupView::Found { count: 1, .. }));

    let (state, _) = step(state, barcode_missed(MB, "012345678905"));
    assert!(matches!(state, IdentifyState::Found { .. }), "got {state:?}");
    assert_eq!(cell(&barcode_rows(&state)[0], MB), &LookupView::NoMatch);
}
