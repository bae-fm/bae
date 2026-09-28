// The ISRC step: every code the audio's tags carry, asked of MusicBrainz in
// one search.

use crate::identify::{IdentifyStateView, IsrcStepView, LookupView, NotAskedReason};

/// A folder whose tags carry `isrcs`, and nothing else to look up.
fn tagged(isrcs: &[&str]) -> Signals {
    Signals {
        isrcs: isrcs.iter().map(|code| code.to_string()).collect(),
        ..signals(DiscIdSignal::Absent, BarcodeSignal::Absent, &[])
    }
}

fn lookup_isrcs(isrcs: &[&str]) -> Effect {
    Effect::LookupIsrcs {
        isrcs: isrcs.iter().map(|code| code.to_string()).collect(),
    }
}

/// Each code is asked once, in one search, however many files carry it and
/// however many snapshots stream; the run waits on the answer, and what it
/// returns is found by the ISRCs.
#[test]
fn the_tags_isrcs_are_asked_once_in_one_search() {
    let codes = ["XX0000000002", "XX0000000001", "XX0000000002"];
    let (state, effects) = update(started(), tagged(&codes));
    assert_eq!(effects, vec![lookup_isrcs(&["XX0000000002", "XX0000000001"])]);
    let (state, effects) = update(state, tagged(&codes));
    assert!(effects.is_empty(), "{effects:?}");
    assert!(!state.is_terminal(), "the run waits on the ISRCs");

    let (state, _) = step(
        state,
        IdentifyEvent::IsrcLookupAnswered {
            outcome: Ok(vec![pair("rel-a", Some("g-x"))]),
        },
    );
    let IdentifyState::Found { findings, .. } = state else {
        panic!("expected Found, got {state:?}");
    };
    assert_eq!(findings.matches[0].release_id, "rel-a");
    assert!(findings.provenance[0].by_isrc);
}

/// What the ISRCs return does not stand for the album the way an identifier
/// does, so the title is still searched, on every catalog.
#[test]
fn what_the_isrcs_find_still_leaves_the_title_to_search() {
    let (state, effects) = update(
        started_searching(vec![MB, DG], "Album Title", "Artist Name"),
        tagged(&["XX0000000001"]),
    );
    assert_eq!(effects, vec![lookup_isrcs(&["XX0000000001"])]);
    let (_, effects) = step(
        state,
        IdentifyEvent::IsrcLookupAnswered {
            outcome: Ok(vec![pair("rel-a", Some("g-x"))]),
        },
    );
    assert_eq!(
        effects,
        vec![
            search_title(MB, "Album Title", "Artist Name"),
            search_title(DG, "Album Title", "Artist Name"),
        ]
    );
}

/// With MusicBrainz not asked, the codes are laid out and nobody is asked
/// about them, so the run offers a manual search.
#[test]
fn a_run_without_musicbrainz_asks_nobody_about_the_isrcs() {
    let (state, effects) = update(started_with(vec![DG]), tagged(&["XX0000000001"]));
    assert!(effects.is_empty(), "{effects:?}");
    let run = match IdentifyStateView::from(state) {
        IdentifyStateView::ManualOnly { run, .. } => run.expect("the codes are laid out"),
        other => panic!("expected ManualOnly, got {other:?}"),
    };
    assert_eq!(
        run.isrc,
        IsrcStepView::Read {
            isrcs: vec!["XX0000000001".to_string()],
            lookup: LookupView::NotAsked {
                reason: NotAskedReason::NoCatalog,
            },
        }
    );
}

/// The search failing fails the run with the ISRCs named, and the ledger says
/// so; tags with no code lay out no lookup.
#[test]
fn a_failed_isrc_search_fails_the_run() {
    let (state, _) = update(started(), tagged(&["XX0000000001"]));
    let (state, _) = step(
        state,
        IdentifyEvent::IsrcLookupAnswered {
            outcome: Err(LookupFailure::Timeout),
        },
    );
    let IdentifyState::Failed {
        failures, ledger, ..
    } = &state
    else {
        panic!("expected Failed, got {state:?}");
    };
    assert_eq!(failures, &vec![IdentifyFailure::Isrc(LookupFailure::Timeout)]);
    assert!(matches!(
        ledger.as_ref().map(|run| &run.isrc),
        Some(IsrcStepView::Read {
            lookup: LookupView::Failed {
                failure: LookupFailure::Timeout
            },
            ..
        })
    ));

    let (state, _) = update(started(), one_code("012345678905"));
    let IdentifyStateView::Triangulating { run, .. } = IdentifyStateView::from(state) else {
        panic!("expected Triangulating");
    };
    assert_eq!(run.isrc, IsrcStepView::Absent);
}
