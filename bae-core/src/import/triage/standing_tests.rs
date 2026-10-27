use super::*;
use std::collections::HashSet;

/// One standing of every state, a row waiting on the person for each reason
/// the lookup can leave it to them.
fn every_standing() -> Vec<PendingStanding> {
    vec![
        PendingStanding::NotLookedUp,
        PendingStanding::Identifying,
        PendingStanding::NeedsYou {
            reason: NeedsYouReason::Matches { count: 3 },
        },
        PendingStanding::NeedsYou {
            reason: NeedsYouReason::NoTracklist,
        },
        PendingStanding::NeedsYou {
            reason: NeedsYouReason::MediumMismatch {
                folder: MediumConflict::NotCdAudio,
                releases: 2,
            },
        },
        PendingStanding::NeedsYou {
            reason: NeedsYouReason::NotFound,
        },
        PendingStanding::NeedsYou {
            reason: NeedsYouReason::NothingToLookUp,
        },
        PendingStanding::Identified,
        PendingStanding::Unmatched,
        PendingStanding::LookupError,
        PendingStanding::Error {
            failure: InternalFailure {
                detail: "the disk is full".to_string(),
            },
        },
        PendingStanding::Importing,
        PendingStanding::ImportError,
    ]
}

/// Every row the Needs You filter holds wears exactly one badge, saying why
/// it waits on the person, and every other row wears none: a failure in the
/// error tone, an answer left to the person in the tone that asks for one.
#[test]
fn every_row_waiting_on_the_person_wears_one_badge() {
    let standings = every_standing();
    let states: HashSet<PendingState> = standings.iter().map(PendingStanding::state).collect();
    // Each of `PendingState`'s nine.
    assert_eq!(states.len(), 9, "{states:?}");
    let mut badges = Vec::new();
    for standing in &standings {
        let state = standing.state();
        // The states Found's Needs You entry holds.
        let waits = matches!(
            state,
            PendingState::NeedsYou
                | PendingState::LookupError
                | PendingState::Error
                | PendingState::ImportError
        );
        match standing.badge() {
            Some(badge) => {
                assert!(waits, "{standing:?}");
                let tone = match state {
                    PendingState::NeedsYou => BadgeTone::Attention,
                    _ => BadgeTone::Failure,
                };
                assert_eq!(badge.tone(), tone, "{standing:?}");
                assert!(!badges.contains(&badge), "{badge:?} twice");
                badges.push(badge);
            }
            None => assert!(!waits, "{standing:?}"),
        }
    }
}

/// A lookup error and bae's own error wear the failure tone.
#[test]
fn a_failure_wears_the_failure_tone() {
    for (standing, badge) in [
        (PendingStanding::LookupError, PendingBadge::LookupError),
        (
            PendingStanding::Error {
                failure: InternalFailure {
                    detail: "disk full".to_string(),
                },
            },
            PendingBadge::Error,
        ),
        (PendingStanding::ImportError, PendingBadge::ImportError),
    ] {
        assert_eq!(standing.badge(), Some(badge));
        assert_eq!(badge.tone(), BadgeTone::Failure);
    }
}
