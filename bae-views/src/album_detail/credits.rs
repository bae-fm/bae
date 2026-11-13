//! The album's Credits: who did what on a release and on which of its tracks.
//!
//! A release's credits are its own and its tracks'; the Credits list them all,
//! every role. For each track they include the non-writing credits of the
//! works it performs, as that track's: the writing credits of works are the
//! works' (their composers' names), not the release's.

use bae_records::{DbCreditScope, DbReleaseCredit};
use bae_types::credit_role::CreditedAs;
use bae_types::credits::CreditLine;
use std::collections::BTreeSet;

/// One row of a release's Credits: a role, the artists who share it over the
/// same tracks, and those tracks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditRow {
    pub role: CreditedAs,
    pub artists: CreditLine,
    /// The position text of each track the row covers, in playing order;
    /// empty for a row covering every track of the release.
    pub tracks: Vec<String>,
}

/// A role and an artist, with the tracks the artist holds it on.
struct Held {
    role: CreditedAs,
    artist_id: String,
    printed: String,
    tracks: BTreeSet<usize>,
}

/// The rows of the Credits `credits` make over the release's tracks, whose
/// ids are `track_ids` and position texts `positions`, in playing order.
/// Credits group by role (with its qualifiers) and artist, each with the
/// tracks it covers; artists sharing a role and the same tracks are one row.
/// Rows go in the order of their first credit.
pub(crate) fn credit_rows(
    credits: &[DbReleaseCredit],
    track_ids: &[&str],
    positions: &[String],
) -> Vec<CreditRow> {
    let mut held: Vec<Held> = Vec::new();
    for credit in credits {
        let tracks: Vec<usize> = match &credit.scope {
            DbCreditScope::Release => (0..positions.len()).collect(),
            DbCreditScope::Track(id) => vec![track_ids
                .iter()
                .position(|track| track == id)
                .expect("a credit's track is one of the release's")],
        };
        match held
            .iter_mut()
            .find(|held| held.role == credit.role && held.artist_id == credit.artist_id)
        {
            Some(held) => held.tracks.extend(tracks),
            None => held.push(Held {
                role: credit.role.clone(),
                artist_id: credit.artist_id.clone(),
                printed: credit.printed.clone(),
                tracks: tracks.into_iter().collect(),
            }),
        }
    }
    let mut rows: Vec<(CreditedAs, BTreeSet<usize>, Vec<String>)> = Vec::new();
    for held in held {
        match rows.iter_mut().find(|(role, tracks, _)| *role == held.role && *tracks == held.tracks)
        {
            Some((_, _, names)) => names.push(held.printed),
            None => rows.push((held.role, held.tracks, vec![held.printed])),
        }
    }
    rows.into_iter()
        .map(|(role, tracks, names)| CreditRow {
            role,
            artists: CreditLine::from_stored(names.iter().map(|name| (name.as_str(), None))),
            tracks: if tracks.len() == positions.len() {
                Vec::new()
            } else {
                tracks.into_iter().map(|index| positions[index].clone()).collect()
            },
        })
        .collect()
}

#[cfg(test)]
#[path = "credits_tests.rs"]
mod tests;
