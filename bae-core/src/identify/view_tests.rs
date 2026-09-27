use super::*;
use crate::db::LibraryStatus;
use crate::identify::state::{
    step, BarcodeEvidence, ChosenCatalog, DiscIdEvidence, IdentifyEvent, LookupState,
    ProviderLookup, SearchProgress, ValueLookup,
};
use crate::identify::{Findings, IdentifyFailure, LookupProvenance, NarrowedOut, TerminalVerdict};
use crate::import::release_group::unranked;
use crate::import::search::MetadataResult;
use crate::import::Catalog;
use crate::signals::SourcedValue;

pub(super) const MB: Catalog = Catalog::MusicBrainz;
const DG: Catalog = Catalog::Discogs;

fn result(source: Catalog, release_id: &str) -> (MetadataResult, LibraryStatus) {
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
            signal: DiscIdSignal::Absent,
            ..Default::default()
        },
        barcode: BarcodeEvidence {
            codes: vec![SourcedValue::new("A".to_string())],
            had_source: true,
            ..Default::default()
        },
        text_settled: true,
        audio: crate::signals::AudioFacts {
            track_count: 9,
            ..Default::default()
        },
        ..SignalsContext::default()
    }
}

/// One code's lookup, with each provider's state as given.
fn code(value: &str, providers: Vec<(Catalog, LookupState)>) -> ValueLookup {
    ValueLookup {
        value: value.to_string(),
        providers: providers
            .into_iter()
            .map(|(source, state)| ProviderLookup { source, state })
            .collect(),
    }
}

fn found(results: Vec<(MetadataResult, LibraryStatus)>) -> LookupState {
    LookupState::Done { results }
}

fn in_flight(context: SignalsContext) -> IdentifyState {
    IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode: BarcodeProgress::Lookups {
            codes: vec![code(
                "A",
                vec![
                    (MB, LookupState::LookingUp),
                    (DG, found(vec![result(DG, "dg-1")])),
                ],
            )],
        },
        catalog: CatalogProgress::Skipped,
        search: SearchProgress::Pending,
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

/// One provider's matches show, badged, while the other is still looking.
#[test]
fn a_landed_provider_s_matches_show_before_the_other_answers() {
    let IdentifyStateView::Triangulating {
        groups, agreements, ..
    } = IdentifyStateView::from(in_flight(context()))
    else {
        panic!("a run in flight");
    };
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].sections[0].pressings[0].releases[0].release_id,
        "dg-1"
    );
    assert_eq!(agreements.len(), 1);
    assert_eq!(agreements[0].0, "dg-1");
    assert!(agreements[0].1.barcode);
}

/// A code left out is still a row, its cells saying it was left out, beside the
/// code that was asked.
#[test]
fn a_code_left_out_is_a_row_that_says_nobody_was_asked() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("BOXSET".to_string()),
        SourcedValue::new("DISC".to_string()),
    ];
    context.barcode.excluded = vec!["BOXSET".to_string()];
    context.providers = vec![MB];
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode: BarcodeProgress::Lookups {
            codes: vec![code("DISC", vec![(MB, found(vec![result(MB, "mb-1")]))])],
        },
        catalog: CatalogProgress::Skipped,
        search: SearchProgress::Pending,
        context,
    };
    let run = run_of(state);
    let rows = barcode_rows(&run);
    assert_eq!(
        rows.iter()
            .map(|row| (row.value.as_str(), row.excluded))
            .collect::<Vec<_>>(),
        vec![("BOXSET", true), ("DISC", false)]
    );
    assert_eq!(
        cells(&rows[0]),
        vec![&LookupView::NotAsked {
            reason: NotAskedReason::LeftOut
        }]
    );
    assert!(matches!(
        cells(&rows[1]).as_slice(),
        [LookupView::Found { count: 1, .. }]
    ));
}

/// With every code left out, each code is still a row whose cells say it was
/// left out.
#[test]
fn every_code_left_out_lists_them_all_unasked() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("BOXSET".to_string()),
        SourcedValue::new("DISC".to_string()),
    ];
    context.barcode.excluded = vec!["BOXSET".to_string(), "DISC".to_string()];
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode: BarcodeProgress::NotAsked {
            codes: vec!["BOXSET".to_string(), "DISC".to_string()],
            reason: NotAskedReason::LeftOut,
        },
        catalog: CatalogProgress::Skipped,
        search: SearchProgress::Pending,
        context,
    };
    let run = run_of(state);
    let rows = barcode_rows(&run);
    assert!(rows.iter().all(|row| row.excluded));
    let left_out = LookupView::NotAsked {
        reason: NotAskedReason::LeftOut,
    };
    assert!(rows
        .iter()
        .all(|row| cells(row) == vec![&left_out, &left_out]));
}

/// A disc ID nobody looked up still reads as read, with its lookup saying why.
#[test]
fn a_disc_id_nobody_looked_up_says_why() {
    for reason in [
        NotAskedReason::LeftOut,
        NotAskedReason::SwitchedOff,
        NotAskedReason::NoCatalog,
    ] {
        let mut context = context();
        context.disc.signal = DiscIdSignal::Computed {
            disc_id: "d".to_string(),
            source_file: Some("rip/Album.LOG".to_string()),
        };
        let step = run_of(IdentifyState::Triangulating {
            discid: DiscidProgress::NotAsked { reason },
            barcode: BarcodeProgress::NoCodes,
            catalog: CatalogProgress::Skipped,
            search: SearchProgress::Pending,
            context,
        })
        .disc_id;
        assert_eq!(
            step,
            DiscIdStepView::Read {
                disc_id: "d".to_string(),
                lookup: LookupView::NotAsked { reason },
            }
        );
    }
}

/// Each code's row shows how each provider answered about that code alone.
#[test]
fn each_code_fills_its_own_cells() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("A".to_string()),
        SourcedValue::new("B".to_string()),
        SourcedValue::new("C".to_string()),
    ];
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode: BarcodeProgress::Lookups {
            codes: vec![
                code(
                    "A",
                    vec![
                        (MB, found(vec![result(MB, "mb-1")])),
                        (DG, found(Vec::new())),
                    ],
                ),
                code(
                    "B",
                    vec![
                        (MB, LookupState::LookingUp),
                        (DG, found(vec![result(DG, "dg-1"), result(DG, "dg-2")])),
                    ],
                ),
                code(
                    "C",
                    vec![
                        (
                            MB,
                            LookupState::Failed {
                                failure: LookupFailure::Timeout,
                            },
                        ),
                        (DG, found(Vec::new())),
                    ],
                ),
            ],
        },
        catalog: CatalogProgress::Skipped,
        search: SearchProgress::Pending,
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
    assert!(matches!(
        cells(&rows[0]).as_slice(),
        [LookupView::Found { count: 1, .. }, LookupView::NoMatch]
    ));
    assert!(matches!(
        cells(&rows[1]).as_slice(),
        [LookupView::LookingUp, LookupView::Found { count: 2, .. }]
    ));
    assert_eq!(
        cells(&rows[2]),
        vec![
            &LookupView::Failed {
                failure: LookupFailure::Timeout
            },
            &LookupView::NoMatch
        ]
    );
}

/// The same code read off two files is one row.
#[test]
fn a_code_seen_in_two_places_is_one_row() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::in_file("A".to_string(), "disc.cue".to_string()),
        SourcedValue::in_file("A".to_string(), "back.jpg".to_string()),
    ];
    let run = run_of(in_flight(context));
    let rows = barcode_rows(&run);
    assert_eq!(
        rows.iter()
            .map(|row| row.value.as_str())
            .collect::<Vec<_>>(),
        vec!["A"]
    );
}

/// While the artwork is being read, each code read so far is a row of queued
/// cells.
#[test]
fn codes_read_so_far_wait_while_the_artwork_is_still_being_read() {
    let mut context = context();
    context.artwork = ArtworkScan::Reading {
        current: Some("Back.jpg".to_string()),
        position: 2,
        total: 3,
    };
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode: BarcodeProgress::Scanning,
        catalog: CatalogProgress::Skipped,
        search: SearchProgress::Pending,
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

/// Chosen catalog numbers are rows and the rest are tiles, one per number.
#[test]
fn chosen_catalog_numbers_are_rows_and_the_rest_are_tiles() {
    let mut context = context();
    context.catalog.numbers = vec![
        "LBL-1".to_string(),
        "LBL-2".to_string(),
        "LBL-3".to_string(),
    ];
    context.catalog.chosen = vec![ChosenCatalog {
        value: "LBL-2".to_string(),
        results: Vec::new(),
        failures: Vec::new(),
    }];
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode: BarcodeProgress::NoCodes,
        catalog: CatalogProgress::Lookups {
            values: vec![ValueLookup {
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
        search: SearchProgress::Pending,
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

/// A settled state's ledger is the run's last frame with the last answers
/// filled in.
#[test]
fn a_settled_state_carries_the_ledger_its_last_frame_showed() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("A".to_string()),
        SourcedValue::new("B".to_string()),
    ];
    let in_flight = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode: BarcodeProgress::Lookups {
            codes: vec![
                code(
                    "A",
                    vec![
                        (MB, found(Vec::new())),
                        (
                            DG,
                            LookupState::Failed {
                                failure: LookupFailure::Network,
                            },
                        ),
                    ],
                ),
                code(
                    "B",
                    vec![(MB, LookupState::LookingUp), (DG, found(Vec::new()))],
                ),
            ],
        },
        catalog: CatalogProgress::Skipped,
        search: SearchProgress::Pending,
        context,
    };
    let last_frame = run_of(in_flight.clone());
    let (settled, _) = step(
        in_flight,
        IdentifyEvent::BarcodeLookupAnswered {
            source: MB,
            for_barcode: "B".to_string(),
            outcome: Ok(vec![result(MB, "mb-1")]),
        },
    );
    assert!(matches!(settled, IdentifyState::Failed { .. }));
    let ledger = run_of(settled);

    assert_eq!(ledger.providers, last_frame.providers);
    assert_eq!(ledger.disc_id, last_frame.disc_id);
    let before = barcode_rows(&last_frame);
    let after = barcode_rows(&ledger);
    assert_eq!(cells(&after[0]), cells(&before[0]));
    assert!(matches!(
        cells(&before[1]).as_slice(),
        [LookupView::LookingUp, LookupView::NoMatch]
    ));
    assert!(matches!(
        cells(&after[1]).as_slice(),
        [LookupView::Found { count: 1, .. }, LookupView::NoMatch]
    ));
}

/// A folder with nothing to look up records no ledger.
#[test]
fn a_run_with_no_inputs_records_no_ledger() {
    let blank = SignalsContext {
        providers: Vec::new(),
        barcode: BarcodeEvidence::default(),
        ..context()
    };
    let settled = crate::identify::state::settle_for_tests(
        DiscidProgress::Skipped,
        BarcodeProgress::Skipped,
        CatalogProgress::Skipped,
        SearchProgress::Skipped,
        blank,
    );
    assert!(matches!(
        IdentifyStateView::from(settled),
        IdentifyStateView::ManualOnly { run: None, .. }
    ));
}

/// A folder with only catalog numbers still records a run that offers them as
/// tiles.
#[test]
fn a_manual_only_folder_with_catalog_numbers_offers_them() {
    let mut context = SignalsContext {
        providers: vec![MB],
        barcode: BarcodeEvidence::default(),
        ..context()
    };
    context.catalog.numbers = vec!["LBL-1".to_string()];
    let settled = crate::identify::state::settle_for_tests(
        DiscidProgress::Skipped,
        BarcodeProgress::Skipped,
        CatalogProgress::Skipped,
        SearchProgress::Skipped,
        context,
    );
    let IdentifyStateView::ManualOnly { run: Some(run), .. } = IdentifyStateView::from(settled)
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

/// A CUE over audio no CD holds says so, with the sample rate.
#[test]
fn a_sheet_over_audio_no_cd_holds_says_why_it_was_not_read() {
    let mut context = context();
    context.disc.signal = DiscIdSignal::NotCdAudio;
    let run = run_of(in_flight(context));
    assert_eq!(run.disc_id, DiscIdStepView::NotCdAudio);
}

/// A found lookup carries the album cards its count stands for.
#[test]
fn a_found_lookup_names_its_releases() {
    let mut context = context();
    context.disc.signal = DiscIdSignal::Computed {
        disc_id: "d".to_string(),
        source_file: Some("rip/Album.LOG".to_string()),
    };
    let state = IdentifyState::Triangulating {
        discid: DiscidProgress::Done {
            results: vec![result(MB, "mb-1"), result(MB, "mb-2")],
        },
        barcode: BarcodeProgress::NoCodes,
        catalog: CatalogProgress::Skipped,
        search: SearchProgress::Pending,
        context,
    };
    let run = run_of(state);
    let DiscIdStepView::Read {
        lookup: LookupView::Found { count, groups },
        ..
    } = run.disc_id
    else {
        panic!("a found disc ID, got {:?}", run.disc_id);
    };
    assert_eq!(count, 2);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].pressings().count(), 2);
}

// ── Resuming a stored verdict ───────────────────────────────────────────────

/// A recorded ledger: a disc ID that found one release, and a barcode
/// MusicBrainz found nothing for and Discogs found one.
fn recorded_ledger() -> IdentifyRunView {
    IdentifyRunView {
        providers: vec![MB, DG],
        disc_id: DiscIdStepView::Read {
            disc_id: "disc-1".to_string(),
            lookup: LookupView::Found {
                count: 1,
                groups: group_results(unranked(vec![MetadataResult::for_test(
                    MB,
                    "mb-1",
                    Some("g"),
                )])),
            },
        },
        barcode: BarcodeStepView::Rows {
            scanning: false,
            rows: vec![SignalValueRow {
                value: "0123456789012".to_string(),
                excluded: false,
                cells: vec![
                    ProviderCell {
                        source: MB,
                        lookup: LookupView::NoMatch,
                    },
                    ProviderCell {
                        source: DG,
                        lookup: LookupView::Found {
                            count: 1,
                            groups: group_results(unranked(vec![MetadataResult::for_test(
                                DG,
                                "dg-1",
                                Some("g"),
                            )])),
                        },
                    },
                ],
            }],
        },
        catalog: CatalogStepView::NoneFound,
        search: SearchStepView::NotNeeded,
    }
}

pub(super) fn not_in_library(result: &MetadataResult) -> LibraryStatus {
    LibraryStatus::absent(&result.release_id)
}

/// A resumed verdict shows its recorded ledger unchanged, including a provider
/// whose answers were narrowed out.
#[test]
fn a_resumed_verdict_shows_the_ledger_its_run_recorded() {
    let verdict = TerminalVerdict::Found {
        findings: Findings {
            matches: vec![MetadataResult::for_test(MB, "mb-1", Some("g"))],
            provenance: vec![LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
                by_search: false,
                named_by: None,
            }],
            pressings: vec![0],
            narrowed_out: NarrowedOut {
                matches: vec![MetadataResult::for_test(DG, "dg-1", Some("g"))],
                provenance: vec![LookupProvenance {
                    by_disc_id: false,
                    by_barcode: true,
                    by_catalog: false,
                    by_search: false,
                    named_by: None,
                }],
                pressings: vec![0],
            },
            medium_conflict: None,
        },
        track_count: 9,
        ledger: Some(recorded_ledger()),
    };
    let run = run_of(verdict.resume_state(&not_in_library, Default::default(), Default::default()));
    assert_eq!(run, recorded_ledger());
    assert_eq!(run.providers, vec![MB, DG]);
    assert!(matches!(
        cells(&barcode_rows(&run)[0]).as_slice(),
        [LookupView::NoMatch, LookupView::Found { count: 1, .. }]
    ));
}

/// A verdict with no recorded ledger resumes without one.
#[test]
fn a_verdict_with_no_recorded_ledger_resumes_without_one() {
    let verdict = TerminalVerdict::Failed {
        failures: vec![IdentifyFailure::DiscId(LookupFailure::Network)],
        findings: Findings::default(),
        track_count: 9,
        ledger: None,
    };
    assert!(matches!(
        IdentifyStateView::from(verdict.resume_state(
            &not_in_library,
            Default::default(),
            Default::default()
        )),
        IdentifyStateView::Failed { run: None, .. }
    ));
}

// ── What agreement narrowed out ─────────────────────────────────────────────

/// Rows agreement set aside stay on their album's card, badged and with a
/// status; an album with no offered row is its own card behind the disclosure.
#[test]
fn what_agreement_narrowed_out_stays_on_its_album_s_card() {
    let mut context = context();
    context.disc.signal = DiscIdSignal::Computed {
        disc_id: "d".to_string(),
        source_file: None,
    };
    let other_album = (
        MetadataResult::for_test(MB, "mb-other", Some("g-other")),
        LibraryStatus::absent("mb-other"),
    );
    context.disc.results = vec![
        result(MB, "mb-shared"),
        result(MB, "mb-only"),
        other_album.clone(),
    ];
    context.barcode.results = vec![result(MB, "mb-shared")];
    context.barcode.matched = Some("A".to_string());

    let IdentifyStateView::Found {
        groups,
        library_statuses,
        agreements,
        narrowed_out,
        ..
    } = IdentifyStateView::from(crate::identify::state::re_derive_for_tests(context))
    else {
        panic!("the signals agree on one release");
    };
    assert_eq!(
        groups.len(),
        1,
        "the album is one card across the disclosure"
    );
    assert_eq!(groups[0].sections.len(), 1);
    let rows = |pressings: &[crate::import::release_group::Pressing]| -> Vec<String> {
        pressings
            .iter()
            .map(|pressing| pressing.lead().release_id.clone())
            .collect()
    };
    assert_eq!(rows(&groups[0].sections[0].pressings), vec!["mb-shared"]);
    assert_eq!(rows(&groups[0].sections[0].narrowed_out), vec!["mb-only"]);
    assert_eq!(narrowed_out.count, 2);
    assert_eq!(narrowed_out.groups.len(), 1);
    assert_eq!(narrowed_out.groups[0].id, "g-other");
    assert_eq!(narrowed_out.groups[0].pressings().count(), 0);
    assert_eq!(
        rows(&narrowed_out.groups[0].sections[0].narrowed_out),
        vec!["mb-other"]
    );
    assert_eq!(
        library_statuses
            .iter()
            .map(|status| status.release_id.as_str())
            .collect::<Vec<_>>(),
        vec!["mb-shared", "mb-only", "mb-other"]
    );
    let only = agreements
        .iter()
        .find(|(release_id, _)| release_id == "mb-only")
        .expect("a row set aside is badged");
    assert!(only.1.disc_id);
    assert!(!only.1.barcode);
}

/// When the disc ID and barcode name different releases, the disc ID's is
/// offered and the barcode's is set aside.
#[test]
fn the_disc_id_s_release_outranks_a_barcode_that_named_another() {
    let mut context = context();
    context.disc.signal = DiscIdSignal::Computed {
        disc_id: "d".to_string(),
        source_file: None,
    };
    context.disc.results = vec![result(MB, "mb-disc")];
    context.barcode.results = vec![result(MB, "mb-barcode")];
    context.barcode.matched = Some("A".to_string());

    let IdentifyStateView::Found {
        groups,
        narrowed_out,
        ..
    } = IdentifyStateView::from(crate::identify::state::re_derive_for_tests(context))
    else {
        panic!("both releases are offered");
    };
    assert_eq!(groups[0].pressings().count(), 1);
    assert_eq!(groups[0].narrowed_out().count(), 1);
    assert_eq!(narrowed_out.count, 1);
}
