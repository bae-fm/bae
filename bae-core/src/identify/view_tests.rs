use super::*;
use crate::db::LibraryStatus;
use crate::identify::state::{
    BarcodeEvidence, BarcodeLookupState, CatalogEvidence, ChosenCatalog, DiscIdEvidence,
    ProviderBarcodeLookup, ProviderLookup, RecordedWalk, WalkEnd,
};
use crate::identify::{IdentifyFailure, TerminalVerdict};
use crate::import::search::{MetadataResult, SourceFailure};
use crate::import::{LookupChoices, MetadataSource};
use crate::signals::{BarcodeSignal, SignalOrigin, Signals, SourcedValue, TextSignal};

const MB: MetadataSource = MetadataSource::MusicBrainz;
const DG: MetadataSource = MetadataSource::Discogs;

fn result(source: MetadataSource, release_id: &str) -> (MetadataResult, LibraryStatus) {
    (
        MetadataResult::for_test(source, release_id, Some("g")),
        LibraryStatus::absent(release_id),
    )
}

fn context() -> SignalsContext {
    SignalsContext {
        providers: vec![MB, DG],
        artwork: ArtworkScan::Absent,
        disc: DiscIdEvidence {
            signal: DiscIdSignal::Absent { track_count: 9 },
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            codes: vec![SourcedValue::new("A".to_string(), SignalOrigin::Artwork)],
            had_source: true,
            ..Default::default()
        },
        catalog: CatalogEvidence::default(),
        track_count: 9,
    }
}

fn in_flight(context: SignalsContext) -> IdentifyState {
    IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped { track_count: 9 },
        barcode: BarcodeProgress::Lookups {
            codes: vec!["A".to_string()],
            providers: vec![
                ProviderBarcodeLookup {
                    source: MB,
                    state: BarcodeLookupState::Trying { index: 0 },
                },
                ProviderBarcodeLookup {
                    source: DG,
                    state: BarcodeLookupState::Matched {
                        code: "A".to_string(),
                        results: vec![result(DG, "dg-1")],
                    },
                },
            ],
        },
        catalog: CatalogProgress::Skipped,
        context,
    }
}

fn run_of(state: IdentifyState) -> IdentifyRunView {
    match IdentifyStateView::from(state) {
        IdentifyStateView::Triangulating { run, .. } => run,
        IdentifyStateView::Found { run: Some(run), .. }
        | IdentifyStateView::NotFoundAnywhere { run: Some(run) }
        | IdentifyStateView::ManualOnly { run: Some(run), .. }
        | IdentifyStateView::Failed { run: Some(run), .. } => run,
        other => panic!("a state with a run to lay out, got {other:?}"),
    }
}

fn barcode_rows(run: &IdentifyRunView) -> &[SignalValueRow] {
    match &run.barcode {
        BarcodeStepView::Rows { rows, .. } => rows,
        other => panic!("barcode rows, got {other:?}"),
    }
}

fn cells(row: &SignalValueRow) -> Vec<&LookupView> {
    row.cells.iter().map(|cell| &cell.lookup).collect()
}

/// What one provider found shows while the other is still looking, with
/// the provenance the settled verdict will give it.
#[test]
fn a_landed_provider_s_matches_show_before_the_other_answers() {
    let IdentifyStateView::Triangulating {
        groups, provenance, ..
    } = IdentifyStateView::from(in_flight(context()))
    else {
        panic!("a run in flight");
    };
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].pressings[0].releases[0].release_id, "dg-1");
    assert_eq!(provenance.len(), 1);
    assert_eq!(provenance[0].0, "dg-1");
    assert!(provenance[0].1.by_barcode);
}

/// A signal the user unchecked contributes nothing mid-run, as it will
/// contribute nothing at settle.
#[test]
fn an_excluded_signal_s_matches_do_not_show() {
    let mut context = context();
    context.barcode.excluded = true;
    let IdentifyStateView::Triangulating { groups, .. } =
        IdentifyStateView::from(in_flight(context))
    else {
        panic!("a run in flight");
    };
    assert!(groups.is_empty());
}

/// Each code is a row, and each provider's walk fills the row's cell from
/// where the walk is: the codes it passed missed, the one it is on is being
/// asked, the ones ahead wait; a walk that matched names its count on the
/// code that matched and never needed the rest.
#[test]
fn a_provider_s_walk_fills_one_cell_per_code() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("A".to_string(), SignalOrigin::Artwork),
        SourcedValue::new("B".to_string(), SignalOrigin::Artwork),
        SourcedValue::new("C".to_string(), SignalOrigin::Artwork),
    ];
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped { track_count: 9 },
        barcode: BarcodeProgress::Lookups {
            codes: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            providers: vec![
                ProviderBarcodeLookup {
                    source: MB,
                    state: BarcodeLookupState::Trying { index: 1 },
                },
                ProviderBarcodeLookup {
                    source: DG,
                    state: BarcodeLookupState::Matched {
                        code: "B".to_string(),
                        results: vec![result(DG, "dg-1"), result(DG, "dg-2")],
                    },
                },
            ],
        },
        catalog: CatalogProgress::Skipped,
        context,
    };
    let run = run_of(state);
    let rows = barcode_rows(&run);
    assert_eq!(
        rows.iter()
            .map(|row| row.value.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "B", "C"]
    );
    assert_eq!(
        cells(&rows[0]),
        vec![&LookupView::NoMatch, &LookupView::NoMatch]
    );
    assert!(matches!(
        cells(&rows[1]).as_slice(),
        [LookupView::LookingUp, LookupView::Found { count: 2, .. }]
    ));
    assert_eq!(
        cells(&rows[2]),
        vec![&LookupView::Queued, &LookupView::NotAsked]
    );
}

/// A walk that failed names its failure on the code it failed at.
#[test]
fn a_failed_walk_warns_on_the_code_it_failed_at() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("A".to_string(), SignalOrigin::Artwork),
        SourcedValue::new("B".to_string(), SignalOrigin::Artwork),
    ];
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped { track_count: 9 },
        barcode: BarcodeProgress::Lookups {
            codes: vec!["A".to_string(), "B".to_string()],
            providers: vec![ProviderBarcodeLookup {
                source: MB,
                state: BarcodeLookupState::Failed {
                    failure: LookupFailure::Timeout,
                    index: 1,
                },
            }],
        },
        catalog: CatalogProgress::Skipped,
        context,
    };
    let run = run_of(state);
    let rows = barcode_rows(&run);
    assert_eq!(cells(&rows[0]), vec![&LookupView::NoMatch]);
    assert_eq!(
        cells(&rows[1]),
        vec![&LookupView::Failed {
            failure: LookupFailure::Timeout
        }]
    );
}

/// The same code read off two files is one row with both places beside it.
#[test]
fn a_code_seen_in_two_places_is_one_row_with_two_sources() {
    let mut context = context();
    let region = ImageRegion::new(0.2, 0.7, 0.4, 0.1);
    context.barcode.codes = vec![
        SourcedValue::in_file(
            "A".to_string(),
            SignalOrigin::CueSheet,
            "disc.cue".to_string(),
        ),
        SourcedValue::in_file(
            "A".to_string(),
            SignalOrigin::Artwork,
            "back.jpg".to_string(),
        )
        .at(region),
    ];
    let run = run_of(in_flight(context));
    let rows = barcode_rows(&run);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].sources,
        vec![
            ValueSource {
                origin: SignalOrigin::CueSheet,
                file: Some("disc.cue".to_string()),
                region: None,
            },
            ValueSource {
                origin: SignalOrigin::Artwork,
                file: Some("back.jpg".to_string()),
                region,
            },
        ]
    );
}

/// While the artwork is still being read, every code read so far is a row
/// with waiting cells, and the step says more may come.
#[test]
fn codes_read_so_far_wait_while_the_artwork_is_still_being_read() {
    let mut context = context();
    context.artwork = ArtworkScan::Reading {
        current: Some("Back.jpg".to_string()),
        position: 2,
        total: 3,
    };
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped { track_count: 9 },
        barcode: BarcodeProgress::Scanning,
        catalog: CatalogProgress::Skipped,
        context,
    };
    let run = run_of(state);
    let BarcodeStepView::Rows { scanning, rows } = &run.barcode else {
        panic!("rows, got {:?}", run.barcode);
    };
    assert!(scanning);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        cells(&rows[0]),
        vec![&LookupView::Queued, &LookupView::Queued]
    );
    assert!(matches!(
        run.catalog,
        CatalogStepView::Numbers { scanning: true, .. }
    ));
}

/// The chosen numbers are rows with cells; the rest are tiles. A number seen
/// twice is one tile.
#[test]
fn chosen_catalog_numbers_are_rows_and_the_rest_are_tiles() {
    let mut context = context();
    context.catalog.numbers = vec![
        SourcedValue::new("LBL-1".to_string(), SignalOrigin::FolderName),
        SourcedValue::in_file(
            "LBL-2".to_string(),
            SignalOrigin::Artwork,
            "back.jpg".to_string(),
        ),
        SourcedValue::in_file(
            "LBL-2".to_string(),
            SignalOrigin::TextFile,
            "info.txt".to_string(),
        ),
        SourcedValue::new("LBL-3".to_string(), SignalOrigin::Filename),
    ];
    context.catalog.chosen = vec![ChosenCatalog {
        value: "LBL-2".to_string(),
        results: Vec::new(),
        failures: Vec::new(),
    }];
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped { track_count: 9 },
        barcode: BarcodeProgress::NoCodes,
        catalog: CatalogProgress::Lookups {
            values: vec![CatalogLookup {
                value: "LBL-2".to_string(),
                providers: vec![
                    ProviderLookup {
                        source: MB,
                        state: LookupState::Done {
                            results: vec![result(MB, "mb-1")],
                        },
                    },
                    ProviderLookup {
                        source: DG,
                        state: LookupState::LookingUp,
                    },
                ],
            }],
        },
        context,
    };
    let run = run_of(state);
    let CatalogStepView::Numbers {
        scanning,
        rows,
        candidates,
    } = &run.catalog
    else {
        panic!("numbers, got {:?}", run.catalog);
    };
    assert!(!scanning);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, "LBL-2");
    assert_eq!(rows[0].sources.len(), 2);
    assert!(matches!(
        cells(&rows[0]).as_slice(),
        [LookupView::Found { count: 1, .. }, LookupView::LookingUp]
    ));
    assert_eq!(
        candidates
            .iter()
            .map(|c| c.value.as_str())
            .collect::<Vec<_>>(),
        vec!["LBL-1", "LBL-3"]
    );
}

/// A settled state lays its run out as it settled — the walks each provider
/// recorded — so the ledger stays up beside the matches.
#[test]
fn a_settled_state_carries_the_run_it_settled_as() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("A".to_string(), SignalOrigin::Artwork),
        SourcedValue::new("B".to_string(), SignalOrigin::Artwork),
    ];
    context.barcode.results = vec![result(MB, "mb-1")];
    context.barcode.matched = Some("B".to_string());
    context.barcode.failures = vec![SourceFailure {
        source: DG,
        failure: LookupFailure::Network,
    }];
    context.barcode.walks = vec![
        RecordedWalk {
            source: MB,
            end: WalkEnd::Matched {
                code: "B".to_string(),
            },
        },
        RecordedWalk {
            source: DG,
            end: WalkEnd::Failed {
                code: "A".to_string(),
            },
        },
    ];
    let state = crate::identify::state::re_derive_for_tests(context);
    assert!(matches!(state, IdentifyState::Failed { .. }));
    let run = run_of(state);
    let rows = barcode_rows(&run);
    assert!(matches!(
        cells(&rows[0]).as_slice(),
        [
            LookupView::NoMatch,
            LookupView::Failed {
                failure: LookupFailure::Network
            }
        ]
    ));
    assert!(matches!(
        cells(&rows[1]).as_slice(),
        [LookupView::Found { count: 1, .. }, LookupView::NotAsked]
    ));
}

/// A verdict stood back up from the store retained no signal inputs, so it
/// has no run to lay out; so does a folder that carries nothing to look up.
#[test]
fn a_state_with_no_inputs_has_no_run() {
    let blank = SignalsContext {
        providers: Vec::new(),
        barcode: BarcodeEvidence::default(),
        ..context()
    };
    assert!(matches!(
        IdentifyStateView::from(IdentifyState::ManualOnly {
            track_count: 9,
            context: blank.clone(),
        }),
        IdentifyStateView::ManualOnly { run: None, .. }
    ));
    assert!(matches!(
        IdentifyStateView::from(IdentifyState::NotFoundAnywhere { context: blank }),
        IdentifyStateView::NotFoundAnywhere { run: None }
    ));
}

/// A folder with nothing to look up automatically but catalog numbers to
/// offer still has a run: the tiles, waiting to be activated.
#[test]
fn a_manual_only_folder_with_catalog_numbers_offers_them() {
    let mut context = SignalsContext {
        providers: vec![MB],
        barcode: BarcodeEvidence::default(),
        ..context()
    };
    context.catalog.numbers = vec![SourcedValue::new(
        "LBL-1".to_string(),
        SignalOrigin::FolderName,
    )];
    let IdentifyStateView::ManualOnly { run: Some(run), .. } =
        IdentifyStateView::from(IdentifyState::ManualOnly {
            track_count: 9,
            context,
        })
    else {
        panic!("a run with tiles");
    };
    assert_eq!(run.disc_id, DiscIdStepView::Absent);
    assert_eq!(run.barcode, BarcodeStepView::Absent);
    assert!(matches!(
        &run.catalog,
        CatalogStepView::Numbers { candidates, rows, .. } if candidates.len() == 1 && rows.is_empty()
    ));
}

/// A found lookup carries the album cards its count stands for.
#[test]
fn a_found_lookup_names_its_releases() {
    let mut context = context();
    context.disc.signal = DiscIdSignal::Computed {
        disc_id: "d".to_string(),
        track_count: 9,
        source_file: Some("rip/Album.LOG".to_string()),
    };
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Done {
            results: vec![result(MB, "mb-1"), result(MB, "mb-2")],
            track_count: 9,
        },
        barcode: BarcodeProgress::NoCodes,
        catalog: CatalogProgress::Skipped,
        context,
    };
    let run = run_of(state);
    let DiscIdStepView::Read {
        source,
        lookup: LookupView::Found { count, groups },
        ..
    } = run.disc_id
    else {
        panic!("a found disc ID, got {:?}", run.disc_id);
    };
    assert_eq!(count, 2);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].pressings.len(), 2);
    assert_eq!(
        source,
        Some(DiscIdFile {
            kind: DiscIdFileKind::Log,
            file: "rip/Album.LOG".to_string(),
        })
    );
}

// ── Resuming a stored verdict ───────────────────────────────────────────────

/// The signals a candidate stores beside its verdict: a disc ID read off a rip
/// log, one barcode read off the back cover, one catalog number off the sheet.
fn stored_signals() -> Signals {
    Signals {
        disc_id: DiscIdSignal::Computed {
            disc_id: "disc-1".to_string(),
            track_count: 9,
            source_file: Some("rip/Album.LOG".to_string()),
        },
        barcode: BarcodeSignal::Settled {
            codes: vec![SourcedValue::in_file(
                "0123456789012".to_string(),
                SignalOrigin::Artwork,
                "back.jpg".to_string(),
            )],
        },
        text: TextSignal::Settled {
            catalogs: vec![SourcedValue::new(
                "LBL-1".to_string(),
                SignalOrigin::CueSheet,
            )],
            free_text: Vec::new(),
        },
        durations: Default::default(),
    }
}

fn not_in_library(result: &MetadataResult) -> LibraryStatus {
    LibraryStatus::absent(&result.release_id)
}

/// A stored failure resumes with the ledger the run failed on: the provider
/// that could not answer warns on the code it was asked about, which is what
/// puts that cell's Retry back on screen.
///
/// Only that provider is laid out. The stored verdict names it and no other,
/// and no source is assumed to have been asked — a second column would claim
/// a source ran when nothing stored says it did.
#[test]
fn a_resumed_failure_lays_out_the_run_it_failed_on() {
    let verdict = TerminalVerdict::Failed {
        failures: vec![IdentifyFailure::Barcode(SourceFailure {
            source: DG,
            failure: LookupFailure::Provider { status: Some(503) },
        })],
        track_count: 9,
    };
    let run = run_of(verdict.resume_state(
        Some(&stored_signals()),
        &LookupChoices::default(),
        &not_in_library,
    ));
    assert_eq!(run.providers, vec![DG]);
    let rows = barcode_rows(&run);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, "0123456789012");
    assert_eq!(
        cells(&rows[0]),
        vec![&LookupView::Failed {
            failure: LookupFailure::Provider { status: Some(503) },
        }]
    );
}

/// A stored `Found` resumes with the ledger it settled on: the disc ID's count
/// on its own row, beside the file it was read off, and the barcode's count on
/// the code the verdict names, under the provider that answered about it.
#[test]
fn a_resumed_found_lays_out_the_run_it_settled_on() {
    let verdict = TerminalVerdict::Found {
        matches: vec![
            MetadataResult::for_test(MB, "mb-1", Some("g")),
            MetadataResult::for_test(DG, "dg-1", Some("g")),
        ],
        track_count: 9,
        provenance: vec![
            ResultProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
            },
            ResultProvenance {
                by_disc_id: false,
                by_barcode: true,
                by_catalog: false,
            },
        ],
        matched_barcode: Some("0123456789012".to_string()),
        narrowed_out: Vec::new(),
        narrowed_out_provenance: Vec::new(),
    };
    let run = run_of(verdict.resume_state(
        Some(&stored_signals()),
        &LookupChoices::default(),
        &not_in_library,
    ));
    assert_eq!(run.providers, vec![MB, DG]);
    let DiscIdStepView::Read { source, lookup, .. } = &run.disc_id else {
        panic!("a disc ID that was read, got {:?}", run.disc_id);
    };
    assert!(matches!(lookup, LookupView::Found { count: 1, .. }));
    assert_eq!(
        source,
        &Some(DiscIdFile {
            kind: DiscIdFileKind::Log,
            file: "rip/Album.LOG".to_string(),
        })
    );
    let rows = barcode_rows(&run);
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        cells(&rows[0]).as_slice(),
        [LookupView::NoMatch, LookupView::Found { count: 1, .. }]
    ));
}

/// Nothing found anywhere names no source, so a resumed run lays out no
/// columns: every source that was asked answered with nothing, and which
/// sources those were is not stored. The codes and the disc ID still stand —
/// they are the candidate's, not the run's — with no cell claiming a source
/// was asked about them.
#[test]
fn a_resumed_empty_run_lays_out_no_provider_it_cannot_name() {
    let run = run_of(TerminalVerdict::NotFoundAnywhere.resume_state(
        Some(&stored_signals()),
        &LookupChoices::default(),
        &|_| unreachable!("a no-match verdict names no release"),
    ));
    assert!(run.providers.is_empty());
    assert!(cells(&barcode_rows(&run)[0]).is_empty());
    assert!(matches!(
        run.disc_id,
        DiscIdStepView::Read {
            lookup: LookupView::NoMatch,
            ..
        }
    ));
}

/// A verdict whose candidate has no stored signals has no inputs to lay a
/// ledger out from, so it resumes without one — and the pane offers the
/// re-run in the failure lines instead.
#[test]
fn a_verdict_resumed_without_stored_signals_has_no_run() {
    let verdict = TerminalVerdict::Failed {
        failures: vec![IdentifyFailure::DiscId(LookupFailure::Network)],
        track_count: 9,
    };
    assert!(matches!(
        IdentifyStateView::from(verdict.resume_state(
            None,
            &LookupChoices::default(),
            &not_in_library
        )),
        IdentifyStateView::Failed { run: None, .. }
    ));
}

// ── What agreement narrowed out ─────────────────────────────────────────────

/// The barcode walks a settled context needs to stand its ledger back up: one
/// provider matched the code, the other tried it and found nothing.
fn settled_walks() -> Vec<RecordedWalk> {
    vec![
        RecordedWalk {
            source: MB,
            end: WalkEnd::Matched {
                code: "A".to_string(),
            },
        },
        RecordedWalk {
            source: DG,
            end: WalkEnd::Exhausted,
        },
    ]
}

/// Agreement is what shortens the list, so what it discarded stays on the
/// state: its own cards, its own statuses, and the provenance saying which
/// signal named each one.
#[test]
fn a_settled_state_lists_what_agreement_narrowed_out() {
    let mut context = context();
    context.disc.signal = DiscIdSignal::Computed {
        disc_id: "d".to_string(),
        track_count: 9,
        source_file: None,
    };
    context.disc.results = vec![result(MB, "mb-shared"), result(MB, "mb-only")];
    context.barcode.results = vec![result(MB, "mb-shared")];
    context.barcode.matched = Some("A".to_string());
    context.barcode.walks = settled_walks();

    let IdentifyStateView::Found {
        groups,
        narrowed_out,
        ..
    } = IdentifyStateView::from(crate::identify::state::re_derive_for_tests(context))
    else {
        panic!("the signals agree on one release");
    };
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].pressings[0].releases[0].release_id, "mb-shared");
    assert_eq!(narrowed_out.groups.len(), 1);
    assert_eq!(
        narrowed_out.groups[0].pressings[0].releases[0].release_id,
        "mb-only"
    );
    assert_eq!(
        narrowed_out
            .library_statuses
            .iter()
            .map(|status| status.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-only"]
    );
    assert_eq!(narrowed_out.provenance[0].0, "mb-only");
    assert!(narrowed_out.provenance[0].1.by_disc_id);
    assert!(!narrowed_out.provenance[0].1.by_barcode);
}

/// Signals that share nothing already list everything they saw, so there is
/// nothing behind the disclosure.
#[test]
fn signals_that_agree_on_nothing_narrow_nothing_out() {
    let mut context = context();
    context.disc.signal = DiscIdSignal::Computed {
        disc_id: "d".to_string(),
        track_count: 9,
        source_file: None,
    };
    context.disc.results = vec![result(MB, "mb-disc")];
    context.barcode.results = vec![result(MB, "mb-barcode")];
    context.barcode.matched = Some("A".to_string());
    context.barcode.walks = settled_walks();

    let IdentifyStateView::Found {
        groups,
        narrowed_out,
        ..
    } = IdentifyStateView::from(crate::identify::state::re_derive_for_tests(context))
    else {
        panic!("both releases are offered");
    };
    assert_eq!(groups[0].pressings.len(), 2);
    assert!(narrowed_out.is_empty());
}
