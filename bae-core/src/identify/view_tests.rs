use super::*;
use crate::db::LibraryStatus;
use crate::identify::state::{
    step, BarcodeEvidence, BarcodeLookupState, CatalogEvidence, ChosenCatalog, DiscIdEvidence,
    IdentifyEvent, ProviderBarcodeLookup, ProviderLookup,
};
use crate::identify::{IdentifyFailure, TerminalVerdict};
use crate::import::release_group::unranked;
use crate::import::search::MetadataResult;
use crate::import::MetadataSource;
use crate::signals::{SignalOrigin, SourcedValue, TextLine};

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
        text: Default::default(),
        text_settled: true,
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

/// What one provider found shows while the other is still looking, badged as
/// the settled verdict will badge it.
#[test]
fn a_landed_provider_s_matches_show_before_the_other_answers() {
    let IdentifyStateView::Triangulating {
        groups, agreements, ..
    } = IdentifyStateView::from(in_flight(context()))
    else {
        panic!("a run in flight");
    };
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].pressings[0].releases[0].release_id, "dg-1");
    assert_eq!(agreements.len(), 1);
    assert_eq!(agreements[0].0, "dg-1");
    assert!(agreements[0].1.barcode);
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

/// The ledger a settled state carries is the frame the run last showed, with
/// whatever was still being asked settled onto it. Nothing is rebuilt, so a
/// cell that had landed does not move when the run ends.
#[test]
fn a_settled_state_carries_the_ledger_its_last_frame_showed() {
    let mut context = context();
    context.barcode.codes = vec![
        SourcedValue::new("A".to_string(), SignalOrigin::Artwork),
        SourcedValue::new("B".to_string(), SignalOrigin::Artwork),
    ];
    let in_flight = IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped { track_count: 9 },
        barcode: BarcodeProgress::Lookups {
            codes: vec!["A".to_string(), "B".to_string()],
            providers: vec![
                ProviderBarcodeLookup {
                    source: MB,
                    state: BarcodeLookupState::Trying { index: 1 },
                },
                ProviderBarcodeLookup {
                    source: DG,
                    state: BarcodeLookupState::Failed {
                        failure: LookupFailure::Network,
                        index: 0,
                    },
                },
            ],
        },
        catalog: CatalogProgress::Skipped,
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
        [LookupView::LookingUp, LookupView::NotAsked]
    ));
    assert!(matches!(
        cells(&after[1]).as_slice(),
        [LookupView::Found { count: 1, .. }, LookupView::NotAsked]
    ));
}

/// A folder that carries nothing to look up records no ledger: there is no
/// run to lay out, so the pane offers manual search on its own.
#[test]
fn a_run_with_no_inputs_records_no_ledger() {
    let blank = SignalsContext {
        providers: Vec::new(),
        barcode: BarcodeEvidence::default(),
        ..context()
    };
    let settled = crate::identify::state::settle_for_tests(
        DiscidProgress::Skipped { track_count: 9 },
        BarcodeProgress::Skipped,
        CatalogProgress::Skipped,
        blank,
    );
    assert!(matches!(
        IdentifyStateView::from(settled),
        IdentifyStateView::ManualOnly { run: None, .. }
    ));
}

/// A folder with nothing to look up automatically but catalog numbers to
/// offer still records a run: the tiles, waiting to be activated.
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
    let settled = crate::identify::state::settle_for_tests(
        DiscidProgress::Skipped { track_count: 9 },
        BarcodeProgress::Skipped,
        CatalogProgress::Skipped,
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

/// One run's recorded ledger: a disc ID read off a rip log that named one
/// release, and one barcode both providers were asked about — MusicBrainz
/// finding nothing, Discogs finding a release of its own.
fn recorded_ledger() -> IdentifyRunView {
    IdentifyRunView {
        providers: vec![MB, DG],
        disc_id: DiscIdStepView::Read {
            disc_id: "disc-1".to_string(),
            source: Some(DiscIdFile {
                kind: DiscIdFileKind::Log,
                file: "rip/Album.LOG".to_string(),
            }),
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
                sources: vec![ValueSource {
                    origin: SignalOrigin::Artwork,
                    file: Some("back.jpg".to_string()),
                    region: None,
                }],
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
    }
}

fn not_in_library(result: &MetadataResult) -> LibraryStatus {
    LibraryStatus::absent(&result.release_id)
}

/// A stored verdict shows the ledger its run recorded, cell for cell — every
/// provider the run asked has its column, including one whose every answer
/// the agreement then narrowed out of the matches.
#[test]
fn a_resumed_verdict_shows_the_ledger_its_run_recorded() {
    let verdict = TerminalVerdict::Found {
        matches: vec![MetadataResult::for_test(MB, "mb-1", Some("g"))],
        track_count: 9,
        provenance: vec![LookupProvenance {
            by_disc_id: true,
            by_barcode: false,
            by_catalog: false,
        }],
        narrowed_out: vec![MetadataResult::for_test(DG, "dg-1", Some("g"))],
        narrowed_out_provenance: vec![LookupProvenance {
            by_disc_id: false,
            by_barcode: true,
            by_catalog: false,
        }],
        ledger: Some(recorded_ledger()),
    };
    let run = run_of(verdict.resume_state(&not_in_library, Default::default()));
    assert_eq!(run, recorded_ledger());
    assert_eq!(run.providers, vec![MB, DG]);
    assert!(matches!(
        cells(&barcode_rows(&run)[0]).as_slice(),
        [LookupView::NoMatch, LookupView::Found { count: 1, .. }]
    ));
}

/// A verdict with no recorded ledger has none to show, and the pane offers
/// the re-run in the failure lines instead.
#[test]
fn a_verdict_with_no_recorded_ledger_resumes_without_one() {
    let verdict = TerminalVerdict::Failed {
        failures: vec![IdentifyFailure::DiscId(LookupFailure::Network)],
        track_count: 9,
        ledger: None,
    };
    assert!(matches!(
        IdentifyStateView::from(verdict.resume_state(&not_in_library, Default::default())),
        IdentifyStateView::Failed { run: None, .. }
    ));
}

// ── What agreement narrowed out ─────────────────────────────────────────────

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
    assert_eq!(narrowed_out.agreements[0].0, "mb-only");
    assert!(narrowed_out.agreements[0].1.disc_id);
    assert!(!narrowed_out.agreements[0].1.barcode);
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

// ── The Catalog # row's agreement chips ─────────────────────────────────────

fn folder(lines: &[&str], struck_out: &[&str]) -> CandidateText {
    let pool: Vec<TextLine> = lines
        .iter()
        .map(|text| TextLine {
            text: (*text).to_string(),
            origin: SignalOrigin::FolderName,
            file: None,
            region: None,
        })
        .collect();
    let struck_out: Vec<String> = struck_out
        .iter()
        .map(|value| (*value).to_string())
        .collect();
    CandidateText::of(&pool, &struck_out)
}

/// One pressing as a provider states it — whichever of its fields the folder
/// then turns out to state is what ranks it.
fn pressing(
    release_id: &str,
    group_id: &str,
    catalog: Option<&str>,
    label: Option<&str>,
    year: Option<i32>,
    country: Option<&str>,
) -> MetadataResult {
    MetadataResult {
        catalog_number: catalog.map(str::to_string),
        label: label.map(str::to_string),
        year,
        country: country.map(str::to_string),
        ..MetadataResult::for_test(MB, release_id, Some(group_id))
    }
}

/// A stored verdict read back: the matches it settled on, what it narrowed
/// out, and the candidate's own text as the person has left it.
fn resumed(
    matches: Vec<MetadataResult>,
    narrowed_out: Vec<MetadataResult>,
    ledger: Option<IdentifyRunView>,
    text: CandidateText,
) -> IdentifyStateView {
    let by_disc_id = |count: usize| {
        vec![
            LookupProvenance {
                by_disc_id: true,
                by_barcode: false,
                by_catalog: false,
            };
            count
        ]
    };
    let verdict = TerminalVerdict::Found {
        provenance: by_disc_id(matches.len()),
        narrowed_out_provenance: by_disc_id(narrowed_out.len()),
        matches,
        narrowed_out,
        track_count: 9,
        ledger,
    };
    IdentifyStateView::from(verdict.resume_state(&not_in_library, text))
}

fn chips(view: &IdentifyStateView) -> Vec<(&str, bool)> {
    let IdentifyStateView::Found {
        catalog_agreements, ..
    } = view
    else {
        panic!("a settled verdict, got {view:?}");
    };
    catalog_agreements
        .iter()
        .map(|chip| (chip.value.as_str(), chip.discounted))
        .collect()
}

fn card_ids(view: &IdentifyStateView) -> Vec<&str> {
    let IdentifyStateView::Found { groups, .. } = view else {
        panic!("a settled verdict, got {view:?}");
    };
    groups
        .iter()
        .map(|group| group.pressings[0].releases[0].release_id.as_str())
        .collect()
}

/// The chips are the numbers the folder states about a release it is
/// offering. A number the folder states about one it set aside is not one of
/// them: that release is behind the disclosure, and nothing it says ranks the
/// list.
#[test]
fn the_chips_are_the_numbers_the_offered_releases_carry() {
    let view = resumed(
        vec![pressing("rel-a", "rg-a", Some("16033-2"), None, None, None)],
        vec![pressing("rel-b", "rg-b", Some("SD-19"), None, None, None)],
        None,
        folder(&["Dirty Deeds [16033-2]", "Atlantic SD-19"], &[]),
    );
    assert_eq!(chips(&view), vec![("16033-2", false)]);
}

/// A number the folder never states is no chip, however many releases carry
/// it: the chip is what the folder says, not what a provider does.
#[test]
fn a_number_the_folder_never_states_is_no_chip() {
    let view = resumed(
        vec![pressing("rel-a", "rg-a", Some("16033-2"), None, None, None)],
        Vec::new(),
        None,
        folder(&["Dirty Deeds"], &[]),
    );
    assert!(chips(&view).is_empty());
}

/// Striking a number out takes the agreement off every release that carried
/// it and re-orders the list, from the same stored verdict — and the chip
/// stays, checked off, so it can be taken back.
#[test]
fn striking_a_number_out_drops_its_agreement_and_re_ranks() {
    let lines = &["Dirty Deeds [16033-2]", "Atlantic 1976 US"];
    let matches = || {
        vec![
            pressing(
                "rel-a",
                "rg-a",
                Some("16033-2"),
                Some("Atlantic"),
                Some(1976),
                None,
            ),
            pressing(
                "rel-b",
                "rg-b",
                None,
                Some("Atlantic"),
                Some(1976),
                Some("US"),
            ),
        ]
    };
    let counted = resumed(matches(), Vec::new(), None, folder(lines, &[]));
    let IdentifyStateView::Found { agreements, .. } = &counted else {
        panic!("a settled verdict");
    };
    assert!(agreements.iter().any(|(id, a)| id == "rel-a" && a.catalog));
    assert_eq!(card_ids(&counted), vec!["rel-a", "rel-b"]);

    let struck = resumed(matches(), Vec::new(), None, folder(lines, &["16033-2"]));
    let IdentifyStateView::Found { agreements, .. } = &struck else {
        panic!("a settled verdict");
    };
    assert!(agreements.iter().any(|(id, a)| id == "rel-a" && !a.catalog));
    assert_eq!(card_ids(&struck), vec!["rel-b", "rel-a"]);
    assert_eq!(chips(&struck), vec![("16033-2", true)]);
}

/// A number one of the offered releases carries is a chip that ranks them, so
/// it is not also a tile that would look it up: the tiles under the table are
/// the numbers nothing came back carrying.
#[test]
fn a_number_an_offered_release_carries_is_a_chip_rather_than_a_tile() {
    let ledger = IdentifyRunView {
        providers: vec![MB],
        disc_id: DiscIdStepView::Absent,
        barcode: BarcodeStepView::Absent,
        catalog: CatalogStepView::Numbers {
            scanning: false,
            rows: Vec::new(),
            candidates: vec![
                CatalogCandidateView {
                    value: "16033 2".to_string(),
                    sources: Vec::new(),
                },
                CatalogCandidateView {
                    value: "SD-19".to_string(),
                    sources: Vec::new(),
                },
            ],
        },
    };
    let view = resumed(
        vec![pressing("rel-a", "rg-a", Some("16033-2"), None, None, None)],
        Vec::new(),
        Some(ledger),
        folder(&["Dirty Deeds [16033-2]", "SD-19"], &[]),
    );
    assert_eq!(chips(&view), vec![("16033-2", false)]);
    let IdentifyStateView::Found { run: Some(run), .. } = &view else {
        panic!("a resumed ledger");
    };
    let CatalogStepView::Numbers { candidates, .. } = &run.catalog else {
        panic!("numbers, got {:?}", run.catalog);
    };
    assert_eq!(
        candidates
            .iter()
            .map(|tile| tile.value.as_str())
            .collect::<Vec<_>>(),
        vec!["SD-19"]
    );
}
