use super::*;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::LookupFailure;
use bae_identify::signals::{DiscIdSignal, SourcedValue};
use bae_identify::state::{BarcodeEvidence, DiscIdEvidence, SignalsContext};
use bae_identify::{
    BarcodeProgress, CatalogProgress, DiscidProgress, IdentifyState, LookupState, ProviderLookup,
    ValueLookup,
};

fn in_flight(barcode: BarcodeProgress) -> IdentifyState {
    IdentifyState::Triangulating {
        discid: DiscidProgress::Skipped,
        barcode,
        catalog: CatalogProgress::Skipped,
        isrc: bae_identify::IsrcProgress::Skipped,
        search: bae_identify::SearchProgress::Skipped,
        context: SignalsContext {
            providers: vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs],
            artwork: bae_identify::signals::ArtworkScan::Absent,
            origin: bae_catalogs::AudioOrigin::default(),
            disc: DiscIdEvidence { signal: DiscIdSignal::Absent, ..Default::default() },
            barcode: BarcodeEvidence {
                codes: vec![SourcedValue::in_file(
                    "0123456789012".to_string(),
                    "back.jpg".to_string(),
                )],
                had_source: true,
                ..Default::default()
            },
            catalog: Default::default(),
            search: Default::default(),
            text: Default::default(),
            text_settled: true,
            audio: bae_identify::signals::AudioFacts { track_count: 9, ..Default::default() },
            isrc: Default::default(),
            track_titles: Vec::new(),
            documents: bae_identify::documents::DocumentReading::Pending,
            pressings: Vec::new(),
        },
    }
}

fn run_of(state: IdentifyState) -> BridgeIdentifyRun {
    match BridgeIdentifyState::from_core(state) {
        BridgeIdentifyState::Triangulating { run, .. } => run,
        other => panic!("expected a run in flight, got {other:?}"),
    }
}

fn barcode_step(state: IdentifyState) -> BridgeBarcodeStep {
    run_of(state).barcode
}

/// Whether the artwork is still being read crosses once, for the run.
#[test]
fn the_run_says_whether_the_artwork_is_still_being_read() {
    assert!(run_of(in_flight(BarcodeProgress::Scanning)).reading_artwork);
    assert!(!run_of(in_flight(BarcodeProgress::Lookups { codes: Vec::new() })).reading_artwork);
}

/// A code crosses as one row with one cell per provider, each its own.
#[test]
fn a_code_crosses_as_a_row_with_one_cell_per_provider() {
    let step = barcode_step(in_flight(BarcodeProgress::Lookups {
        codes: vec![ValueLookup {
            value: "0123456789012".to_string(),
            providers: vec![
                ProviderLookup {
                    source: LookupCatalog::MusicBrainz,
                    state: LookupState::LookingUp,
                },
                ProviderLookup {
                    source: LookupCatalog::Discogs,
                    state: LookupState::Failed {
                        failure: LookupFailure::Provider { status: Some(500) },
                    },
                },
            ],
        }],
    }));
    let BridgeBarcodeStep::Rows { rows } = step else {
        panic!("a lookup in flight crosses as rows, got {step:?}");
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, "0123456789012");
    assert!(!rows[0].excluded);
    assert_eq!(rows[0].cells.len(), 2);
    assert_eq!(rows[0].cells[0].source, BridgeLookupCatalog::MusicBrainz);
    assert!(matches!(rows[0].cells[0].lookup, BridgeLookupState::LookingUp));
    assert_eq!(rows[0].cells[1].source, BridgeLookupCatalog::Discogs);
    assert!(matches!(
        &rows[0].cells[1].lookup,
        BridgeLookupState::Failed { failure: BridgeLookupFailure::Provider { status: Some(500) } }
    ));
}

/// A code the person left out crosses as a row whose cells say so.
#[test]
fn a_code_left_out_crosses_as_a_row_that_says_so() {
    let mut state = in_flight(BarcodeProgress::NotAsked {
        codes: vec!["0123456789012".to_string()],
        reason: bae_identify::NotAskedReason::LeftOut,
    });
    let IdentifyState::Triangulating { context, .. } = &mut state else {
        panic!("a run in flight");
    };
    context.barcode.excluded = vec!["0123456789012".to_string()];
    let step = barcode_step(state);
    let BridgeBarcodeStep::Rows { rows } = step else {
        panic!("a left-out code still crosses as a row, got {step:?}");
    };
    assert_eq!(rows.len(), 1);
    assert!(rows[0].excluded);
    assert!(rows[0].cells.iter().all(|cell| matches!(
        cell.lookup,
        BridgeLookupState::NotAsked { reason: BridgeNotAskedReason::LeftOut }
    )));
}

/// A disc ID nobody looked up crosses as read, with why.
#[test]
fn a_disc_id_nobody_looked_up_crosses_with_why() {
    use bae_identify::NotAskedReason;
    let step = |reason: NotAskedReason| {
        let mut state = in_flight(BarcodeProgress::Skipped);
        let IdentifyState::Triangulating { discid, context, .. } = &mut state else {
            panic!("a run in flight");
        };
        *discid = DiscidProgress::NotAsked { reason };
        context.disc.signal = DiscIdSignal::Computed {
            discs: vec![bae_identify::signals::ComputedDiscId {
                disc_id: "d".to_string(),
                source_file: None,
            }],
        };
        match BridgeIdentifyState::from_core(state) {
            BridgeIdentifyState::Triangulating { run, .. } => match run.disc_id {
                BridgeDiscIdStep::Read { discs } => discs,
                other => panic!("expected read Disc IDs, got {other:?}"),
            },
            other => panic!("expected a run in flight, got {other:?}"),
        }
    };
    for (reason, crossed) in [
        (NotAskedReason::LeftOut, BridgeNotAskedReason::LeftOut),
        (NotAskedReason::NoCatalog, BridgeNotAskedReason::NoCatalog),
    ] {
        assert!(matches!(
            step(reason).as_slice(),
            [BridgeDiscIdRow {
                lookup: BridgeLookupState::NotAsked { reason },
                ..
            }] if *reason == crossed
        ));
    }
}

/// An unread sheet crosses with the rate of the audio the run was over:
/// the stored step states no number, the run's signals do.
#[test]
fn an_unread_sheet_crosses_with_the_audio_s_rate() {
    let mut state = in_flight(BarcodeProgress::Skipped);
    let IdentifyState::Triangulating { context, .. } = &mut state else {
        panic!("a run in flight");
    };
    context.origin.not_cd_rate = Some(96_000);
    context.disc.signal = DiscIdSignal::NotCdAudio;
    match BridgeIdentifyState::from_core(state) {
        BridgeIdentifyState::Triangulating { run, .. } => assert!(matches!(
            run.disc_id,
            BridgeDiscIdStep::NotCdAudio { sample_rate_hz: Some(96_000) }
        )),
        other => panic!("expected a run in flight, got {other:?}"),
    }
}

/// A run bae broke crosses as an error with its text.
#[test]
fn a_run_bae_broke_crosses_as_an_error_with_its_text() {
    let failure = bae_catalogs::InternalFailure {
        detail: "checking the library: the store is locked".to_string(),
    };
    let IdentifyState::Triangulating { context, .. } = in_flight(BarcodeProgress::Skipped) else {
        unreachable!("in_flight builds a run in flight");
    };
    let state = IdentifyState::Error { failure: failure.clone(), context };
    assert_eq!(
        match BridgeIdentifyState::from_core(state) {
            BridgeIdentifyState::Error { failure } => failure.detail,
            other => panic!("expected an error, got {other:?}"),
        },
        failure.detail
    );
}

/// A pressing row crosses with every record's year, each naming the catalog
/// that states it, and every record's country.
#[test]
fn a_pressing_row_crosses_with_every_record_s_values() {
    use bae_catalogs::search::MetadataResult;
    use bae_types::pressing::{Country, ReleaseArea};

    let record = |source, year, country: &str| MetadataResult {
        year: Some(year),
        area: Some(ReleaseArea::Country(Country::from_code(country).expect("a country"))),
        ..MetadataResult::for_test(source, "id", None)
    };
    let row = BridgePressing::from_core(bae_identify::release_group::Pressing::over(vec![
        record(LookupCatalog::MusicBrainz, 1992, "US"),
        record(LookupCatalog::Discogs, 1993, "DE"),
    ]));
    assert_eq!(
        row.years,
        vec![
            BridgeYearStatement { year: 1992, catalogs: vec![BridgeLookupCatalog::MusicBrainz] },
            BridgeYearStatement { year: 1993, catalogs: vec![BridgeLookupCatalog::Discogs] },
        ]
    );
    assert_eq!(
        row.areas,
        vec![
            BridgeTermsStatement {
                terms: vec![BridgeFactTerm::Country { code: "US".to_string() }],
                catalogs: vec![BridgeLookupCatalog::MusicBrainz],
            },
            BridgeTermsStatement {
                terms: vec![BridgeFactTerm::Country { code: "DE".to_string() }],
                catalogs: vec![BridgeLookupCatalog::Discogs],
            },
        ]
    );
}
