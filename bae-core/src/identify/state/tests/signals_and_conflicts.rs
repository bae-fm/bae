use super::*;
use crate::identify::{IdentifyFailure, LookupProvenance};
use crate::import::album_links::{AlbumLinks, GroupReading};
use crate::import::{Catalog, LookupChoices};
use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, SourcedValue, TextSignal};

fn mk_result(release_id: &str, group_id: Option<&str>) -> MetadataResult {
    mk_result_from(Catalog::MusicBrainz, release_id, group_id)
}

fn mk_result_from(source: Catalog, release_id: &str, group_id: Option<&str>) -> MetadataResult {
    MetadataResult::for_test(source, release_id, group_id)
}

fn pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
    (
        mk_result(release_id, group_id),
        LibraryStatus::absent(release_id),
    )
}

/// A Discogs result, for runs where both providers answer.
fn discogs_pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
    (
        mk_result_from(Catalog::Discogs, release_id, group_id),
        LibraryStatus::absent(release_id),
    )
}

const MB: Catalog = Catalog::MusicBrainz;
const DG: Catalog = Catalog::Discogs;

/// A run started with MusicBrainz as its only provider, waiting for signals.
fn started() -> IdentifyState {
    started_with(vec![MB])
}

/// A run started with `providers`, nothing chosen or left out.
fn started_with(providers: Vec<Catalog>) -> IdentifyState {
    let (state, effects) = started_with_choices(providers, LookupChoices::default());
    assert!(
        effects.is_empty(),
        "with nothing chosen, Started dispatches no effects"
    );
    state
}

/// A run started with `choices`, with the chosen numbers' lookups as effects.
fn started_with_choices(
    providers: Vec<Catalog>,
    choices: LookupChoices,
) -> (IdentifyState, Vec<Effect>) {
    step(
        IdentifyState::Idle,
        IdentifyEvent::Started {
            providers,
            steps: crate::config::IdentificationSteps::default(),
            choices,
            title_search: None,
        },
    )
}

/// Choices leaving out the disc ID (when `disc_id`) and the named barcodes.
fn excluding(disc_id: bool, barcodes: &[&str]) -> LookupChoices {
    LookupChoices {
        disc_id_excluded: disc_id,
        excluded_barcodes: barcodes.iter().map(|value| value.to_string()).collect(),
        chosen_catalogs: Vec::new(),
        search_words: None,
        discounted_catalogs: Vec::new(),
    }
}

/// Choices picking the named catalog numbers and leaving nothing out.
fn choosing(catalogs: &[&str]) -> LookupChoices {
    LookupChoices {
        disc_id_excluded: false,
        excluded_barcodes: Vec::new(),
        chosen_catalogs: catalogs.iter().map(|value| value.to_string()).collect(),
        search_words: None,
        discounted_catalogs: Vec::new(),
    }
}

/// The toolbar badge for one signal.
fn badge(state: &IdentifyState, kind: SignalKind) -> ToolbarSignal {
    state
        .toolbar()
        .into_iter()
        .find(|signal| signal.kind == kind)
        .unwrap_or_else(|| panic!("the toolbar carries a {kind:?} badge"))
}

/// The audio every run in these tests is over: five tracks.
fn five_tracks() -> crate::signals::AudioFacts {
    crate::signals::AudioFacts {
        track_count: 5,
        ..Default::default()
    }
}

fn update(state: IdentifyState, signals: Signals) -> (IdentifyState, Vec<Effect>) {
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
fn barcode_matched(
    source: Catalog,
    barcode: &str,
    results: Vec<(MetadataResult, LibraryStatus)>,
) -> IdentifyEvent {
    IdentifyEvent::BarcodeLookupAnswered {
        source,
        for_barcode: barcode.to_string(),
        outcome: Ok(results),
    }
}

/// One provider knew nothing about `barcode`.
fn barcode_missed(source: Catalog, barcode: &str) -> IdentifyEvent {
    barcode_matched(source, barcode, Vec::new())
}

/// One provider's lookup of `barcode` failed.
fn barcode_failed(source: Catalog, barcode: &str, failure: LookupFailure) -> IdentifyEvent {
    IdentifyEvent::BarcodeLookupAnswered {
        source,
        for_barcode: barcode.to_string(),
        outcome: Err(failure),
    }
}

fn lookup_barcode(source: Catalog, barcode: &str) -> Effect {
    Effect::LookupBarcode {
        source,
        barcode: barcode.to_string(),
    }
}

/// Barcodes read off the artwork; the reducer reads only their values.
fn artwork_codes(values: &[&str]) -> Vec<SourcedValue> {
    values
        .iter()
        .map(|v| SourcedValue::new(v.to_string()))
        .collect()
}

fn signals(disc_id: DiscIdSignal, barcode: BarcodeSignal, catalogs: &[&str]) -> Signals {
    signals_with_catalogs(
        disc_id,
        barcode,
        catalogs.iter().map(|s| s.to_string()).collect(),
    )
}

fn signals_with_catalogs(
    disc_id: DiscIdSignal,
    barcode: BarcodeSignal,
    catalogs: Vec<String>,
) -> Signals {
    Signals {
        origin: crate::signals::AudioOrigin::default(),
        disc_id,
        barcode,
        text: TextSignal::Settled {
            catalogs,
            free_text: vec![],
        },
        text_pool: Vec::new(),
        registered_in: None,
    }
}

/// A disc ID computed from no named file.
fn disc(disc_id: &str) -> DiscIdSignal {
    DiscIdSignal::Computed {
        disc_id: disc_id.to_string(),
        source_file: None,
    }
}

/// A disc ID for five tracks, no barcode source, and the named catalog numbers.
fn disc_only(catalogs: &[&str]) -> Signals {
    signals(disc("d"), BarcodeSignal::Absent, catalogs)
}

/// A MusicBrainz run given the disc ID alone.
fn disc_only_started() -> (IdentifyState, Vec<Effect>) {
    update(started(), disc_only(&[]))
}

/// A disc ID, with the given barcodes settled.
fn disc_and_codes(disc_id: &str, codes: &[&str]) -> Signals {
    signals(
        disc(disc_id),
        BarcodeSignal::Settled {
            codes: artwork_codes(codes),
        },
        &[],
    )
}

#[test]
fn started_enters_triangulating_awaiting_signals() {
    match started_with(vec![MB, DG]) {
        IdentifyState::Triangulating {
            discid,
            barcode,
            catalog: _,
            search: _,
            context,
        } => {
            assert!(matches!(discid, DiscidProgress::Computing));
            assert!(matches!(barcode, BarcodeProgress::Scanning));
            assert!(context.catalog.numbers.is_empty());
            assert_eq!(context.providers, vec![MB, DG]);
        }
        other => panic!("expected Triangulating, got {other:?}"),
    }
}

/// The disc-ID lookup is dispatched exactly once, even as snapshots stream.
#[test]
fn disc_computed_dispatches_lookup_idempotently() {
    let snapshot = || signals(disc("d"), BarcodeSignal::Scanning { codes: vec![] }, &[]);
    let (state, effects) = update(started(), snapshot());
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupDiscid { .. })));
    // A repeated snapshot must not re-dispatch the lookup.
    let (_, effects) = update(state, snapshot());
    assert!(
        effects.is_empty(),
        "disc-id lookup dispatched only once, got {effects:?}"
    );
}

#[test]
fn no_disc_no_barcode_is_manual_only() {
    let (state, effects) = update(
        started(),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            &[],
        ),
    );
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
        origin: crate::signals::AudioOrigin::default(),
        disc_id: DiscIdSignal::Absent,
        barcode: BarcodeSignal::Absent,
        text: TextSignal::Scanning {
            catalogs: vec![],
            free_text: vec![],
        },
        text_pool: Vec::new(),
        registered_in: None,
    };
    let (state, effects) = update(started(), scanning);
    assert!(effects.is_empty());
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "the run holds while the text is still scanning, got {state:?}"
    );

    let (state, effects) = update(
        state,
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Absent,
            &[],
        ),
    );
    assert!(effects.is_empty());
    assert!(
        matches!(state, IdentifyState::ManualOnly { track_count: 5, .. }),
        "the settled snapshot answers it, got {state:?}"
    );
}

/// An aborted extraction settles the run as failed, with nothing asked.
#[test]
fn an_aborted_extraction_settles_the_run_as_failed() {
    let failure = LookupFailure::Diagnostic {
        detail: "fast-pass spawn_blocking failed: task panicked".to_string(),
    };
    let aborted = Signals {
        origin: crate::signals::AudioOrigin::default(),
        disc_id: DiscIdSignal::Failed {
            failure: failure.clone(),
        },
        barcode: BarcodeSignal::Failed {
            failure: failure.clone(),
            codes: vec![],
        },
        text: TextSignal::Failed {
            failure: failure.clone(),
            catalogs: vec![],
            free_text: vec![],
        },
        text_pool: Vec::new(),
        registered_in: None,
    };
    let (state, effects) = update(started(), aborted);
    assert!(effects.is_empty(), "nothing is asked, got {effects:?}");
    let IdentifyState::Failed { failures, .. } = &state else {
        panic!("the run settles as failed, got {state:?}");
    };
    assert!(
        failures
            .iter()
            .any(|f| matches!(f, IdentifyFailure::DiscId(f) if *f == failure)),
        "the disc ID failure carries the abort's detail, got {failures:?}"
    );
    assert!(
        failures
            .iter()
            .any(|f| matches!(f, IdentifyFailure::BarcodeScan(f) if *f == failure)),
        "the barcode scan failure carries it too, got {failures:?}"
    );
}

/// No barcode source offers manual search, while artwork read and holding no
/// code is a no-match.
#[test]
fn absent_barcode_offers_manual_search_where_scanned_and_empty_is_a_no_match() {
    let settle = |barcode: BarcodeSignal| {
        let (state, effects) = update(
            started(),
            signals(DiscIdSignal::Absent, barcode, &[]),
        );
        assert!(effects.is_empty(), "no codes to look up either way");
        state
    };

    let absent = settle(BarcodeSignal::Absent);
    assert_eq!(absent.toolbar()[1].state, SignalState::Skipped);
    match absent {
        IdentifyState::ManualOnly { track_count, .. } => assert_eq!(track_count, 5),
        other => panic!("expected ManualOnly for an absent barcode source, got {other:?}"),
    }

    let scanned = settle(BarcodeSignal::Settled { codes: Vec::new() });
    assert_eq!(scanned.toolbar()[1].state, SignalState::NoMatch);
    assert!(
        matches!(scanned, IdentifyState::NotFoundAnywhere { .. }),
        "scanned-and-empty is a no-match, not a skip",
    );
}

/// Barcode lookups go out only once the codes have settled, never while
/// scanning.
#[test]
fn barcode_lookups_start_only_from_settled() {
    let (state, effects) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Scanning {
                codes: artwork_codes(&["A"]),
            },
            &[],
        ),
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { .. })),
        "no barcode lookup while still scanning"
    );
    let (_, effects) = update(
        state,
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
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

#[test]
fn disc_only_resolves_to_found_with_provenance() {
    let (state, _) = disc_only_started();
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
        },
    );
    match state {
        IdentifyState::Found {
            findings:
                Findings {
                    matches,
                    provenance,
                    ..
                },
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn both_signals_intersect_to_found_combined() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![
                pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x")),
                pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-x")),
            ],
        },
    );
    let (state, _) = step(
        state,
        barcode_matched(
            MB,
            "BAR",
            vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-x"))],
        ),
    );
    match state {
        IdentifyState::Found {
            findings:
                Findings {
                    matches,
                    provenance,
                    ..
                },
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert_eq!(
                matches[0].release_id,
                "e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b"
            );
            assert!(provenance[0].by_disc_id && provenance[0].by_barcode);
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
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
        },
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
            context,
            findings:
                Findings {
                    matches,
                    provenance,
                    ..
                },
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
            assert_eq!(context.disc.results.len(), 1);
            assert_eq!(context.barcode.results.len(), 1);
            assert_eq!(context.barcode.matched.as_deref(), Some("BAR"));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

/// Every code is asked of every provider at once, and a match on one stops
/// nothing.
#[test]
fn every_code_is_asked_of_every_provider() {
    let (state, effects) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
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
    let (state, effects) = step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-a", Some("g-x"))]),
    );
    assert!(effects.is_empty());
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, _) = step(state, barcode_missed(DG, "B"));
    // The last answer settles the lookups, and the run reads the albums' links.
    let (state, _) = step(
        state,
        barcode_matched(MB, "B", vec![pair("mb-b", Some("g-y"))]),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::AlbumLinksRead {
            read: vec![GroupReading::of_links("g-y", AlbumLinks::Read(Vec::new()))],
        },
    );
    let IdentifyState::Found {
        context,
        findings: Findings { matches, .. },
        ..
    } = state
    else {
        panic!("expected Found");
    };
    // Both codes' answers compete in the one ranking.
    let mut found: Vec<&str> = matches.iter().map(|m| m.release_id.as_str()).collect();
    found.sort_unstable();
    assert_eq!(found, vec!["dg-a", "mb-b"]);
    // The badge names the earliest code anything was found for.
    assert_eq!(context.barcode.matched.as_deref(), Some("A"));
}

/// A second answer from the same provider about the same code changes nothing.
#[test]
fn a_repeated_barcode_answer_is_ignored() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Absent,
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, effects) = step(
        state,
        barcode_failed(
            MB,
            "A",
            LookupFailure::Diagnostic {
                detail: "provider lookup failed".to_string(),
            },
        ),
    );
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

    let failure = LookupFailure::Diagnostic {
        detail: "provider lookup failed".to_string(),
    };
    let source_failure = SourceFailure {
        source: MB,
        failure: failure.clone(),
    };
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
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_failed(DG, "A", LookupFailure::Timeout));
    let (state, _) = step(state, barcode_missed(DG, "B"));
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, _) = step(
        state,
        barcode_matched(MB, "B", vec![pair("mb-b", Some("g-y"))]),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::AlbumLinksRead {
            read: vec![GroupReading::of_links("g-y", AlbumLinks::Read(Vec::new()))],
        },
    );
    match state {
        IdentifyState::Failed {
            failures,
            context,
            findings: Findings { matches, .. },
            ..
        } => {
            assert_eq!(
                failures,
                vec![crate::identify::IdentifyFailure::Barcode(SourceFailure {
                    source: DG,
                    failure: LookupFailure::Timeout,
                })]
            );
            assert_eq!(matches.len(), 1, "MusicBrainz's match still stands");
            assert_eq!(context.barcode.matched.as_deref(), Some("B"));
        }
        other => panic!("expected Failed with the surviving match, got {other:?}"),
    }
}

#[test]
fn failed_discid_lookup_preserves_track_count() {
    let (state, _) = disc_only_started();
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
        },
    );
    match state {
        IdentifyState::Failed {
            failures,
            track_count,
            ..
        } => {
            assert_eq!(track_count, 5);
            assert_eq!(
                failures,
                vec![crate::identify::IdentifyFailure::DiscId(
                    LookupFailure::Provider { status: Some(503) }
                )]
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// A catalog number nobody chose narrows nothing.
#[test]
fn an_unchosen_catalog_number_narrows_nothing() {
    let (state, _) = update(started(), disc_only(&["LBL 001"]));
    let mut r_a = mk_result("rel-a", Some("g-x"));
    r_a.labels = vec![crate::pressing::ReleaseLabel::of(None, Some("LBL-001"))];
    let mut r_b = mk_result("rel-b", Some("g-y"));
    r_b.labels = vec![crate::pressing::ReleaseLabel::of(None, Some("LBL-002"))];
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![
                (r_a, LibraryStatus::absent("rel-a")),
                (r_b, LibraryStatus::absent("rel-b")),
            ],
        },
    );
    match state {
        IdentifyState::Found {
            findings:
                Findings {
                    matches,
                    provenance,
                    ..
                },
            ..
        } => {
            assert_eq!(matches.len(), 2);
            assert!(provenance.iter().all(|p| !p.by_catalog));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn both_lookups_empty_is_not_found_anywhere() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![],
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

/// A run not asking MusicBrainz asks nobody about the disc ID, says why, and
/// with nothing else to go on offers manual search.
#[test]
fn a_run_without_the_disc_id_source_dispatches_no_disc_id_lookup() {
    let state = started_with(vec![Catalog::Discogs]);
    let (next, effects) = update(state, disc_only(&[]));

    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::LookupDiscid { .. })),
        "no source was asked about the disc ID: {effects:?}"
    );
    let context = next.context().expect("a started run carries its context");
    assert_eq!(context.providers, vec![Catalog::Discogs]);
    assert!(
        matches!(next, IdentifyState::ManualOnly { .. }),
        "nobody was asked anything, got {next:?}"
    );
    let no_catalog = crate::identify::NotAskedReason::NoCatalog;
    assert!(matches!(
        ledger_of(&next).disc_id,
        crate::identify::DiscIdStepView::Read {
            lookup: crate::identify::LookupView::NotAsked { reason },
            ..
        } if reason == no_catalog
    ));
    assert_eq!(
        badge(&next, SignalKind::DiscId).state,
        SignalState::NotAsked { reason: no_catalog }
    );
}
