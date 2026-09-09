use super::*;
use crate::db::LibraryStatus;
use crate::identify::state::{
    step, BarcodeEvidence, BarcodeProgress, DiscIdEvidence, DiscidProgress, IdentifyEvent,
};
use crate::identify::view::{BarcodeStepView, IdentifyStateView, LookupView};
use crate::identify::{CatalogProgress, ProviderBarcodeLookup};
use crate::import::MetadataSource;

fn mk_result(release_id: &str) -> MetadataResult {
    MetadataResult::for_test(MetadataSource::MusicBrainz, release_id, Some("group-1"))
}

/// A bare context, standing in for whatever the reducer would have
/// accumulated by this point — its contents don't matter to these tests,
/// only that `Idle`/`Triangulating` carry one and still aren't terminal.
fn mk_context(track_count: u32) -> SignalsContext {
    SignalsContext {
        providers: Vec::new(),
        artwork: crate::signals::ArtworkScan::Absent,
        disc: DiscIdEvidence {
            signal: crate::signals::DiscIdSignal::Absent { track_count },
            ..Default::default()
        },
        barcode: BarcodeEvidence::default(),
        catalog: Default::default(),
        text: Default::default(),
        track_count,
    }
}

/// `Idle` and `Triangulating` are not verdicts — the conversion must reject
/// them (not silently invent an empty verdict), and hand the state back.
#[test]
fn in_flight_states_have_no_terminal_verdict() {
    assert!(TerminalVerdict::try_from(IdentifyState::Idle).is_err());
    assert!(TerminalVerdict::try_from(IdentifyState::Triangulating {
        discid: DiscidProgress::Computing,
        barcode: BarcodeProgress::Scanning,
        catalog: crate::identify::CatalogProgress::Skipped,
        context: mk_context(0),
    })
    .is_err());
}

fn found_state() -> IdentifyState {
    IdentifyState::Found {
        matches: vec![mk_result("rel-1")],
        library_statuses: vec![LibraryStatus::absent("rel-1")],
        track_count: 11,
        provenance: vec![LookupProvenance {
            by_disc_id: true,
            by_barcode: false,
            by_catalog: false,
        }],
        narrowed_out: NarrowedOut::default(),
        ledger: None,
        context: mk_context(11),
    }
}

/// `Found` keeps its matches and provenance, and drops
/// `library_statuses` — that's re-checked live, not stored. `mk_context`
/// carries no recorded failure, so this also stands as the positive case:
/// a `Found` reached with both lookups completing converts normally.
#[test]
fn found_drops_library_status_and_keeps_the_rest() {
    let verdict = TerminalVerdict::try_from(found_state()).unwrap();
    assert_eq!(
        verdict,
        TerminalVerdict::Found {
            matches: vec![mk_result("rel-1")],
            track_count: 11,
            provenance: vec![LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
            }],
            narrowed_out: Vec::new(),
            narrowed_out_provenance: Vec::new(),
            ledger: None,
        }
    );
}

/// The ledger the run showed is what the verdict carries away: the terminal
/// state records it once, and conversion moves it rather than deriving
/// anything of its own.
#[test]
fn a_terminal_verdict_carries_the_ledger_its_run_recorded() {
    let mut context = mk_context(9);
    context.barcode.had_source = true;
    context.barcode.codes = vec![crate::signals::SourcedValue::new(
        "012345".to_string(),
        crate::signals::SignalOrigin::Artwork,
    )];
    context.providers = vec![MetadataSource::MusicBrainz];
    let (settled, _) = step(
        IdentifyState::Triangulating {
            discid: DiscidProgress::Skipped { track_count: 9 },
            barcode: BarcodeProgress::Lookups {
                codes: vec!["012345".to_string()],
                providers: vec![ProviderBarcodeLookup {
                    source: MetadataSource::MusicBrainz,
                    state: crate::identify::BarcodeLookupState::Trying { index: 0 },
                }],
            },
            catalog: CatalogProgress::Skipped,
            context,
        },
        IdentifyEvent::BarcodeLookupAnswered {
            source: MetadataSource::MusicBrainz,
            for_barcode: "012345".to_string(),
            outcome: Ok(vec![(mk_result("rel-1"), LibraryStatus::absent("rel-1"))]),
        },
    );
    let IdentifyStateView::Found {
        run: Some(shown), ..
    } = IdentifyStateView::from(settled.clone())
    else {
        panic!("the run found one release");
    };
    let verdict = TerminalVerdict::try_from(settled).unwrap();
    let TerminalVerdict::Found { ledger, .. } = &verdict else {
        panic!("a found state stores as a found verdict");
    };
    assert_eq!(ledger.as_ref(), Some(&shown));
    let BarcodeStepView::Rows { rows, .. } = &shown.barcode else {
        panic!("one row per code, got {:?}", shown.barcode);
    };
    assert!(matches!(
        rows[0].cells[0].lookup,
        LookupView::Found { count: 1, .. }
    ));
}

fn disc_id_only() -> LookupProvenance {
    LookupProvenance {
        by_disc_id: true,
        by_barcode: false,
        by_catalog: false,
    }
}

/// The releases agreement narrowed out are part of what the run learned,
/// so they store beside the matches instead of being dropped at the
/// boundary — and the live library check covers them, because a surface
/// offers them beside the matches.
#[test]
fn a_stored_verdict_keeps_what_agreement_narrowed_out() {
    let mut state = found_state();
    let IdentifyState::Found { narrowed_out, .. } = &mut state else {
        panic!("a found state");
    };
    *narrowed_out = NarrowedOut {
        matches: vec![mk_result("rel-out")],
        library_statuses: vec![LibraryStatus::absent("rel-out")],
        provenance: vec![disc_id_only()],
    };
    let verdict = TerminalVerdict::try_from(state).unwrap();
    let TerminalVerdict::Found {
        narrowed_out,
        narrowed_out_provenance,
        ..
    } = &verdict
    else {
        panic!("a found state stores as a found verdict");
    };
    assert_eq!(narrowed_out, &vec![mk_result("rel-out")]);
    assert_eq!(narrowed_out_provenance, &vec![disc_id_only()]);
    assert_eq!(
        verdict
            .named_releases()
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-1", "rel-out"]
    );
}

/// A resumed verdict stands them back up with a live library status each,
/// so the disclosure says the same thing after a relaunch as it did when
/// the run settled.
#[test]
fn a_resumed_verdict_stands_its_narrowed_out_releases_back_up() {
    let verdict = TerminalVerdict::Found {
        matches: vec![mk_result("rel-1")],
        track_count: 11,
        provenance: vec![disc_id_only()],
        narrowed_out: vec![mk_result("rel-out")],
        narrowed_out_provenance: vec![disc_id_only()],
        ledger: None,
    };
    let IdentifyState::Found { narrowed_out, .. } = verdict.resume_state(
        &|result| LibraryStatus::absent(&result.release_id),
        Default::default(),
    ) else {
        panic!("a found verdict resumes as Found");
    };
    assert_eq!(narrowed_out.matches, vec![mk_result("rel-out")]);
    assert_eq!(
        narrowed_out
            .library_statuses
            .iter()
            .map(|status| status.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-out"]
    );
    assert_eq!(narrowed_out.provenance, vec![disc_id_only()]);
}

/// The reducer exposes a failed lookup directly, and verdict conversion
/// preserves that terminal state.
#[test]
fn a_recorded_discid_failure_derives_and_stores_as_failed() {
    let mut context = mk_context(11);
    context.disc.failure = Some(crate::signals::LookupFailure::Provider { status: None });
    let state = crate::identify::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 11,
            ..
        } if failures == vec![IdentifyFailure::DiscId(crate::signals::LookupFailure::Provider {
                status: None
            })]
    ));
}

/// Signals that share no result settle as one `Found` over their union, so
/// what stores is a single match list — not two sections. Both
/// Neither signal recorded a failure here, so this also stands as the
/// positive case for a union-shaped `Found`.
#[test]
fn a_union_of_disagreeing_signals_stores_as_one_match_list() {
    let context = SignalsContext {
        disc: DiscIdEvidence {
            signal: crate::signals::DiscIdSignal::Absent { track_count: 9 },
            results: vec![(mk_result("rel-a"), LibraryStatus::absent("rel-a"))],
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            had_source: true,
            results: vec![(mk_result("rel-b"), LibraryStatus::absent("rel-b"))],
            matched: Some("012345".to_string()),
            ..Default::default()
        },
        ..mk_context(9)
    };
    let state = crate::identify::state::re_derive_for_tests(context);
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert_eq!(
        verdict,
        TerminalVerdict::Found {
            matches: vec![mk_result("rel-a"), mk_result("rel-b")],
            track_count: 9,
            provenance: vec![
                LookupProvenance {
                    by_disc_id: true,
                    by_barcode: false,
                    by_catalog: false,
                },
                LookupProvenance {
                    by_disc_id: false,
                    by_barcode: true,
                    by_catalog: false,
                },
            ],
            narrowed_out: Vec::new(),
            narrowed_out_provenance: Vec::new(),
            ledger: None,
        }
    );
}

/// A union reached where the disc-ID lookup failed rather than genuinely
/// disagreeing: had it succeeded with a release the barcode side also
/// returned, the intersection would have narrowed to one release. A
/// missing intersection partner is exactly what can manufacture a longer
/// match list, so this stores as a failure rather than that partial list.
#[test]
fn a_union_reached_with_a_recorded_discid_failure_is_failed() {
    let context = SignalsContext {
        disc: DiscIdEvidence {
            signal: crate::signals::DiscIdSignal::Absent { track_count: 9 },
            failure: Some(crate::signals::LookupFailure::Network),
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            had_source: true,
            results: vec![
                (mk_result("rel-1"), LibraryStatus::absent("rel-1")),
                (mk_result("rel-2"), LibraryStatus::absent("rel-2")),
            ],
            ..Default::default()
        },
        ..mk_context(9)
    };
    let state = crate::identify::state::re_derive_for_tests(context);
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 9,
            ..
        } if failures == vec![IdentifyFailure::DiscId(crate::signals::LookupFailure::Network)]
    ));
}

/// A genuinely empty search — both signals ran, neither failed, neither
/// found anything — is a real answer and must convert.
#[test]
fn clean_not_found_anywhere_is_terminal() {
    let context = mk_context(7);
    let verdict = TerminalVerdict::try_from(IdentifyState::NotFoundAnywhere {
        ledger: None,
        context,
    });
    assert!(matches!(
        verdict,
        Ok(TerminalVerdict::NotFoundAnywhere { ledger: None })
    ));
}

/// A disc-ID lookup failure derives to `Failed`, never to no-match.
#[test]
fn discid_failure_derives_to_failed() {
    let mut context = mk_context(7);
    context.disc.failure = Some(crate::signals::LookupFailure::Network);
    let state = crate::identify::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 7,
            ..
        } if failures == vec![IdentifyFailure::DiscId(crate::signals::LookupFailure::Network)]
    ));
}

/// Same for the barcode side, naming the provider that failed.
#[test]
fn barcode_failure_derives_to_failed() {
    let mut context = mk_context(7);
    context.barcode.failures = vec![SourceFailure {
        source: MetadataSource::Discogs,
        failure: crate::signals::LookupFailure::Timeout,
    }];
    let state = crate::identify::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 7,
            ..
        } if failures == vec![IdentifyFailure::Barcode(SourceFailure {
            source: MetadataSource::Discogs,
            failure: crate::signals::LookupFailure::Timeout,
        })]
    ));
}

/// One provider failing on the barcode while the other answered is still a
/// failed verdict — but the live state keeps the answering provider's
/// match, so the pane shows it instead of blanking.
#[test]
fn a_partial_barcode_answer_keeps_its_matches_on_a_failed_state() {
    let mut context = mk_context(7);
    context.barcode.had_source = true;
    context.barcode.results = vec![(mk_result("rel-mb"), LibraryStatus::absent("rel-mb"))];
    context.barcode.failures = vec![SourceFailure {
        source: MetadataSource::Discogs,
        failure: crate::signals::LookupFailure::Network,
    }];
    let state = crate::identify::state::re_derive_for_tests(context);
    let IdentifyState::Failed {
        matches, failures, ..
    } = &state
    else {
        panic!("a provider failure is a failed state");
    };
    assert_eq!(matches.len(), 1, "the other provider's match still stands");
    assert_eq!(
        failures,
        &vec![IdentifyFailure::Barcode(SourceFailure {
            source: MetadataSource::Discogs,
            failure: crate::signals::LookupFailure::Network,
        })]
    );
    // What stores is the failure: the partial match is live evidence, and
    // re-running is what turns it into an answer.
    assert!(matches!(
        TerminalVerdict::try_from(state).unwrap(),
        TerminalVerdict::Failed { .. }
    ));
}

/// Both providers answering the barcode is an ordinary `Found`, with no
/// failure recorded.
#[test]
fn both_providers_answering_the_barcode_is_found() {
    let mut context = mk_context(7);
    context.barcode.had_source = true;
    context.barcode.results = vec![
        (mk_result("rel-mb"), LibraryStatus::absent("rel-mb")),
        (mk_result("rel-dg"), LibraryStatus::absent("rel-dg")),
    ];
    let state = crate::identify::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Found { .. }));
}

/// A provider failing with nothing from anyone leaves a failure and no
/// matches at all.
#[test]
fn a_barcode_failure_with_no_results_carries_no_matches() {
    let mut context = mk_context(7);
    context.barcode.had_source = true;
    context.barcode.failures = vec![SourceFailure {
        source: MetadataSource::Discogs,
        failure: crate::signals::LookupFailure::Network,
    }];
    let IdentifyState::Failed { matches, .. } =
        crate::identify::state::re_derive_for_tests(context)
    else {
        panic!("a provider failure is a failed state");
    };
    assert!(matches.is_empty());
}

#[test]
fn chosen_catalog_failure_derives_to_failed() {
    let mut context = mk_context(7);
    context.catalog.chosen = vec![crate::identify::state::ChosenCatalog {
        value: "CAT-7".to_string(),
        results: Vec::new(),
        failures: vec![SourceFailure {
            source: MetadataSource::MusicBrainz,
            failure: crate::signals::LookupFailure::Network,
        }],
    }];
    let state = crate::identify::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 7,
            ..
        } if failures == vec![IdentifyFailure::Catalog(SourceFailure {
            source: MetadataSource::MusicBrainz,
            failure: crate::signals::LookupFailure::Network,
        })]
    ));
}
