//! The run as a whole: how a run starts, takes its snapshots, ends on bae's
//! own failure or the person's cancel, and combines what its lookups found
//! into where it lands. The helpers here serve every topic's tests.

use super::*;
use crate::documents::{ReleaseDocument, ReleaseReading};
use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, SourcedValue, TextSignal};
use crate::state::PressingKey;
use crate::LookupChoices;
use bae_catalogs::album_links::AlbumLinks;
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::records::LookupRef;
use bae_catalogs::records::MetadataRef;

/// [`super::step`], answering each read of the offered records' documents at
/// once with none to be had, and each lookup of an offered row's pressing on
/// another catalog with nothing found: these tests are about the folder's
/// own keys, and a run whose documents and pressings add nothing settles on
/// what those found. The documents' and the pressings' own tests drive
/// `ReleasesRead` and `PressingLookupAnswered` themselves.
pub(super) fn step(state: IdentifyState, event: IdentifyEvent) -> (IdentifyState, Vec<Effect>) {
    let (mut state, mut effects) = super::step(state, event);
    while let Some(at) = effects.iter().position(|effect| {
        matches!(effect, Effect::ReadReleases { .. } | Effect::LookupPressing { .. })
    }) {
        let answer = match effects.remove(at) {
            Effect::ReadReleases { releases, .. } => IdentifyEvent::ReleasesRead {
                read: releases
                    .into_iter()
                    .map(|release| crate::documents::ReleaseReading {
                        release: release.release().clone(),
                        document: Err(LookupFailure::Network),
                    })
                    .collect(),
            },
            Effect::LookupPressing { source, key } => {
                IdentifyEvent::PressingLookupAnswered { source, key, outcome: Ok(Vec::new()) }
            }
            _ => unreachable!("the position is of a read or a pressing lookup"),
        };
        let (next, more) = super::step(state, answer);
        state = next;
        effects.extend(more);
    }
    (state, effects)
}

/// Whether a lookup of the folder's disc IDs found `result`, as the run's
/// ledger records it.
pub(super) fn found_by_disc_id(
    ledger: Option<&crate::IdentifyRunView>,
    result: &MetadataResult,
) -> bool {
    ledger.is_some_and(|ledger| {
        ledger
            .disc_id_releases()
            .contains(&bae_catalogs::records::LookupRef::new(result.source, &result.release_id))
    })
}

/// The records a read of documents asks for.
pub(super) fn records_read(releases: &[crate::documents::ReleaseToRead]) -> Vec<LookupRef> {
    releases.iter().map(|release| release.release().clone()).collect()
}

pub(super) fn mk_result(release_id: &str, group_id: Option<&str>) -> MetadataResult {
    mk_result_from(LookupCatalog::MusicBrainz, release_id, group_id)
}

pub(super) fn mk_result_from(
    source: LookupCatalog,
    release_id: &str,
    group_id: Option<&str>,
) -> MetadataResult {
    MetadataResult::for_test(source, release_id, group_id)
}

pub(super) fn pair(release_id: &str, group_id: Option<&str>) -> MetadataResult {
    mk_result(release_id, group_id)
}

/// A Discogs result, for runs where both providers answer.
pub(super) fn discogs_pair(release_id: &str, group_id: Option<&str>) -> MetadataResult {
    mk_result_from(LookupCatalog::Discogs, release_id, group_id)
}

pub(super) const MB: LookupCatalog = LookupCatalog::MusicBrainz;
pub(super) const DG: LookupCatalog = LookupCatalog::Discogs;

/// A run started with MusicBrainz as its only provider, waiting for signals.
pub(super) fn started() -> IdentifyState {
    started_with(vec![MB])
}

/// A run started with `providers`, nothing chosen or left out.
pub(super) fn started_with(providers: Vec<LookupCatalog>) -> IdentifyState {
    let (state, effects) = started_with_choices(providers, LookupChoices::default());
    assert!(effects.is_empty(), "with nothing chosen, Started dispatches no effects");
    state
}

/// The run's ledger as its view lays it out.
pub(super) fn ledger_of(state: &IdentifyState) -> crate::IdentifyRunView {
    match crate::IdentifyStateView::from(state.clone()) {
        crate::IdentifyStateView::Triangulating { run, .. } => run,
        crate::IdentifyStateView::Found { run, .. }
        | crate::IdentifyStateView::NotFoundAnywhere { run }
        | crate::IdentifyStateView::ManualOnly { run, .. }
        | crate::IdentifyStateView::Failed { run, .. } => run.expect("the run recorded its ledger"),
        crate::IdentifyStateView::Idle | crate::IdentifyStateView::Error { .. } => {
            panic!("an idle or broken state lays out no run")
        }
    }
}

/// A run started with `choices`, with the chosen numbers' lookups as effects.
pub(super) fn started_with_choices(
    providers: Vec<LookupCatalog>,
    choices: LookupChoices,
) -> (IdentifyState, Vec<Effect>) {
    step(IdentifyState::Idle, IdentifyEvent::Started { providers, choices, title_search: None })
}

/// Choices leaving out the disc ID (when `disc_id`) and the named barcodes.
pub(super) fn excluding(disc_id: bool, barcodes: &[&str]) -> LookupChoices {
    LookupChoices {
        disc_id_excluded: disc_id,
        excluded_barcodes: barcodes.iter().map(|value| value.to_string()).collect(),
        chosen_catalogs: Vec::new(),
        search_words: None,
        discounted_catalogs: Vec::new(),
    }
}

/// Choices picking the named catalog numbers and leaving nothing out.
pub(super) fn choosing(catalogs: &[&str]) -> LookupChoices {
    LookupChoices {
        disc_id_excluded: false,
        excluded_barcodes: Vec::new(),
        chosen_catalogs: catalogs.iter().map(|value| value.to_string()).collect(),
        search_words: None,
        discounted_catalogs: Vec::new(),
    }
}

/// Each disc ID's lookup, as the run's ledger lays it out.
pub(super) fn disc_lookups(state: &IdentifyState) -> Vec<crate::LookupView> {
    match ledger_of(state).disc_id {
        crate::DiscIdStepView::Read { discs } => {
            discs.into_iter().map(|disc| disc.lookup).collect()
        }
        other => panic!("expected the disc IDs read, got {other:?}"),
    }
}

/// The barcode rows of the run's ledger.
pub(super) fn barcode_rows(state: &IdentifyState) -> Vec<crate::SignalValueRow> {
    match ledger_of(state).barcode {
        crate::BarcodeStepView::Rows { rows, .. } => rows,
        other => panic!("expected the barcodes laid out, got {other:?}"),
    }
}

/// The catalog numbers of the run's ledger: the rows in effect or struck out,
/// and the numbers only offered.
pub(super) fn catalog_rows(state: &IdentifyState) -> (Vec<crate::SignalValueRow>, Vec<String>) {
    match ledger_of(state).catalog {
        crate::CatalogStepView::Numbers { rows, candidates, .. } => {
            (rows, candidates.into_iter().map(|candidate| candidate.value).collect())
        }
        other => panic!("expected the catalog numbers laid out, got {other:?}"),
    }
}

/// What `source` answered for `row`.
pub(super) fn cell(row: &crate::SignalValueRow, source: LookupCatalog) -> &crate::LookupView {
    &row.cells
        .iter()
        .find(|cell| cell.source == source)
        .unwrap_or_else(|| panic!("{} has a {source:?} cell", row.value))
        .lookup
}

/// Each row's value and whether the person left it out.
pub(super) fn left_out(rows: &[crate::SignalValueRow]) -> Vec<(&str, bool)> {
    rows.iter().map(|row| (row.value.as_str(), row.excluded)).collect()
}

/// The audio every run in these tests is over: five tracks.
pub(super) fn five_tracks() -> crate::signals::AudioFacts {
    crate::signals::AudioFacts {
        track_count: 5,
        track_lengths_ms: vec![200_000; 5],
        ..Default::default()
    }
}

pub(super) fn update(state: IdentifyState, signals: Signals) -> (IdentifyState, Vec<Effect>) {
    step(
        state,
        IdentifyEvent::SignalsUpdated {
            signals,
            audio: five_tracks(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    )
}

/// One provider matched `barcode`.
pub(super) fn barcode_matched(
    source: LookupCatalog,
    barcode: &str,
    results: Vec<MetadataResult>,
) -> IdentifyEvent {
    IdentifyEvent::BarcodeLookupAnswered {
        source,
        for_barcode: barcode.to_string(),
        outcome: Ok(results),
    }
}

/// One provider knew nothing about `barcode`.
pub(super) fn barcode_missed(source: LookupCatalog, barcode: &str) -> IdentifyEvent {
    barcode_matched(source, barcode, Vec::new())
}

/// One provider's lookup of `barcode` failed.
pub(super) fn barcode_failed(
    source: LookupCatalog,
    barcode: &str,
    failure: LookupFailure,
) -> IdentifyEvent {
    IdentifyEvent::BarcodeLookupAnswered {
        source,
        for_barcode: barcode.to_string(),
        outcome: Err(failure),
    }
}

pub(super) fn lookup_barcode(source: LookupCatalog, barcode: &str) -> Effect {
    Effect::LookupBarcode { source, barcode: barcode.to_string() }
}

/// Barcodes read off the artwork; the reducer reads only their values.
pub(super) fn artwork_codes(values: &[&str]) -> Vec<SourcedValue> {
    values.iter().map(|v| SourcedValue::new(v.to_string())).collect()
}

pub(super) fn signals(disc_id: DiscIdSignal, barcode: BarcodeSignal, catalogs: &[&str]) -> Signals {
    signals_with_catalogs(disc_id, barcode, catalogs.iter().map(|s| s.to_string()).collect())
}

pub(super) fn signals_with_catalogs(
    disc_id: DiscIdSignal,
    barcode: BarcodeSignal,
    catalogs: Vec<String>,
) -> Signals {
    Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id,
        barcode,
        text: TextSignal::Settled { catalogs, free_text: vec![] },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    }
}

/// A disc ID computed from no named file.
pub(super) fn disc(disc_id: &str) -> DiscIdSignal {
    DiscIdSignal::Computed {
        discs: vec![crate::signals::ComputedDiscId {
            disc_id: disc_id.to_string(),
            source_file: None,
        }],
    }
}

/// A disc ID for five tracks, no barcode source, and the named catalog numbers.
pub(super) fn disc_only(catalogs: &[&str]) -> Signals {
    signals(disc("d"), BarcodeSignal::Absent, catalogs)
}

/// A MusicBrainz run given the disc ID alone.
pub(super) fn disc_only_started() -> (IdentifyState, Vec<Effect>) {
    update(started(), disc_only(&[]))
}

/// A disc ID, with the given barcodes settled.
pub(super) fn disc_and_codes(disc_id: &str, codes: &[&str]) -> Signals {
    signals(disc(disc_id), BarcodeSignal::Settled { codes: artwork_codes(codes) }, &[])
}

#[test]
fn started_enters_triangulating_awaiting_signals() {
    match started_with(vec![MB, DG]) {
        IdentifyState::Triangulating { discid, barcode, catalog: _, isrc, search: _, context } => {
            assert!(matches!(discid, DiscidProgress::Computing));
            assert!(matches!(isrc, IsrcProgress::Reading));
            assert!(matches!(barcode, BarcodeProgress::Scanning));
            assert!(context.catalog.numbers.is_empty());
            assert_eq!(context.providers, vec![MB, DG]);
        }
        other => panic!("expected Triangulating, got {other:?}"),
    }
}

#[test]
fn no_disc_no_barcode_is_manual_only() {
    let (state, effects) =
        update(started(), signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &[]));
    assert!(effects.is_empty());
    match state {
        IdentifyState::ManualOnly { track_count, .. } => assert_eq!(track_count, 5),
        other => panic!("expected ManualOnly, got {other:?}"),
    }
}

/// A run with nothing to look up waits for the settled text before it settles.
#[test]
fn nothing_to_run_waits_for_the_settled_text() {
    let scanning = Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Scanning { catalogs: vec![], free_text: vec![] },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    };
    let (state, effects) = update(started(), scanning);
    assert!(effects.is_empty());
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "the run holds while the text is still scanning, got {state:?}"
    );

    let (state, effects) = update(state, signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &[]));
    assert!(effects.is_empty());
    assert!(
        matches!(state, IdentifyState::ManualOnly { track_count: 5, .. }),
        "the settled snapshot answers it, got {state:?}"
    );
}

/// An aborted extraction ends the run as bae's own error, with nothing asked
/// and the abort's detail stated.
#[test]
fn an_aborted_extraction_ends_the_run_as_an_error() {
    let failure = bae_catalogs::InternalFailure {
        detail: "fast-pass spawn_blocking failed: task panicked".to_string(),
    };
    let aborted = Signals {
        origin: bae_catalogs::AudioOrigin::default(),
        disc_id: DiscIdSignal::Failed { failure: failure.clone() },
        barcode: BarcodeSignal::Failed { failure: failure.clone(), codes: vec![] },
        text: TextSignal::Failed { failure: failure.clone(), catalogs: vec![], free_text: vec![] },
        text_pool: Vec::new(),
        isrcs: Vec::new(),
        track_titles: Vec::new(),
    };
    let (state, effects) = update(started(), aborted);
    assert!(effects.is_empty(), "nothing is asked, got {effects:?}");
    let IdentifyState::Error { failure: ended, .. } = &state else {
        panic!("the run ends as an error, got {state:?}");
    };
    assert_eq!(*ended, failure);
}

/// Reading the artwork breaking while the disc ID is being looked up ends the
/// run there: the lookup's answer lands on a run that is over.
#[test]
fn an_artwork_reading_that_breaks_ends_the_run_mid_lookup() {
    let failure = bae_catalogs::InternalFailure { detail: "OCR worker failed".to_string() };
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                discs: vec![crate::signals::ComputedDiscId {
                    disc_id: "disc-1".to_string(),
                    source_file: None,
                }],
            },
            BarcodeSignal::Scanning { codes: vec![] },
            &[],
        ),
    );
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    let (state, _) = update(
        state,
        signals(
            DiscIdSignal::Computed {
                discs: vec![crate::signals::ComputedDiscId {
                    disc_id: "disc-1".to_string(),
                    source_file: None,
                }],
            },
            BarcodeSignal::Failed { failure: failure.clone(), codes: vec![] },
            &[],
        ),
    );
    assert!(
        matches!(&state, IdentifyState::Error { failure: ended, .. } if *ended == failure),
        "got {state:?}"
    );
    let (after, effects) = step(
        state.clone(),
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "disc-1".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![])),
        },
    );
    assert_eq!(after, state, "a lookup landing after the error changes nothing");
    assert!(effects.is_empty());
}

/// bae breaking while carrying out a lookup ends the run as its error.
#[test]
fn bae_breaking_during_a_lookup_ends_the_run_as_its_error() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                discs: vec![crate::signals::ComputedDiscId {
                    disc_id: "disc-1".to_string(),
                    source_file: None,
                }],
            },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    let failure = bae_catalogs::InternalFailure {
        detail: "checking the library: the store is locked".to_string(),
    };
    let (state, effects) = step(state, IdentifyEvent::Broke { failure: failure.clone() });
    assert!(effects.is_empty());
    assert!(
        matches!(&state, IdentifyState::Error { failure: ended, .. } if *ended == failure),
        "got {state:?}"
    );
    assert!(state.is_terminal());
}

/// No barcode source offers manual search, while artwork read and holding no
/// code is a no-match.
#[test]
fn absent_barcode_offers_manual_search_where_scanned_and_empty_is_a_no_match() {
    let settle = |barcode: BarcodeSignal| {
        let (state, effects) = update(started(), signals(DiscIdSignal::Absent, barcode, &[]));
        assert!(effects.is_empty(), "no codes to look up either way");
        state
    };

    let absent = settle(BarcodeSignal::Absent);
    assert!(!absent.context().expect("a settled run keeps its context").barcode.had_source);
    match absent {
        IdentifyState::ManualOnly { track_count, .. } => assert_eq!(track_count, 5),
        other => panic!("expected ManualOnly for an absent barcode source, got {other:?}"),
    }

    let scanned = settle(BarcodeSignal::Settled { codes: Vec::new() });
    let barcode = &scanned.context().expect("a settled run keeps its context").barcode;
    assert!(barcode.had_source && barcode.codes.is_empty(), "read, and holding no code");
    assert!(
        matches!(scanned, IdentifyState::NotFoundAnywhere { .. }),
        "scanned-and-empty is a no-match, not a skip",
    );
}

#[test]
fn disc_only_resolves_to_found_by_the_disc_id() {
    let (state, _) = disc_only_started();
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
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn both_signals_intersect_to_found_combined() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![
                pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x")),
                pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-x")),
            ])),
        },
    );
    let (state, _) = step(
        state,
        barcode_matched(MB, "BAR", vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-x"))]),
    );
    match state {
        IdentifyState::Found { findings: Findings { matches, .. }, ledger, .. } => {
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].release_id, "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b");
            assert!(found_by_disc_id(ledger.as_ref(), &matches[0]));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

/// When the disc ID and barcode name different releases, the disc ID's is
/// offered and both lookups' results stay in the context.
#[test]
fn a_barcode_that_named_something_else_waits_under_the_disc_id_s_answer() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
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
    let (state, _) = step(
        state,
        barcode_matched(MB, "BAR", vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))]),
    );
    match state {
        IdentifyState::Found { context, findings: Findings { matches, .. }, ledger, .. } => {
            assert_eq!(matches.len(), 1);
            assert!(found_by_disc_id(ledger.as_ref(), &matches[0]));
            assert_eq!(context.disc.results().len(), 1);
            assert_eq!(context.barcode.results.len(), 1);
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn failed_discid_lookup_preserves_track_count() {
    let (state, _) = disc_only_started();
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Err(LookupFailure::Provider { status: Some(503) }),
        },
    );
    match state {
        IdentifyState::Failed { failures, track_count, .. } => {
            assert_eq!(track_count, 5);
            assert_eq!(
                failures,
                vec![crate::IdentifyFailure::DiscId(LookupFailure::Provider { status: Some(503) })]
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn both_lookups_empty_is_not_found_anywhere() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(vec![])),
        },
    );
    let (state, _) = step(state, barcode_missed(MB, "BAR"));
    assert!(matches!(state, IdentifyState::NotFoundAnywhere { .. }));
}

#[test]
fn cancellation_returns_to_idle() {
    let (state, effects) = step(started(), IdentifyEvent::Cancelled);
    assert!(matches!(state, IdentifyState::Idle));
    assert!(effects.is_empty());
}

/// A document stating five tracks, `notes`, and nothing else.
pub(super) fn plain_document(notes: &[&str]) -> ReleaseDocument {
    ReleaseDocument {
        labels: Vec::new(),
        barcodes: Vec::new(),
        album_first_year: None,
        source_tracks: bae_catalogs::search::SourceTracks::Listed { count: 5 },
        track_titles: Vec::new(),
        notes: notes.iter().map(|note| note.to_string()).collect(),
        links: Vec::new(),
        album_links: AlbumLinks::NotAsked,
        stored: std::sync::Arc::new(bae_catalogs::source_release::SourceRelease::for_test(
            bae_catalogs::records::LookupRef::new(
                bae_catalogs::catalog::LookupCatalog::MusicBrainz,
                "release-1",
            ),
        )),
    }
}

/// A MusicBrainz release's document naming the Discogs release `linked` as
/// itself, filed under `master`.
pub(super) fn linking_document(linked: &str, master: &str) -> ReleaseDocument {
    ReleaseDocument {
        links: vec![MetadataRef::new(DG.catalog(), linked)],
        album_links: AlbumLinks::Read(vec![MetadataRef::new(DG.catalog(), master)]),
        ..plain_document(&[])
    }
}

/// A release on `group` in `packaging`.
pub(super) fn packed_in(
    source: LookupCatalog,
    release_id: &str,
    group: &str,
    packaging: bae_types::pressing::Packaging,
) -> MetadataResult {
    MetadataResult { packaging: Some(packaging), ..mk_result_from(source, release_id, Some(group)) }
}

/// What a run asked in its rounds past the folder's own keys: each read's
/// records, and each lookup of an offered row's pressing.
#[derive(Debug, Default)]
pub(super) struct Asked {
    pub(super) reads: Vec<Vec<LookupRef>>,
    pub(super) pressings: Vec<(LookupCatalog, PressingKey)>,
}

/// Answer every read of documents and every pressing lookup `step` asks for:
/// each record with what `document` says of it, each pressing lookup with
/// what `pressing` finds. Returns where the run lands, the effects it asks
/// for past these, and what it asked.
pub(super) fn answer_rounds(
    (mut state, mut effects): (IdentifyState, Vec<Effect>),
    document: impl Fn(&LookupRef) -> ReleaseDocument,
    pressing: impl Fn(LookupCatalog, &PressingKey) -> Vec<MetadataResult>,
) -> (IdentifyState, Vec<Effect>, Asked) {
    let mut asked = Asked::default();
    while let Some(at) = effects.iter().position(|effect| {
        matches!(effect, Effect::ReadReleases { .. } | Effect::LookupPressing { .. })
    }) {
        let answer = match effects.remove(at) {
            Effect::ReadReleases { releases, .. } => {
                asked.reads.push(records_read(&releases));
                IdentifyEvent::ReleasesRead {
                    read: releases
                        .iter()
                        .map(|release| ReleaseReading {
                            release: release.release().clone(),
                            document: Ok(document(release.release())),
                        })
                        .collect(),
                }
            }
            Effect::LookupPressing { source, key } => {
                asked.pressings.push((source, key.clone()));
                let outcome = Ok(pressing(source, &key));
                IdentifyEvent::PressingLookupAnswered { source, key, outcome }
            }
            _ => unreachable!("the position is of a read or a pressing lookup"),
        };
        let (next, more) = super::step(state, answer);
        state = next;
        effects.extend(more);
    }
    (state, effects, asked)
}

/// Every release a settled state names with its row and whether a lookup of
/// the folder's disc IDs found it, offered or set aside.
pub(super) fn rows_of(state: &IdentifyState) -> Vec<(MetadataResult, bool, u32)> {
    let IdentifyState::Found { findings, ledger, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    let by_disc_id = |result: &MetadataResult| found_by_disc_id(ledger.as_ref(), result);
    findings
        .matches
        .iter()
        .zip(&findings.pressings)
        .map(|(result, row)| (result.clone(), by_disc_id(result), *row))
        .chain(
            findings
                .narrowed_out
                .matches
                .iter()
                .zip(&findings.narrowed_out.pressings)
                .map(|(result, row)| (result.clone(), by_disc_id(result), *row + 100)),
        )
        .collect()
}

/// A disc ID run on both catalogs whose lookup named `results`, holding at
/// the read of its offered records' documents.
pub(super) fn disc_run_reading(
    results: Vec<MetadataResult>,
    text_pool: Vec<crate::signals::TextLine>,
) -> (IdentifyState, Vec<Effect>) {
    let mut disc_signals = disc_only(&[]);
    disc_signals.text_pool = text_pool;
    let (state, effects) = super::step(
        started_with(vec![MB, DG]),
        IdentifyEvent::SignalsUpdated {
            signals: disc_signals,
            audio: five_tracks(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    assert!(
        matches!(effects.as_slice(), [Effect::LookupDiscid { .. }]),
        "the disc ID is looked up: {effects:?}"
    );
    super::step(
        state,
        IdentifyEvent::DiscidLookupAnswered {
            disc_id: "d".into(),
            outcome: Ok(DiscIdAnswer::for_test(results)),
        },
    )
}

/// A release of `source` by "Artist Name" printing `number` under `label`.
pub(super) fn labelled(
    source: LookupCatalog,
    release_id: &str,
    label: &str,
    number: &str,
) -> MetadataResult {
    credited(source, release_id, "Artist Name", label, number)
}

/// A run on both catalogs given a disc ID alone, its folder named `folder`.
pub(super) fn disc_run_of_folder(folder: &str, catalogs: &[&str]) -> IdentifyState {
    disc_run_of_text(&[(folder, crate::signals::TextOrigin::FolderName)], catalogs)
}

/// A run on both catalogs given a disc ID alone, its text the given lines.
pub(super) fn disc_run_of_text(
    lines: &[(&str, crate::signals::TextOrigin)],
    catalogs: &[&str],
) -> IdentifyState {
    let mut disc_signals = disc_only(catalogs);
    disc_signals.text_pool = lines
        .iter()
        .map(|(text, origin)| crate::signals::TextLine { text: text.to_string(), origin: *origin })
        .collect();
    let (state, effects) = crate::state::step(
        started_with(vec![MB, DG]),
        IdentifyEvent::SignalsUpdated {
            signals: disc_signals,
            audio: five_tracks(),
            artwork: crate::signals::ArtworkScan::Absent,
        },
    );
    assert_eq!(effects, vec![Effect::LookupDiscid { disc_id: "d".into() }]);
    state
}

/// A run that reads a title off the candidate's draft, with nothing chosen or
/// excluded.
pub(super) fn started_searching(
    providers: Vec<LookupCatalog>,
    album: &str,
    artist: &str,
) -> IdentifyState {
    let (state, effects) = step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers,
            choices: LookupChoices::default(),
            title_search: TitleSearch::of(album, artist),
        },
    );
    assert!(effects.is_empty(), "with nothing chosen, Started dispatches no effects");
    state
}

pub(super) fn search_title(source: LookupCatalog, album: &str, artist: &str) -> Effect {
    Effect::SearchTitle {
        source,
        query: TitleSearch { album: album.to_string(), artist: artist.to_string() },
    }
}

pub(super) fn search_answered(
    source: LookupCatalog,
    results: Vec<MetadataResult>,
) -> IdentifyEvent {
    IdentifyEvent::SearchAnswered { source, outcome: Ok(results) }
}

/// A folder with one barcode and no disc ID: enough for the ledger to have a
/// run to lay out, and a code the providers can miss.
pub(super) fn one_code(code: &str) -> Signals {
    signals(DiscIdSignal::Absent, BarcodeSignal::Settled { codes: artwork_codes(&[code]) }, &[])
}

/// A folder whose tags carry `isrcs`, and nothing else to look up.
pub(super) fn tagged(isrcs: &[&str]) -> Signals {
    Signals {
        isrcs: isrcs.iter().map(|code| code.to_string()).collect(),
        ..signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &[])
    }
}

pub(super) fn lookup_isrcs(isrcs: &[&str]) -> Effect {
    Effect::LookupIsrcs { isrcs: isrcs.iter().map(|code| code.to_string()).collect() }
}

/// A release of `source` credited to `artist`, printing `number` under
/// `label`.
pub(super) fn credited(
    source: LookupCatalog,
    release_id: &str,
    artist: &str,
    label: &str,
    number: &str,
) -> MetadataResult {
    MetadataResult {
        artist: Some(bae_types::credits::CreditLine::of_name(artist)),
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some(label), Some(number))],
        ..mk_result_from(source, release_id, Some("g"))
    }
}
