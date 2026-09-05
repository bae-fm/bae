use super::*;
use crate::import::MetadataSource;
use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, SourcedValue, TextSignal};

fn mk_result(release_id: &str, group_id: Option<&str>) -> MetadataResult {
    mk_result_from(MetadataSource::MusicBrainz, release_id, group_id)
}

fn mk_result_from(
    source: MetadataSource,
    release_id: &str,
    group_id: Option<&str>,
) -> MetadataResult {
    MetadataResult {
        source,
        release_id: release_id.to_string(),
        title: "Album".to_string(),
        artist: None,
        year: None,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
        barcode: None,
        cover_art: None,
        source_group_id: group_id.map(str::to_string),
        source_tracks: None,
    }
}

fn mk_status(release_id: &str) -> LibraryStatus {
    LibraryStatus {
        release_id: release_id.to_string(),
        release_in_library: false,
        album_in_library: false,
        album_title: None,
        album_id: None,
    }
}

fn pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
    (mk_result(release_id, group_id), mk_status(release_id))
}

/// A Discogs result, for runs where both providers answer.
fn discogs_pair(release_id: &str, group_id: Option<&str>) -> (MetadataResult, LibraryStatus) {
    (
        mk_result_from(MetadataSource::Discogs, release_id, group_id),
        mk_status(release_id),
    )
}

const MB: MetadataSource = MetadataSource::MusicBrainz;
const DG: MetadataSource = MetadataSource::Discogs;

/// Drive `Idle` → `Triangulating` via `Started` with MusicBrainz as the only
/// provider (no effects yet — the reducer waits for `SignalsUpdated`).
fn started() -> IdentifyState {
    started_with(vec![MB])
}

/// `Started` with the given providers in the run.
fn started_with(providers: Vec<MetadataSource>) -> IdentifyState {
    let (state, effects) = step(IdentifyState::Idle, IdentifyEvent::Started { providers });
    assert!(effects.is_empty(), "Started dispatches no effects");
    state
}

fn update(state: IdentifyState, signals: Signals) -> (IdentifyState, Vec<Effect>) {
    step(state, IdentifyEvent::SignalsUpdated { signals })
}

/// One provider matched `barcode`.
fn barcode_matched(
    source: MetadataSource,
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
fn barcode_missed(source: MetadataSource, barcode: &str) -> IdentifyEvent {
    barcode_matched(source, barcode, Vec::new())
}

/// One provider's lookup of `barcode` failed.
fn barcode_failed(source: MetadataSource, barcode: &str, failure: LookupFailure) -> IdentifyEvent {
    IdentifyEvent::BarcodeLookupAnswered {
        source,
        for_barcode: barcode.to_string(),
        outcome: Err(failure),
    }
}

fn lookup_barcode(source: MetadataSource, barcode: &str) -> Effect {
    Effect::LookupBarcode {
        source,
        barcode: barcode.to_string(),
    }
}

/// Barcode codes with an arbitrary `Artwork` origin — the state machine reads only
/// `.value` from them, so the origin doesn't matter here.
fn artwork_codes(values: &[&str]) -> Vec<SourcedValue> {
    values
        .iter()
        .map(|v| SourcedValue::new(v.to_string(), SignalOrigin::Artwork))
        .collect()
}

fn signals(disc_id: DiscIdSignal, barcode: BarcodeSignal, catalogs: &[&str]) -> Signals {
    signals_with_catalogs(
        disc_id,
        barcode,
        catalogs
            .iter()
            .map(|s| SourcedValue::new(s.to_string(), SignalOrigin::FolderName))
            .collect(),
    )
}

fn signals_with_catalogs(
    disc_id: DiscIdSignal,
    barcode: BarcodeSignal,
    catalogs: Vec<SourcedValue>,
) -> Signals {
    Signals {
        disc_id,
        barcode,
        text: TextSignal::Settled {
            catalogs,
            free_text: vec![],
        },
        durations: crate::import::probe::SourceDurations::default(),
    }
}

/// The disc ID computed, with the given barcode codes settled — both providers'
/// walks start on the first code.
fn disc_and_codes(disc_id: &str, codes: &[&str]) -> Signals {
    signals(
        DiscIdSignal::Computed {
            disc_id: disc_id.to_string(),
            track_count: 5,
            source_file: None,
        },
        BarcodeSignal::Settled {
            codes: artwork_codes(codes),
        },
        &[],
    )
}

/// The barcode pipe's per-provider walks, for asserting where each is.
fn barcode_walks(state: &IdentifyState) -> &[ProviderBarcodeLookup] {
    match state {
        IdentifyState::Triangulating {
            barcode: BarcodeProgress::Lookups { providers, .. },
            ..
        } => providers,
        other => panic!("expected barcode walks in flight, got {other:?}"),
    }
}

fn walk_of(state: &IdentifyState, source: MetadataSource) -> &BarcodeLookupState {
    &barcode_walks(state)
        .iter()
        .find(|p| p.source == source)
        .expect("provider in the run")
        .state
}

#[test]
fn started_enters_triangulating_awaiting_signals() {
    match started_with(vec![MB, DG]) {
        IdentifyState::Triangulating {
            discid,
            barcode,
            catalog: _,
            context,
        } => {
            assert!(matches!(discid, DiscidProgress::Computing));
            assert!(matches!(barcode, BarcodeProgress::Scanning));
            assert!(context.catalogs.is_empty());
            assert_eq!(context.providers, vec![MB, DG]);
        }
        other => panic!("expected Triangulating, got {other:?}"),
    }
}

/// The disc-ID lookup is dispatched exactly once, even as snapshots stream.
#[test]
fn disc_computed_dispatches_lookup_idempotently() {
    let snapshot = || {
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Scanning { codes: vec![] },
            &[],
        )
    };
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
            DiscIdSignal::Absent { track_count: 7 },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    assert!(effects.is_empty());
    match state {
        IdentifyState::ManualOnly { track_count, .. } => assert_eq!(track_count, 7),
        other => panic!("expected ManualOnly, got {other:?}"),
    }
}

/// The two empty-code barcode signals mean opposite things and must settle
/// differently. `Absent` (no barcode source at all — no CUE catalog, and no
/// artwork *or* no analyzer to read it with) leaves the pipe `Skipped` and, with
/// no disc ID either, offers manual search. `Settled { codes: [] }` means the
/// artwork was decoded and held no barcode — a real no-match, which combines to
/// `NotFoundAnywhere`.
///
/// A platform with no artwork analyzer has no barcode source, so it must produce
/// the first, not the second: claiming "we read the cover and it holds no
/// barcode" of a cover nothing decoded sends the user to a dead end instead of
/// the search box.
#[test]
fn absent_barcode_offers_manual_search_where_scanned_and_empty_is_a_no_match() {
    let settle = |barcode: BarcodeSignal| {
        let (state, effects) = update(
            started(),
            signals(DiscIdSignal::Absent { track_count: 9 }, barcode, &[]),
        );
        assert!(effects.is_empty(), "no codes to look up either way");
        state
    };

    let absent = settle(BarcodeSignal::Absent);
    assert_eq!(absent.toolbar()[1].state, SignalState::Skipped);
    match absent {
        IdentifyState::ManualOnly { track_count, .. } => assert_eq!(track_count, 9),
        other => panic!("expected ManualOnly for an absent barcode source, got {other:?}"),
    }

    let scanned = settle(BarcodeSignal::Settled { codes: Vec::new() });
    assert_eq!(scanned.toolbar()[1].state, SignalState::NoMatch);
    assert!(
        matches!(scanned, IdentifyState::NotFoundAnywhere { .. }),
        "scanned-and-empty is a no-match, not a skip",
    );
}

/// The barcode walks start only once the codes settle — never from a
/// still-`Scanning` snapshot — so every provider walks a stable list.
#[test]
fn barcode_walks_start_only_from_settled() {
    let (state, effects) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
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
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    // Every provider is asked about the first code, and nothing else yet.
    assert_eq!(effects, vec![lookup_barcode(MB, "A"), lookup_barcode(DG, "A")]);
}

#[test]
fn disc_only_resolves_to_found_with_provenance() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 5,
        },
    );
    match state {
        IdentifyState::Found {
            matches,
            provenance,
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
            track_count: 10,
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
            matches,
            provenance,
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

#[test]
fn empty_intersection_is_conflict() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
            track_count: 5,
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
            matches,
            provenance,
            context,
            ..
        } => {
            // Neither signal is wrong about having seen a release, so both are
            // offered — and each signal's own results stay in the context, so
            // toggling one off re-combines over the rest.
            assert_eq!(matches.len(), 2);
            assert!(provenance[0].by_disc_id && !provenance[0].by_barcode);
            assert!(!provenance[1].by_disc_id && provenance[1].by_barcode);
            assert_eq!(context.discid_results.len(), 1);
            assert_eq!(context.barcode_results.len(), 1);
            assert_eq!(context.matched_barcode.as_deref(), Some("BAR"));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

/// A provider walks the codes in order and stops at its first match.
#[test]
fn a_provider_s_walk_stops_at_its_first_match() {
    let (state, effects) = update(
        started(),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B", "C"]),
            },
            &[],
        ),
    );
    assert_eq!(effects, vec![lookup_barcode(MB, "A")]);
    let (state, effects) = step(state, barcode_missed(MB, "A"));
    assert_eq!(effects, vec![lookup_barcode(MB, "B")]);
    let (state, effects) = step(
        state,
        barcode_matched(
            MB,
            "B",
            vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
        ),
    );
    assert!(effects.is_empty(), "a match ends the walk; C is never asked");
    match state {
        IdentifyState::Found {
            provenance,
            context,
            ..
        } => {
            assert!(provenance[0].by_barcode && !provenance[0].by_disc_id);
            assert_eq!(context.matched_barcode.as_deref(), Some("B"));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

/// Each provider walks the codes on its own: Discogs matching the first code
/// does not stop MusicBrainz trying the second, and MusicBrainz still being
/// out does not hold Discogs's answer back from the state.
#[test]
fn each_provider_walks_the_codes_on_its_own() {
    let (state, effects) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    assert_eq!(effects, vec![lookup_barcode(MB, "A"), lookup_barcode(DG, "A")]);

    // Discogs answers first, with a match. Its answer lands at once; the run
    // stays open for MusicBrainz.
    let (state, effects) = step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-1", Some("g-x"))]),
    );
    assert!(effects.is_empty());
    assert!(matches!(
        walk_of(&state, DG),
        BarcodeLookupState::Matched { code: Some(code), .. } if code == "A"
    ));
    assert!(matches!(
        walk_of(&state, MB),
        BarcodeLookupState::Trying { index: 0 }
    ));

    // MusicBrainz misses A and moves on to B, on its own.
    let (state, effects) = step(state, barcode_missed(MB, "A"));
    assert_eq!(effects, vec![lookup_barcode(MB, "B")]);
    assert!(matches!(
        walk_of(&state, MB),
        BarcodeLookupState::Trying { index: 1 }
    ));

    // MusicBrainz misses B too: its walk is exhausted, and the pipe settles on
    // what Discogs found.
    let (state, effects) = step(state, barcode_missed(MB, "B"));
    assert!(effects.is_empty());
    match state {
        IdentifyState::Found {
            matches,
            provenance,
            context,
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].source, DG);
            assert!(provenance[0].by_barcode);
            assert_eq!(context.matched_barcode.as_deref(), Some("A"));
            assert!(context.barcode_failures.is_empty());
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

/// The matched code is the earliest in the list any provider matched, even
/// when the providers matched different codes.
#[test]
fn the_matched_code_is_the_earliest_any_provider_matched() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, _) = step(
        state,
        barcode_matched(MB, "B", vec![pair("mb-b", Some("g-y"))]),
    );
    let (state, _) = step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-a", Some("g-x"))]),
    );
    let IdentifyState::Found {
        matches, context, ..
    } = state
    else {
        panic!("expected Found");
    };
    // MusicBrainz's results come first, whichever provider answered first.
    assert_eq!(
        matches.iter().map(|m| m.source).collect::<Vec<_>>(),
        vec![MB, DG]
    );
    assert_eq!(context.matched_barcode.as_deref(), Some("A"));
}

/// An answer for a code the provider's walk has already moved past is stale
/// and dropped; the other provider's walk is never touched by it.
#[test]
fn stale_barcode_response_is_ignored() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_missed(MB, "A"));
    // A late failed "A" from MusicBrainz arrives; its walk is on "B", so it's
    // dropped.
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
    assert!(matches!(
        walk_of(&state, MB),
        BarcodeLookupState::Trying { index: 1 }
    ));
    assert!(matches!(
        walk_of(&state, DG),
        BarcodeLookupState::Trying { index: 0 }
    ));
}

/// A provider failing settles its own walk as failed; the pipe as a whole
/// settles on that when it is the only provider.
#[test]
fn barcode_lookup_failure_settles_failed() {
    let (state, effects) = update(started(), disc_and_codes("d", &["A", "B"]));
    assert!(effects.contains(&lookup_barcode(MB, "A")));

    let failure = LookupFailure::Diagnostic {
        detail: "provider lookup failed".to_string(),
    };
    let source_failure = SourceFailure {
        source: MB,
        failure: failure.clone(),
    };
    let (state, effects) = step(state, barcode_failed(MB, "A", failure));
    assert!(effects.is_empty(), "a failed walk does not move on to B");
    match &state {
        IdentifyState::Triangulating { barcode, .. } => {
            assert!(barcode.is_settled());
            assert_eq!(barcode.failures(), vec![source_failure]);
            assert!(barcode.results().is_empty());
        }
        other => panic!("expected the barcode pipe settled failed, got {other:?}"),
    }
}

/// One provider failing does not stop the other's walk, and what the other
/// finds is offered beside the failure rather than hidden by it.
#[test]
fn a_failed_provider_does_not_stop_the_other_s_walk() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, effects) = step(state, barcode_failed(DG, "A", LookupFailure::Timeout));
    assert!(effects.is_empty());
    assert!(matches!(
        state,
        IdentifyState::Triangulating { .. }
    ));

    let (state, effects) = step(state, barcode_missed(MB, "A"));
    assert_eq!(effects, vec![lookup_barcode(MB, "B")]);
    let (state, _) = step(
        state,
        barcode_matched(MB, "B", vec![pair("mb-b", Some("g-y"))]),
    );
    match state {
        IdentifyState::Failed {
            failures,
            matches,
            context,
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
            assert_eq!(context.matched_barcode.as_deref(), Some("B"));
        }
        other => panic!("expected Failed with the surviving match, got {other:?}"),
    }
}

/// Retry re-asks exactly the provider that failed, from its first code, and
/// keeps what the other provider found.
#[test]
fn retry_re_asks_only_the_failed_provider() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_failed(DG, "A", LookupFailure::Timeout));
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, _) = step(
        state,
        barcode_matched(MB, "B", vec![pair("mb-b", Some("g-y"))]),
    );
    assert!(matches!(state, IdentifyState::Failed { .. }));

    let (state, effects) = step(state, IdentifyEvent::RetryFailed);
    assert_eq!(effects, vec![lookup_barcode(DG, "A")]);
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    // MusicBrainz's answer stood back up from the context: kept, with the
    // code it matched no longer known.
    assert!(matches!(
        walk_of(&state, MB),
        BarcodeLookupState::Matched { code: None, results } if results.len() == 1
    ));
    assert!(matches!(
        walk_of(&state, DG),
        BarcodeLookupState::Trying { index: 0 }
    ));

    let (state, _) = step(
        state,
        barcode_matched(DG, "A", vec![discogs_pair("dg-a", Some("g-x"))]),
    );
    match state {
        IdentifyState::Found {
            matches, context, ..
        } => {
            assert_eq!(matches.len(), 2);
            assert!(context.barcode_failures.is_empty());
            // A matched earlier than B, so the retried provider's code wins.
            assert_eq!(context.matched_barcode.as_deref(), Some("A"));
        }
        other => panic!("expected Found after the retry, got {other:?}"),
    }
}

/// Mid-run, a retry restarts the failed provider's walk in place while the
/// other provider's walk carries on untouched.
#[test]
fn retry_mid_run_restarts_only_the_failed_walk() {
    let (state, _) = update(
        started_with(vec![MB, DG]),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, _) = step(state, barcode_missed(MB, "A"));
    let (state, _) = step(state, barcode_failed(DG, "A", LookupFailure::Network));

    let (state, effects) = step(state, IdentifyEvent::RetryFailed);
    assert_eq!(effects, vec![lookup_barcode(DG, "A")]);
    assert!(matches!(
        walk_of(&state, MB),
        BarcodeLookupState::Trying { index: 1 }
    ));
    assert!(matches!(
        walk_of(&state, DG),
        BarcodeLookupState::Trying { index: 0 }
    ));
}

/// A retry with nothing failed asks nothing and leaves the answer as it is.
#[test]
fn retry_with_nothing_failed_changes_nothing() {
    let (state, _) = update(started(), disc_and_codes("d", &["BAR"]));
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-a", Some("g-x"))],
            track_count: 5,
        },
    );
    let (found, _) = step(
        state,
        barcode_matched(MB, "BAR", vec![pair("rel-a", Some("g-x"))]),
    );
    assert!(matches!(found, IdentifyState::Found { .. }));

    let (state, effects) = step(found.clone(), IdentifyEvent::RetryFailed);
    assert!(effects.is_empty());
    assert_eq!(state, found);
}

/// A failed disc-ID lookup is retried too, when there is a disc ID to ask
/// about.
#[test]
fn retry_re_asks_a_failed_disc_id_lookup() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Network,
            track_count: 5,
        },
    );
    assert!(matches!(state, IdentifyState::Failed { .. }));

    let (state, effects) = step(state, IdentifyEvent::RetryFailed);
    assert_eq!(
        effects,
        vec![Effect::LookupDiscid {
            disc_id: "d".to_string(),
            track_count: 5,
        }]
    );
    assert!(matches!(
        state,
        IdentifyState::Triangulating {
            discid: DiscidProgress::LookingUp,
            ..
        }
    ));
}

#[test]
fn failed_discid_lookup_preserves_track_count() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
            track_count: 5,
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

/// An extracted catalog number nobody checked narrows nothing: it is an option
/// on the catalog badge, not a filter the run applies on its own.
#[test]
fn an_unchosen_catalog_number_narrows_nothing() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
                source_file: None,
            },
            BarcodeSignal::Absent,
            &["LBL 001"],
        ),
    );
    let mut r_a = mk_result("rel-a", Some("g-x"));
    r_a.catalog_number = Some("LBL-001".to_string());
    let mut r_b = mk_result("rel-b", Some("g-y"));
    r_b.catalog_number = Some("LBL-002".to_string());
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![(r_a, mk_status("rel-a")), (r_b, mk_status("rel-b"))],
            track_count: 5,
        },
    );
    match state {
        IdentifyState::Found {
            matches,
            provenance,
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
            track_count: 5,
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
