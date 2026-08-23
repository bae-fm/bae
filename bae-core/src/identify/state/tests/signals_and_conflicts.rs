use super::*;
use crate::import::MetadataSource;
use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};

fn mk_result(release_id: &str, group_id: Option<&str>) -> MetadataResult {
    MetadataResult {
        source: MetadataSource::MusicBrainz,
        release_id: release_id.to_string(),
        title: "Album".to_string(),
        artist: None,
        year: None,
        format: None,
        label: None,
        catalog_number: None,
        country: None,
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

/// Drive `Idle` → `Triangulating` via `Started` (no effects yet — the
/// reducer waits for `SignalsUpdated`).
fn started() -> IdentifyState {
    let (state, effects) = step(IdentifyState::Idle, IdentifyEvent::Started);
    assert!(effects.is_empty(), "Started dispatches no effects");
    state
}

fn update(state: IdentifyState, signals: Signals) -> (IdentifyState, Vec<Effect>) {
    step(state, IdentifyEvent::SignalsUpdated { signals })
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
        durations: crate::import::probe::ProbedDurations::default(),
    }
}

#[test]
fn started_enters_triangulating_awaiting_signals() {
    match started() {
        IdentifyState::Triangulating {
            discid,
            barcode,
            context,
        } => {
            assert!(matches!(discid, DiscidProgress::Computing));
            assert!(matches!(barcode, BarcodeProgress::Scanning));
            assert!(context.catalogs.is_empty());
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

/// The barcode queue is seeded only once the codes settle — never from a
/// still-`Scanning` snapshot — so first-match-wins runs over a stable list.
#[test]
fn barcode_queue_seeded_only_from_settled() {
    let (state, effects) = update(
        started(),
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
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "A")));
}

#[test]
fn disc_only_resolves_to_found_with_provenance() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
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
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 10,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["BAR"]),
            },
            &[],
        ),
    );
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
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-x"))],
        },
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
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["BAR"]),
            },
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
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("e6cdc0f3-3a7b-458b-86aa-fd093cc5e79b", Some("g-y"))],
        },
    );
    match state {
        IdentifyState::Conflict { context } => {
            // The conflict retains each signal's results for the surface.
            assert_eq!(context.discid_results.len(), 1);
            assert_eq!(context.barcode_results.len(), 1);
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[test]
fn barcode_iteration_first_match_wins() {
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
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "A")));
    let (state, effects) = step(
        state,
        IdentifyEvent::BarcodeLookupMissed {
            for_barcode: "A".to_string(),
        },
    );
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "B")));
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "B".to_string(),
            results: vec![pair("e6cdc1f3-3a7b-473e-86aa-fe093cc5e94e", Some("g-x"))],
        },
    );
    match state {
        IdentifyState::Found { provenance, .. } => {
            assert!(provenance[0].by_barcode && !provenance[0].by_disc_id);
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn stale_barcode_response_is_ignored() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Absent { track_count: 0 },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMissed {
            for_barcode: "A".to_string(),
        },
    );
    // A late failed "A" arrives; current is "B", so it's dropped.
    let (state, effects) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "A".to_string(),
            failure: LookupFailure::Diagnostic {
                detail: "provider lookup failed".to_string(),
            },
        },
    );
    assert!(effects.is_empty());
    match state {
        IdentifyState::Triangulating {
            barcode: BarcodeProgress::LookingUp { current, .. },
            ..
        } => assert_eq!(current, "B"),
        other => panic!("expected LookingUp B, got {other:?}"),
    }
}

#[test]
fn barcode_lookup_failure_settles_failed() {
    let (state, effects) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 0,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["A", "B"]),
            },
            &[],
        ),
    );
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "A")));

    let failure = LookupFailure::Diagnostic {
        detail: "provider lookup failed".to_string(),
    };
    let (state, effects) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "A".to_string(),
            failure: failure.clone(),
        },
    );
    assert!(effects.is_empty());
    match &state {
        IdentifyState::Triangulating {
            barcode: BarcodeProgress::Failed { failure: actual },
            ..
        } => assert_eq!(actual, &failure),
        other => panic!("expected failed barcode progress, got {other:?}"),
    }
}

#[test]
fn failed_discid_lookup_preserves_track_count() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
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
        IdentifyState::NotFoundAnywhere { context } => assert_eq!(context.track_count, 5),
        other => panic!("expected NotFoundAnywhere, got {other:?}"),
    }
}

#[test]
fn catalog_filter_narrows_and_flags_provenance() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
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
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].release_id, "rel-a");
            assert!(provenance[0].matches_catalog);
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn artwork_catalog_does_not_narrow_or_flag_provenance() {
    let (state, _) = update(
        started(),
        signals_with_catalogs(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Absent,
            vec![SourcedValue::new(
                "LBL 002".to_string(),
                SignalOrigin::Artwork,
            )],
        ),
    );
    let mut r_a = mk_result("rel-a", Some("g-x"));
    r_a.catalog_number = Some("LBL-001".to_string());
    let mut r_b = mk_result("rel-b", Some("g-x"));
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
            assert!(provenance.iter().all(|p| !p.matches_catalog));
        }
        other => panic!("expected Found, got {other:?}"),
    }
}

#[test]
fn both_lookups_empty_is_not_found_anywhere() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["BAR"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![],
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMissed {
            for_barcode: "BAR".to_string(),
        },
    );
    assert!(matches!(state, IdentifyState::NotFoundAnywhere { .. }));
}

#[test]
fn cancellation_returns_to_idle() {
    let (state, effects) = step(started(), IdentifyEvent::Cancelled);
    assert!(matches!(state, IdentifyState::Idle));
    assert!(effects.is_empty());
}
