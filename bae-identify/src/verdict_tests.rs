use super::*;
use crate::state::{
    step, BarcodeEvidence, BarcodeProgress, DiscIdEvidence, DiscidProgress, IdentifyEvent,
    SearchProgress,
};
use crate::state::{LookupState, ProviderLookup, ValueLookup};
use crate::view::{BarcodeStepView, IdentifyStateView, LookupView};
use crate::CatalogProgress;
use bae_catalogs::catalog::Catalog;
use bae_catalogs::catalog::LookupCatalog;

fn mk_result(release_id: &str) -> MetadataResult {
    MetadataResult::for_test(LookupCatalog::MusicBrainz, release_id, Some("group-1"))
}

/// A bare context for `track_count` tracks.
fn mk_context(track_count: u32) -> SignalsContext {
    SignalsContext {
        origin: bae_catalogs::AudioOrigin::default(),
        providers: Vec::new(),
        artwork: crate::signals::ArtworkScan::Absent,
        disc: DiscIdEvidence { signal: crate::signals::DiscIdSignal::Absent, ..Default::default() },
        barcode: BarcodeEvidence::default(),
        catalog: Default::default(),
        search: Default::default(),
        text: Default::default(),
        text_settled: true,
        audio: crate::signals::AudioFacts {
            track_count,
            track_lengths_ms: vec![200_000; track_count as usize],
            ..Default::default()
        },
        documents: crate::documents::DocumentReading::Pending,
        pressings: Vec::new(),
        isrc: Default::default(),
        track_titles: Vec::new(),
    }
}

/// `Idle` and `Triangulating` are not verdicts, and conversion rejects them.
#[test]
fn in_flight_states_have_no_terminal_verdict() {
    assert!(TerminalVerdict::try_from(IdentifyState::Idle).is_err());
    assert!(TerminalVerdict::try_from(IdentifyState::Triangulating {
        discid: DiscidProgress::Computing,
        barcode: BarcodeProgress::Scanning,
        catalog: crate::CatalogProgress::Skipped,
        isrc: crate::IsrcProgress::Skipped,
        search: SearchProgress::Pending,
        context: mk_context(0),
    })
    .is_err());
}

fn one_disc_id_match() -> Findings {
    Findings {
        matches: vec![mk_result("rel-1")],
        pressings: vec![0],
        narrowed_out: NarrowedOut::default(),
        medium_conflict: None,
        named_notes: Vec::new(),
    }
}

fn found_state() -> IdentifyState {
    IdentifyState::Found {
        findings: one_disc_id_match(),
        track_count: 11,
        ledger: None,
        context: mk_context(11),
    }
}

/// `Found` keeps its matches and their pressing rows.
#[test]
fn found_keeps_its_matches_and_their_rows() {
    let verdict = TerminalVerdict::try_from(found_state()).unwrap();
    assert_eq!(
        verdict,
        TerminalVerdict::Found { findings: one_disc_id_match(), track_count: 11, ledger: None }
    );
}

/// The verdict carries away the ledger the run showed.
#[test]
fn a_terminal_verdict_carries_the_ledger_its_run_recorded() {
    let mut context = mk_context(9);
    context.barcode.had_source = true;
    context.barcode.codes = vec![crate::signals::SourcedValue::new("012345".to_string())];
    context.providers = vec![LookupCatalog::MusicBrainz];
    let (settled, _) = step(
        IdentifyState::Triangulating {
            discid: DiscidProgress::Skipped,
            barcode: BarcodeProgress::Lookups {
                codes: vec![ValueLookup {
                    value: "012345".to_string(),
                    providers: vec![ProviderLookup {
                        source: LookupCatalog::MusicBrainz,
                        state: LookupState::LookingUp,
                    }],
                }],
            },
            catalog: CatalogProgress::Skipped,
            isrc: crate::IsrcProgress::Skipped,
            search: SearchProgress::Pending,
            context,
        },
        IdentifyEvent::BarcodeLookupAnswered {
            source: LookupCatalog::MusicBrainz,
            for_barcode: "012345".to_string(),
            outcome: Ok(vec![mk_result("rel-1")]),
        },
    );
    // The offered record's document is read before the run settles; this
    // one cannot be had, which leaves the record as its lookup stated it.
    let (settled, _) = step(
        settled,
        IdentifyEvent::ReleasesRead {
            read: vec![crate::documents::ReleaseReading {
                release: bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "rel-1"),
                document: Err(bae_catalogs::LookupFailure::Network),
            }],
        },
    );
    let IdentifyStateView::Found { run: Some(shown), .. } =
        IdentifyStateView::from(settled.clone())
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
    assert!(matches!(rows[0].cells[0].lookup, LookupView::Found { count: 1, .. }));
}

/// What agreement narrowed out stores beside the matches, and the verdict
/// names it with them.
#[test]
fn a_stored_verdict_keeps_what_agreement_narrowed_out() {
    let mut state = found_state();
    let IdentifyState::Found { findings, .. } = &mut state else {
        panic!("a found state");
    };
    findings.narrowed_out = NarrowedOut { matches: vec![mk_result("rel-out")], pressings: vec![0] };
    let verdict = TerminalVerdict::try_from(state).unwrap();
    let TerminalVerdict::Found { findings, .. } = &verdict else {
        panic!("a found state stores as a found verdict");
    };
    assert_eq!(findings.narrowed_out.matches, vec![mk_result("rel-out")]);
    assert_eq!(
        verdict
            .named_releases()
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-1", "rel-out"]
    );
}

/// A resumed verdict stands its narrowed-out releases back up.
#[test]
fn a_resumed_verdict_stands_its_narrowed_out_releases_back_up() {
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            narrowed_out: NarrowedOut { matches: vec![mk_result("rel-out")], pressings: vec![0] },
            ..one_disc_id_match()
        },
        track_count: 11,
        ledger: None,
    };
    let IdentifyState::Found { findings, .. } = verdict.resume_state(
        None,
        &crate::LookupChoices::default(),
        crate::signals::AudioFacts::default(),
    ) else {
        panic!("a found verdict resumes as Found");
    };
    let narrowed_out = findings.narrowed_out;
    assert_eq!(narrowed_out.matches, vec![mk_result("rel-out")]);
}

/// The origin a resumed state reads its rows against is the one the
/// candidate's stored signals say, not a default.
#[test]
fn a_resumed_verdict_takes_its_origin_from_the_stored_signals() {
    use crate::signals::{BarcodeSignal, DiscIdSignal, TextSignal};
    let signals = Signals {
        origin: bae_catalogs::AudioOrigin { source: None, not_cd_rate: Some(96_000) },
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Settled { catalogs: Vec::new(), free_text: Vec::new() },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    };
    let verdict =
        TerminalVerdict::Found { findings: one_disc_id_match(), track_count: 11, ledger: None };
    let IdentifyState::Found { context, .. } = verdict.resume_state(
        Some(&signals),
        &crate::LookupChoices::default(),
        crate::signals::AudioFacts::default(),
    ) else {
        panic!("a found verdict resumes as Found");
    };
    assert_eq!(context.origin.not_cd_rate, Some(96_000));
}

/// The reducer exposes a failed lookup directly, and verdict conversion
/// preserves that terminal state.
#[test]
fn a_recorded_discid_failure_derives_and_stores_as_failed() {
    let mut context = mk_context(11);
    context.disc.failures = vec![bae_catalogs::LookupFailure::Provider { status: None }];
    let state = crate::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 11,
            ..
        } if failures == vec![IdentifyFailure::DiscId(bae_catalogs::LookupFailure::Provider {
                status: None
            })]
    ));
}

/// Signals that share no result store as one match list: the disc ID's
/// release offered, the barcode's narrowed out beside it.
#[test]
fn signals_that_share_no_result_store_as_one_match_list() {
    let context = SignalsContext {
        disc: DiscIdEvidence {
            signal: crate::signals::DiscIdSignal::Absent,
            answers: vec![vec![mk_result("rel-a")]],
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            had_source: true,
            results: vec![mk_result("rel-b")],
            ..Default::default()
        },
        ..mk_context(9)
    };
    let state = crate::state::re_derive_for_tests(context);
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert_eq!(
        verdict,
        TerminalVerdict::Found {
            findings: Findings {
                matches: vec![mk_result("rel-a")],
                pressings: vec![0],
                narrowed_out: NarrowedOut { matches: vec![mk_result("rel-b")], pressings: vec![0] },
                medium_conflict: None,
                named_notes: Vec::new(),
            },
            track_count: 9,
            ledger: None,
        }
    );
}

/// A union reached because the disc-ID lookup failed stores as a failure,
/// not as the longer list the missing lookup left.
#[test]
fn a_union_reached_with_a_recorded_discid_failure_is_failed() {
    let context = SignalsContext {
        disc: DiscIdEvidence {
            signal: crate::signals::DiscIdSignal::Absent,
            failures: vec![bae_catalogs::LookupFailure::Network],
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            had_source: true,
            results: vec![mk_result("rel-1"), mk_result("rel-2")],
            ..Default::default()
        },
        ..mk_context(9)
    };
    let state = crate::state::re_derive_for_tests(context);
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 9,
            ..
        } if failures == vec![IdentifyFailure::DiscId(bae_catalogs::LookupFailure::Network)]
    ));
}

/// Lookups that ran, failed nothing and found nothing are a real answer.
#[test]
fn clean_not_found_anywhere_is_terminal() {
    let context = mk_context(7);
    let verdict =
        TerminalVerdict::try_from(IdentifyState::NotFoundAnywhere { ledger: None, context });
    assert!(matches!(verdict, Ok(TerminalVerdict::NotFoundAnywhere { ledger: None })));
}

/// A disc-ID lookup failure derives to `Failed`, never to no-match.
#[test]
fn discid_failure_derives_to_failed() {
    let mut context = mk_context(7);
    context.disc.failures = vec![bae_catalogs::LookupFailure::Network];
    let state = crate::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 7,
            ..
        } if failures == vec![IdentifyFailure::DiscId(bae_catalogs::LookupFailure::Network)]
    ));
}

/// Same for the barcode side, naming the provider that failed.
#[test]
fn barcode_failure_derives_to_failed() {
    let mut context = mk_context(7);
    context.barcode.failures = vec![SourceFailure {
        source: Catalog::Discogs,
        failure: bae_catalogs::LookupFailure::Timeout,
    }];
    let state = crate::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 7,
            ..
        } if failures == vec![IdentifyFailure::Barcode(SourceFailure {
            source: Catalog::Discogs,
            failure: bae_catalogs::LookupFailure::Timeout,
        })]
    ));
}

/// One provider failing on the barcode is a failed verdict that keeps the
/// other's match, live and stored.
#[test]
fn a_partial_barcode_answer_keeps_its_matches_on_a_failed_state() {
    let mut context = mk_context(7);
    context.barcode.had_source = true;
    context.barcode.results = vec![mk_result("rel-mb")];
    context.barcode.failures = vec![SourceFailure {
        source: Catalog::Discogs,
        failure: bae_catalogs::LookupFailure::Network,
    }];
    let state = crate::state::re_derive_for_tests(context);
    let IdentifyState::Failed { findings, failures, .. } = &state else {
        panic!("a provider failure is a failed state");
    };
    assert_eq!(findings.matches.len(), 1, "the other provider's match still stands");
    let live = findings.clone();
    assert_eq!(
        failures,
        &vec![IdentifyFailure::Barcode(SourceFailure {
            source: Catalog::Discogs,
            failure: bae_catalogs::LookupFailure::Network,
        })]
    );
    let TerminalVerdict::Failed { findings, .. } = TerminalVerdict::try_from(state).unwrap() else {
        panic!("a failed state stores as a failed verdict");
    };
    assert_eq!(findings, live, "the stored failure keeps the match");
}

/// Both providers answering the barcode is an ordinary `Found`, with no
/// failure recorded.
#[test]
fn both_providers_answering_the_barcode_is_found() {
    let mut context = mk_context(7);
    context.barcode.had_source = true;
    context.barcode.results = vec![mk_result("rel-mb"), mk_result("rel-dg")];
    let state = crate::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Found { .. }));
}

/// A provider failing with nothing from anyone leaves a failure and no
/// matches at all.
#[test]
fn a_barcode_failure_with_no_results_carries_no_matches() {
    let mut context = mk_context(7);
    context.barcode.had_source = true;
    context.barcode.failures = vec![SourceFailure {
        source: Catalog::Discogs,
        failure: bae_catalogs::LookupFailure::Network,
    }];
    let IdentifyState::Failed { findings, .. } = crate::state::re_derive_for_tests(context) else {
        panic!("a provider failure is a failed state");
    };
    assert!(findings.is_empty());
}

#[test]
fn chosen_catalog_failure_derives_to_failed() {
    let mut context = mk_context(7);
    context.catalog.chosen = vec!["CAT-7".to_string()];
    context.catalog.searched = vec![crate::state::SearchedCatalog {
        value: "CAT-7".to_string(),
        results: Vec::new(),
        failures: vec![SourceFailure {
            source: Catalog::MusicBrainz,
            failure: bae_catalogs::LookupFailure::Network,
        }],
    }];
    let state = crate::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let verdict = TerminalVerdict::try_from(state).unwrap();
    assert!(matches!(
        verdict,
        TerminalVerdict::Failed {
            failures,
            track_count: 7,
            ..
        } if failures == vec![IdentifyFailure::Catalog(SourceFailure {
            source: Catalog::MusicBrainz,
            failure: bae_catalogs::LookupFailure::Network,
        })]
    ));
}

/// A later failure, such as fetching release details, keeps the findings,
/// and a second one joins the first.
#[test]
fn a_later_failure_keeps_the_findings_and_joins_earlier_failures() {
    let details = |detail: &str| {
        IdentifyFailure::ReleaseDetails(bae_catalogs::LookupFailure::Provider {
            status: Some(match detail {
                "first" => 500,
                _ => 503,
            }),
        })
    };
    let mut verdict = TerminalVerdict::try_from(found_state()).unwrap();
    verdict.fail(details("first"));
    assert_eq!(
        verdict,
        TerminalVerdict::Failed {
            failures: vec![details("first")],
            findings: one_disc_id_match(),
            track_count: 11,
            ledger: None,
        }
    );
    verdict.fail(details("second"));
    let TerminalVerdict::Failed { failures, .. } = &verdict else {
        panic!("a failure stays a failure");
    };
    assert_eq!(failures, &vec![details("first"), details("second")]);
}

/// A stored failure stands back up with its findings, each with a live
/// library status.
#[test]
fn a_resumed_failure_stands_its_findings_back_up() {
    let verdict = TerminalVerdict::Failed {
        failures: vec![IdentifyFailure::DiscId(bae_catalogs::LookupFailure::Network)],
        findings: one_disc_id_match(),
        track_count: 11,
        ledger: None,
    };
    assert_eq!(
        verdict
            .named_releases()
            .iter()
            .map(|result| result.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["rel-1"]
    );
    let IdentifyState::Failed { findings, .. } = verdict.resume_state(
        None,
        &crate::LookupChoices::default(),
        crate::signals::AudioFacts::default(),
    ) else {
        panic!("a failed verdict resumes as Failed");
    };
    assert_eq!(findings, one_disc_id_match());
}
