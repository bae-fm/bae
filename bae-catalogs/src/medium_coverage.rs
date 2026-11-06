//! Which of a release's mediums a candidate's audio is a rip of.
//!
//! A catalog describes a whole pressing: every disc of a box, both layers of
//! a hybrid SACD. A folder holds some of that — one disc, the CD layer, three
//! of six — and the tracklist it is read against has to be those mediums'
//! tracks alone. The candidates are the sets of mediums whose tracks, laid
//! out some way, number as many as the folder's; a reading lays each out
//! against the audio and keeps the ones that fit it exactly
//! (see `SourceRelease::read`). Among those, `Fit` orders what decides.

use crate::AudioOrigin;
use bae_types::pressing::{CdAudio, Medium};
use std::cmp::Reverse;
use std::collections::BTreeSet;

/// How far a row's two lengths may differ before the row says so.
///
/// A source that rounds its track lengths to whole seconds is out by up to one;
/// lossy encoder delay and padding add a fraction more; a pregap counted on one
/// side and not the other is up to two on a Red Book disc. Three seconds
/// absorbs all of that.
///
/// What it deliberately does not absorb is a wrong pairing. Two different
/// tracks off one album differ by tens of seconds far more often than by three,
/// and the row shows both numbers regardless — this only decides whether to
/// point at them, with no consequence beyond a mark: nothing here disables the
/// commit.
pub const LENGTH_DISAGREEMENT_MS: u64 = 3_000;

/// Whether two lengths, in milliseconds, are further apart than
/// [`LENGTH_DISAGREEMENT_MS`].
pub fn lengths_disagree(a_ms: u64, b_ms: u64) -> bool {
    a_ms.abs_diff(b_ms) > LENGTH_DISAGREEMENT_MS
}

/// The mediums a reading covers, as ascending positions into the release's
/// medium list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediumCoverage {
    positions: Vec<usize>,
}

impl MediumCoverage {
    /// No medium: what a release that lists no tracks is read over.
    pub(crate) fn none() -> Self {
        Self { positions: Vec::new() }
    }

    /// Every medium of a release with `count` of them.
    #[cfg(any(test, feature = "test-utils"))]
    pub(crate) fn all(count: usize) -> Self {
        Self { positions: (0..count).collect() }
    }

    pub(crate) fn covers(&self, position: usize) -> bool {
        self.positions.binary_search(&position).is_ok()
    }

    /// The covered mediums as the release numbers them, from 1.
    pub(crate) fn medium_numbers(&self) -> impl Iterator<Item = u32> + '_ {
        self.positions
            .iter()
            .map(|&position| u32::try_from(position + 1).expect("a medium number fits u32"))
    }
}

/// One medium as the choice weighs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MediumShape {
    /// Every number of tracks a layout of its rows gives: one for a
    /// MusicBrainz medium, one per index and heading layout for a Discogs one.
    /// Empty for a medium that lists no tracks, which no set covers.
    pub(crate) counts: BTreeSet<usize>,
    pub(crate) carrier: Option<Medium>,
    /// Whether a Disc ID MusicBrainz registers against it is one the folder's
    /// rips hash to.
    pub(crate) holds_folder_disc: bool,
}

/// Above this many mediums only runs of consecutive ones are tried, since
/// every subset would be too many to lay out.
const EXHAUSTIVE_MEDIUM_LIMIT: usize = 20;

/// Every set of mediums whose tracks, laid out some way, number `wanted`:
/// each set once, ascending positions, earlier sets first. A medium that
/// lists no tracks is in none of them.
pub(crate) fn candidate_sets(mediums: &[MediumShape], wanted: usize) -> Vec<MediumCoverage> {
    let mut sets = Vec::new();
    if mediums.len() <= EXHAUSTIVE_MEDIUM_LIMIT {
        let mut chosen = Vec::new();
        let reachable = Sums::start(wanted);
        subsets_reaching(mediums, wanted, 0, &reachable, &mut chosen, &mut sets);
    } else {
        for start in 0..mediums.len() {
            let mut reachable = Sums::start(wanted);
            for (end, medium) in mediums.iter().enumerate().skip(start) {
                if medium.counts.is_empty() {
                    break;
                }
                reachable = reachable.adding(&medium.counts);
                if reachable.holds(wanted) {
                    sets.push(MediumCoverage { positions: (start..=end).collect() });
                }
                if reachable.is_empty() {
                    break;
                }
            }
        }
    }
    sets
}

/// The track counts a set of mediums can lay out to, up to the folder's.
#[derive(Debug, Clone)]
struct Sums {
    reachable: Vec<bool>,
}

impl Sums {
    /// No medium yet: zero tracks.
    fn start(wanted: usize) -> Self {
        let mut reachable = vec![false; wanted + 1];
        reachable[0] = true;
        Self { reachable }
    }

    /// Every sum reachable once a medium of `counts` joins.
    fn adding(&self, counts: &BTreeSet<usize>) -> Self {
        let mut reachable = vec![false; self.reachable.len()];
        for (sum, _) in self.reachable.iter().enumerate().filter(|(_, held)| **held) {
            for count in counts {
                if let Some(slot) = reachable.get_mut(sum + count) {
                    *slot = true;
                }
            }
        }
        Self { reachable }
    }

    fn holds(&self, sum: usize) -> bool {
        self.reachable[sum]
    }

    /// Whether no sum within the folder's count is left.
    fn is_empty(&self) -> bool {
        !self.reachable.iter().any(|held| *held)
    }
}

fn subsets_reaching(
    mediums: &[MediumShape],
    wanted: usize,
    from: usize,
    reachable: &Sums,
    chosen: &mut Vec<usize>,
    sets: &mut Vec<MediumCoverage>,
) {
    if !chosen.is_empty() && reachable.holds(wanted) {
        sets.push(MediumCoverage { positions: chosen.clone() });
    }
    for position in from..mediums.len() {
        let counts = &mediums[position].counts;
        if counts.is_empty() {
            continue;
        }
        let next = reachable.adding(counts);
        if next.is_empty() {
            continue;
        }
        chosen.push(position);
        subsets_reaching(mediums, wanted, position + 1, &next, chosen, sets);
        chosen.pop();
    }
}

/// How well a set of mediums, laid out against the audio, answers for it.
/// Lower is better, and the fields decide in order: the folder's Disc IDs
/// first, then what its files say about the carrier, then the lengths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Fit {
    /// How many of the set's mediums hold a Disc ID the folder's rips hash
    /// to: more is better.
    discs_held: Reverse<usize>,
    /// How many of the set's mediums the audio's origin rules out: a proven
    /// CD rip rules out a carrier that plays no CD audio, and audio at a rate
    /// no CD plays rules out a carrier that plays only CD audio.
    ruled_out: usize,
    /// Tracks whose stated length is further than
    /// [`LENGTH_DISAGREEMENT_MS`] from the measured one.
    disagreeing: usize,
    /// Tracks the set states no length for: an unstated length is never
    /// better than a stated one that agrees.
    unstated: usize,
    /// The summed distance between stated and measured lengths.
    distance_ms: u64,
}

impl Fit {
    /// The fit of the set `coverage` of `mediums`, whose laid-out tracks
    /// state `stated` lengths against the audio's `measured` ones, track for
    /// track.
    pub(crate) fn of(
        mediums: &[MediumShape],
        coverage: &MediumCoverage,
        origin: &AudioOrigin,
        stated: &[Option<u64>],
        measured: &[u64],
    ) -> Self {
        assert_eq!(
            stated.len(),
            measured.len(),
            "a fitting layout states one length per measured track"
        );
        let covered = || coverage.positions.iter().map(|&position| &mediums[position]);
        let mut fit = Self {
            discs_held: Reverse(covered().filter(|medium| medium.holds_folder_disc).count()),
            ruled_out: covered().filter(|medium| rules_out(origin, medium.carrier)).count(),
            disagreeing: 0,
            unstated: 0,
            distance_ms: 0,
        };
        for (stated, &measured) in stated.iter().zip(measured) {
            match stated {
                Some(stated) => {
                    let distance = stated.abs_diff(measured);
                    if lengths_disagree(*stated, measured) {
                        fit.disagreeing += 1;
                    }
                    fit.distance_ms += distance;
                }
                None => fit.unstated += 1,
            }
        }
        fit
    }
}

/// Whether the audio's origin says it cannot have come off `carrier`. A
/// medium whose carrier the catalog does not say is never ruled out.
fn rules_out(origin: &AudioOrigin, carrier: Option<Medium>) -> bool {
    let Some(carrier) = carrier else {
        return false;
    };
    match carrier.cd_audio() {
        CdAudio::Never => origin.is_cd_rip(),
        CdAudio::Only => origin.not_cd_rate.is_some(),
        CdAudio::Partly => false,
    }
}

#[cfg(test)]
#[path = "medium_coverage_tests.rs"]
mod tests;
