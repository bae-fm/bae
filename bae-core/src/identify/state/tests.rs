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

/// Build barcode code payloads with an arbitrary `Artwork` origin — the
/// state machine only reads `.value`, so the origin is immaterial here.
fn artwork_codes(values: &[&str]) -> Vec<SourcedValue> {
    values
        .iter()
        .map(|v| SourcedValue::new(v.to_string(), SignalOrigin::Artwork))
        .collect()
}

fn signals(disc_id: DiscIdSignal, barcode: BarcodeSignal, catalogs: &[&str]) -> Signals {
    Signals {
        disc_id,
        barcode,
        text: TextSignal::Settled {
            catalogs: catalogs
                .iter()
                .map(|s| SourcedValue::new(s.to_string(), SignalOrigin::FolderName))
                .collect(),
            free_text: vec![],
        },
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
            results: vec![pair("rel-1", Some("g-x"))],
            track_count: 5,
        },
    );
    match state {
        IdentifyState::Found {
            matches,
            source,
            provenance,
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert_eq!(source, IdentifySource::Discid);
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
            results: vec![pair("rel-1", Some("g-x")), pair("rel-2", Some("g-x"))],
            track_count: 10,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("rel-2", Some("g-x"))],
        },
    );
    match state {
        IdentifyState::Found {
            matches,
            source,
            provenance,
            ..
        } => {
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].release_id, "rel-2");
            assert_eq!(source, IdentifySource::Combined);
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
            results: vec![pair("rel-1", Some("g-x"))],
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("rel-2", Some("g-y"))],
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
            results: vec![pair("rel-1", Some("g-x"))],
        },
    );
    match state {
        IdentifyState::Found { source, .. } => assert_eq!(source, IdentifySource::Barcode),
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
    // A late duplicate "A" arrives; current is "B", so it's dropped.
    let (state, effects) = step(
        state,
        IdentifyEvent::BarcodeLookupMissed {
            for_barcode: "A".to_string(),
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
fn barcode_lookup_failure_advances_like_a_miss() {
    let (state, effects) = update(
        started(),
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

    // The first code's lookup fails (a provider outage) — iteration should
    // continue to "B" exactly as a miss would, not abort the whole pipe.
    let (state, effects) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "A".to_string(),
        },
    );
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "B")));
    match &state {
        IdentifyState::Triangulating {
            barcode: BarcodeProgress::LookingUp { current, .. },
            ..
        } => assert_eq!(current, "B"),
        other => panic!("expected LookingUp B, got {other:?}"),
    }

    // The last code also fails with nothing left in the queue — the barcode
    // pipe settles as a clean miss rather than stranding the pipeline.
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupFailed {
            for_barcode: "B".to_string(),
        },
    );
    assert!(
        !matches!(state, IdentifyState::Triangulating { .. }),
        "pipeline should have settled after the last barcode failed, got {state:?}"
    );
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

/// Drive the reducer through triangulation into a conflict: disc-id and
/// barcode each match a single release, but on different groups. Returns
/// the `Conflict` state.
fn driven_conflict() -> IdentifyState {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "d".to_string(),
                track_count: 7,
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
            results: vec![pair("rel-1", Some("g-x"))],
            track_count: 7,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("rel-2", Some("g-y")), pair("rel-3", Some("g-y"))],
        },
    );
    assert!(
        matches!(state, IdentifyState::Conflict { .. }),
        "expected Conflict, got {state:?}",
    );
    state
}

/// Excluding the disc-ID signal from a conflict collapses to the barcode
/// side's single coherent group; re-including it restores the conflict.
#[test]
fn toggle_excludes_discid_then_re_includes() {
    let conflict = driven_conflict();

    let (state, effects) = step(
        conflict,
        IdentifyEvent::SignalToggled {
            signal: ExcludedSignal::Disc,
        },
    );
    assert!(
        effects.is_empty(),
        "toggle re-combines in place, no lookups"
    );
    match &state {
        IdentifyState::Found {
            matches, source, ..
        } => {
            assert_eq!(matches.len(), 2);
            assert_eq!(*source, IdentifySource::Barcode);
        }
        other => panic!("expected Found, got {other:?}"),
    }
    // The disc-ID badge reports its exclusion.
    let disc = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert!(disc.excluded);

    // Re-including the disc-ID signal restores the empty-intersection
    // conflict.
    let (restored, _) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: ExcludedSignal::Disc,
        },
    );
    assert!(
        matches!(restored, IdentifyState::Conflict { .. }),
        "expected Conflict after re-include, got {restored:?}",
    );
}

/// Toggling a signal *during* triangulation records the exclusion without
/// collapsing the in-flight lookups: the state stays `Triangulating`, the
/// badge reads excluded, and the exclusion is honored once the lookups
/// settle (disc excluded → barcode-only Found).
#[test]
fn toggle_during_triangulation_keeps_looking_up() {
    // Disc-ID computed + one barcode → both lookups dispatched, still
    // Triangulating.
    let (state, effects) = update(
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
    assert!(matches!(state, IdentifyState::Triangulating { .. }));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::LookupDiscid { .. })));

    // Toggle the disc signal off mid-lookup: must NOT collapse to a
    // terminal state, and must dispatch nothing.
    let (state, effects) = step(
        state,
        IdentifyEvent::SignalToggled {
            signal: ExcludedSignal::Disc,
        },
    );
    assert!(
        matches!(state, IdentifyState::Triangulating { .. }),
        "toggling mid-lookup must stay Triangulating, got {state:?}",
    );
    assert!(effects.is_empty(), "toggle dispatches no lookups");
    let disc = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert!(disc.excluded, "disc badge reads excluded after toggle");

    // Both lookups settle; the exclusion is honored → barcode-only Found.
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-1", Some("g-x"))],
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("rel-2", Some("g-y"))],
        },
    );
    match state {
        IdentifyState::Found {
            source, matches, ..
        } => {
            assert_eq!(source, IdentifySource::Barcode);
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].release_id, "rel-2");
        }
        other => panic!("expected barcode-only Found, got {other:?}"),
    }
}

/// `ReRun` from a settled state resets to `Triangulating` and re-dispatches
/// both lookups from the retained signals.
#[test]
fn rerun_re_dispatches_lookups() {
    // A plain disc-only Found, then re-run.
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
            results: vec![pair("rel-1", Some("g-x"))],
            track_count: 5,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("rel-1", Some("g-x"))],
        },
    );
    assert!(matches!(state, IdentifyState::Found { .. }));

    let (rerun_state, effects) = step(state, IdentifyEvent::ReRun);
    assert!(
        matches!(rerun_state, IdentifyState::Triangulating { .. }),
        "expected Triangulating after re-run, got {rerun_state:?}",
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::LookupDiscid { disc_id, .. } if disc_id == "d")),
        "re-run re-dispatches the disc-ID lookup, got {effects:?}",
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::LookupBarcode { barcode } if barcode == "BAR")),
        "re-run re-dispatches the barcode lookup, got {effects:?}",
    );
}

/// `ReRun` from a skip-only candidate (disc Absent, no barcodes) dispatches
/// no lookups, so rerun must settle immediately via settle_if_ready rather
/// than park in `Triangulating` waiting for results that never arrive.
#[test]
fn rerun_with_nothing_to_look_up_settles() {
    let (settled, effects) = update(
        started(),
        signals(
            DiscIdSignal::Absent { track_count: 5 },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    assert!(effects.is_empty(), "skip-only signals dispatch no lookups");
    assert!(
        !matches!(settled, IdentifyState::Triangulating { .. }),
        "skip-only signals settle on first snapshot, got {settled:?}",
    );
    let settled_kind = std::mem::discriminant(&settled);

    let (rerun_state, rerun_effects) = step(settled, IdentifyEvent::ReRun);
    assert!(
        rerun_effects.is_empty(),
        "re-run of a skip-only candidate dispatches no lookups, got {rerun_effects:?}",
    );
    assert!(
        !matches!(rerun_state, IdentifyState::Triangulating { .. }),
        "re-run must settle, not park in Triangulating, got {rerun_state:?}",
    );
    assert_eq!(
        std::mem::discriminant(&rerun_state),
        settled_kind,
        "re-run re-settles to the same terminal state",
    );
}

/// `ReRun` preserves the user's exclusions: a re-run after excluding the
/// disc still does not re-include it (no disc lookup dispatched once the
/// re-derive masks it). Here we exclude the barcode, re-run, and confirm
/// the barcode lookup is still dispatched (re-run replays the lookups
/// regardless) but the settled re-derive masks the excluded side.
#[test]
fn rerun_preserves_exclusions() {
    let conflict = driven_conflict();
    // Exclude the disc-ID signal: lands on barcode-only Found.
    let (excluded, _) = step(
        conflict,
        IdentifyEvent::SignalToggled {
            signal: ExcludedSignal::Disc,
        },
    );
    assert!(matches!(excluded, IdentifyState::Found { .. }));

    // Re-run, then re-settle both lookups. The disc exclusion survives,
    // so the re-derived state is barcode-only Found again, not a conflict.
    let (state, _) = step(excluded, IdentifyEvent::ReRun);
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupCompleted {
            results: vec![pair("rel-1", Some("g-x"))],
            track_count: 7,
        },
    );
    let (state, _) = step(
        state,
        IdentifyEvent::BarcodeLookupMatched {
            for_barcode: "BAR".to_string(),
            results: vec![pair("rel-2", Some("g-y")), pair("rel-3", Some("g-y"))],
        },
    );
    match state {
        IdentifyState::Found { source, .. } => assert_eq!(source, IdentifySource::Barcode),
        other => panic!("expected barcode-only Found after re-run, got {other:?}"),
    }
}

// MARK: - Toolbar projection

#[test]
fn toolbar_while_triangulating_shows_spinners() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &["LBL-001"],
        ),
    );
    let toolbar = state.toolbar();
    // disc, barcode, one catalog.
    assert_eq!(toolbar.len(), 3);

    let disc = &toolbar[0];
    assert_eq!(disc.kind, SignalKind::DiscId);
    assert_eq!(disc.role, SignalRole::Identity);
    assert_eq!(disc.value.as_deref(), Some("disc-hash"));
    assert_eq!(disc.origin, SignalOrigin::DiscToc);
    assert_eq!(disc.state, SignalState::LookingUp);

    let barcode = &toolbar[1];
    assert_eq!(barcode.kind, SignalKind::Barcode);
    assert_eq!(barcode.value.as_deref(), Some("012345678905"));
    assert_eq!(barcode.state, SignalState::LookingUp);

    let catalog = &toolbar[2];
    assert_eq!(catalog.kind, SignalKind::Catalog);
    assert_eq!(catalog.role, SignalRole::Filter);
    assert_eq!(catalog.value.as_deref(), Some("LBL-001"));
}

#[test]
fn toolbar_found_reports_counts_and_catalog_confirms() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Absent,
            &["LBL 001", "LBL 999"],
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
    assert!(matches!(state, IdentifyState::Found { .. }));

    let toolbar = state.toolbar();
    let disc = toolbar
        .iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert_eq!(disc.state, SignalState::Found { count: 2 });

    let barcode = toolbar
        .iter()
        .find(|s| s.kind == SignalKind::Barcode)
        .expect("barcode badge");
    assert_eq!(barcode.state, SignalState::Skipped);
    assert_eq!(barcode.value, None);

    let catalogs: Vec<&ToolbarSignal> = toolbar
        .iter()
        .filter(|s| s.kind == SignalKind::Catalog)
        .collect();
    assert_eq!(catalogs.len(), 2);
    // `LBL 001` confirms rel-a (catno LBL-001 normalizes equal).
    let confirming = catalogs
        .iter()
        .find(|c| c.value.as_deref() == Some("LBL 001"))
        .expect("confirming catalog");
    assert_eq!(confirming.state, SignalState::Confirms { count: 1 });
    // `LBL 999` confirms nothing.
    let noise = catalogs
        .iter()
        .find(|c| c.value.as_deref() == Some("LBL 999"))
        .expect("noise catalog");
    assert_eq!(noise.state, SignalState::Confirms { count: 0 });
}

#[test]
fn toolbar_shows_failed_disc_id_lookup() {
    // The disc-id lookup fails while the barcode is still in flight, so the
    // pipeline stays in Triangulating and the disc badge reports the
    // failure rather than a spinner or a count.
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Computed {
                disc_id: "disc-hash".to_string(),
                track_count: 5,
            },
            BarcodeSignal::Settled {
                codes: artwork_codes(&["012345678905"]),
            },
            &[],
        ),
    );
    let (state, _) = step(
        state,
        IdentifyEvent::DiscidLookupFailed {
            failure: LookupFailure::Provider { status: Some(503) },
        },
    );
    let disc = state
        .toolbar()
        .into_iter()
        .find(|s| s.kind == SignalKind::DiscId)
        .expect("disc badge");
    assert_eq!(
        disc.state,
        SignalState::Failed {
            failure: LookupFailure::Provider { status: Some(503) }
        }
    );
}

#[test]
fn toolbar_skipped_disc_and_barcode_in_manual_only() {
    let (state, _) = update(
        started(),
        signals(
            DiscIdSignal::Absent { track_count: 7 },
            BarcodeSignal::Absent,
            &[],
        ),
    );
    assert!(matches!(state, IdentifyState::ManualOnly { .. }));
    let toolbar = state.toolbar();
    let disc = &toolbar[0];
    assert_eq!(disc.state, SignalState::Skipped);
    assert_eq!(disc.value, None);
    let barcode = &toolbar[1];
    assert_eq!(barcode.state, SignalState::Skipped);
}

#[test]
fn idle_has_empty_toolbar() {
    assert!(IdentifyState::Idle.toolbar().is_empty());
}
