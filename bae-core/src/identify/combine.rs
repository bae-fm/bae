//! Combine logic for the triangulation pipeline.
//!
//! Once the checked signals settle, the reducer hands their result sets to
//! `combine_results`, which intersects them. Pure: no I/O, no state.
//!
//! Every checked signal is a claim about the same disc, so a release has to
//! satisfy all of them — an intersection is what agreement looks like. Signals
//! the user left unchecked are not in the intersection at all: they arrive here
//! as an empty set and drop out. Signals that do not intersect are not a
//! failure to identify: each saw something, and the union of what they saw is
//! the set the user picks from, each row carrying which signal produced it.
//!
//! **The pressing is what is offered or set aside, not the release.** Two
//! sources' records of one physical object are one row a person picks whole,
//! and the two rarely arrive by the same route: a disc ID answers on
//! MusicBrainz alone, so the Discogs record of that same pressing can only ever
//! be a barcode's answer and never the intersection's. So every answer the run
//! returned is paired first — [`group_results`] — and agreement is then read
//! off whole rows: a row survives the intersection when any of its records is
//! in it, and what the candidate's text agrees with about the row is what its
//! records agree with together.
//!
//! The candidate's own text is the second narrowing. Each row is judged
//! against it — see [`super::agreements`] — the rows are ordered by how much of
//! the folder agrees with them, and a pressing the folder says nothing about
//! joins what the intersection left out.

use super::agreements::{agreements_of, CandidateText};
use crate::db::LibraryStatus;
use crate::import::release_group::{group_results, Judged, Judgements, Pressing};
use crate::import::search::MetadataResult;
use crate::import::Catalog;
use std::collections::{HashMap, HashSet};

/// Which lookup produced one result: the result came back from that signal's
/// lookup. The other half of a row's badges — what the folder's own text says
/// about the result — is derived from this and the text (see
/// [`agreements_of`]), never stored.
///
/// `Serialize`/`Deserialize`: carried on `identify::TerminalVerdict::Found`,
/// which `import_candidate_match` persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LookupProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
}

/// The pressings agreement left out, as the releases they are made of — every
/// row a checked signal named that the intersection does not hold.
///
/// Agreement is what makes a short list: a disc ID that named three releases
/// and a barcode that named two settle on the one they share, and the other
/// four never reach the person. Each of those four is a real answer from a real
/// lookup, and one of them may be the disc on the desk, so combine hands them
/// back beside the matches instead of dropping them.
///
/// The rows the folder's own text says nothing about are here too: a barcode
/// lookup that comes back naming somebody else's record answered a question
/// the folder never asked.
///
/// A row is here whole or not at all — every record of a set-aside pressing,
/// and none of an offered one — and `pressings` says which row each release
/// belongs to, so a reader reads the rows the run built rather than forming
/// its own from a list that no longer holds what they were decided against.
///
/// Empty when nothing was narrowed: one signal answering alone is the whole
/// answer, signals that shared nothing already list their union, and a set the
/// text agrees with nowhere is offered whole rather than emptied.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NarrowedOut {
    /// In signal order, each release once.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`.
    pub library_statuses: Vec<LibraryStatus>,
    /// Index-aligned with `matches`: which signals named each one.
    pub provenance: Vec<LookupProvenance>,
    /// Index-aligned with `matches`: which row of this list each release
    /// belongs to, numbered from zero in row order.
    pub pressings: Vec<u32>,
}

impl NarrowedOut {
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
}

/// What combine decided; the reducer lifts it into a terminal `IdentifyState`.
#[derive(Debug, Clone)]
pub enum CombineOutcome {
    /// One or more results. `provenance` is index-aligned with `matches` and
    /// says which signal produced each one.
    Found {
        matches: Vec<MetadataResult>,
        library_statuses: Vec<LibraryStatus>,
        provenance: Vec<LookupProvenance>,
        /// Index-aligned with `matches`: which row each release belongs to,
        /// numbered from zero in row order.
        pressings: Vec<u32>,
        narrowed_out: NarrowedOut,
    },
    /// Every checked signal settled with zero results.
    NotFoundAnywhere,
}

type Results = Vec<(MetadataResult, LibraryStatus)>;
type ReleaseKey = (Catalog, String);

/// Settle the checked signals' results into a `CombineOutcome`.
///
/// A signal the user left unchecked arrives empty and takes no part. So does a
/// checked signal whose lookup found nothing — which lands on the same answer
/// either way, since an intersection it emptied would fall through to the
/// union of the rest.
///
/// Every answer the run returned is paired into pressing rows first, and the
/// two narrowings then read those rows:
///
/// 1. **Nothing.** Every set empty: `NotFoundAnywhere`.
/// 2. **Agreement.** With more than one set answering, the releases every set
///    names are what agreement holds; a row holding none of them is set aside.
///    One set alone is the whole answer, and sets that share nothing named
///    different releases — neither narrows anything, and every row stands.
/// 3. **The candidate's own text.** A row the folder says nothing about joins
///    what agreement left out.
///
/// The rows come back most-agreed-with first, as records, each carrying the
/// row of its list it belongs to: a row is offered whole or set aside whole,
/// and which rows those are is this run's answer, stored with its releases
/// rather than re-derived from either list alone.
pub fn combine_results(
    discid_results: Results,
    barcode_results: Results,
    catalog_results: Results,
    text: &CandidateText,
) -> CombineOutcome {
    let by_signal = [&discid_results, &barcode_results, &catalog_results];
    let keys: Vec<HashSet<ReleaseKey>> = by_signal.iter().map(|set| release_keys(set)).collect();

    let present: Vec<&Results> = by_signal
        .into_iter()
        .filter(|set| !set.is_empty())
        .collect();
    let Some((first, rest)) = present.split_first() else {
        return CombineOutcome::NotFoundAnywhere;
    };

    // Every answer the run returned, each release once, in signal order.
    // Pairing runs over all of them, so two sources' records of one pressing
    // meet however the intersection falls between them.
    let all = union_all(&present);

    // What every answering signal named. Empty when nothing narrows: one set
    // alone is the whole answer, and sets that share nothing already list
    // their union.
    let agreed: HashSet<ReleaseKey> = match rest.is_empty() {
        true => HashSet::new(),
        false => release_keys(&intersect_all(first, rest)),
    };

    let lookup_of = |result: &MetadataResult| {
        let key = (result.source, result.release_id.clone());
        LookupProvenance {
            by_disc_id: keys[0].contains(&key),
            by_barcode: keys[1].contains(&key),
            by_catalog: keys[2].contains(&key),
        }
    };
    let judged: Vec<Judged> = all
        .iter()
        .map(|(result, _)| {
            let agreements = agreements_of(result, text, &lookup_of(result));
            (result.clone(), agreements)
        })
        .collect();
    let judgements = Judgements::of(&judged);
    let rows: Vec<Pressing> = group_results(judged)
        .into_iter()
        .flat_map(|group| group.pressings)
        .collect();
    let (offered, set_aside) = split_rows(rows, &judgements, &agreed, text.is_empty());

    let statuses: HashMap<ReleaseKey, LibraryStatus> = all
        .into_iter()
        .map(|(result, status)| ((result.source, result.release_id), status))
        .collect();
    // Each list's records in row order, each carrying the row of that list it
    // belongs to. The rows are what this run decided, and they are carried
    // rather than re-derived: neither list alone holds what the other said,
    // and grouping one without the other rolls up records this kept apart.
    let records = |rows: Vec<Pressing>| -> (Results, Vec<u32>) {
        let mut records = Results::new();
        let mut pressings = Vec::new();
        for (row, pressing) in rows.into_iter().enumerate() {
            let row = u32::try_from(row).expect("a run answers fewer rows than u32 counts");
            for result in pressing.releases {
                let status = statuses
                    .get(&(result.source, result.release_id.clone()))
                    .cloned()
                    .expect("a pressing is built from the run's own answers");
                records.push((result, status));
                pressings.push(row);
            }
        }
        (records, pressings)
    };

    let (combined, pressings) = records(offered);
    let (left_out, narrowed_out_pressings) = records(set_aside);
    let provenance = combined.iter().map(|(r, _)| lookup_of(r)).collect();
    let narrowed_out_provenance = left_out.iter().map(|(r, _)| lookup_of(r)).collect();
    let (matches, library_statuses) = combined.into_iter().unzip();
    let (narrowed_matches, narrowed_statuses) = left_out.into_iter().unzip();
    CombineOutcome::Found {
        matches,
        library_statuses,
        provenance,
        pressings,
        narrowed_out: NarrowedOut {
            matches: narrowed_matches,
            library_statuses: narrowed_statuses,
            provenance: narrowed_out_provenance,
            pressings: narrowed_out_pressings,
        },
    }
}

/// Split the ranked rows into the ones offered and the ones set aside, each
/// keeping the ranked order.
///
/// A row is set aside when agreement left it out — `agreed` names releases and
/// none of the row's is among them — or when the candidate's text states
/// nothing about it.
///
/// Nothing is set aside on the text unless the text is evidence. `speechless`
/// is a candidate that carries no text at all — a library release being
/// re-identified before its artwork is read — and nothing was consulted about
/// its answers. Neither is anything set aside when the text stands behind none
/// of the rows agreement kept: folding shortens the list, it never empties it.
fn split_rows(
    rows: Vec<Pressing>,
    judgements: &Judgements,
    agreed: &HashSet<ReleaseKey>,
    speechless: bool,
) -> (Vec<Pressing>, Vec<Pressing>) {
    let held: Vec<bool> =
        rows.iter()
            .map(|row| {
                agreed.is_empty()
                    || row.releases.iter().any(|release| {
                        agreed.contains(&(release.source, release.release_id.clone()))
                    })
            })
            .collect();
    let stated: Vec<bool> = rows
        .iter()
        .map(|row| row.agreements(judgements).offered())
        .collect();
    let kept_rows = || held.iter().zip(&stated).filter(|(held, _)| **held);
    let fold_on_text = !speechless
        && kept_rows().any(|(_, stated)| *stated)
        && kept_rows().any(|(_, stated)| !*stated);

    let mut offered = Vec::new();
    let mut set_aside = Vec::new();
    for ((row, held), stated) in rows.into_iter().zip(&held).zip(&stated) {
        match *held && (!fold_on_text || *stated) {
            true => offered.push(row),
            false => set_aside.push(row),
        }
    }
    (offered, set_aside)
}

fn release_keys(results: &Results) -> HashSet<ReleaseKey> {
    results
        .iter()
        .map(|(r, _)| (r.source, r.release_id.clone()))
        .collect()
}

/// The releases every set names, in the first set's order.
fn intersect_all(first: &Results, rest: &[&Results]) -> Results {
    let rest_keys: Vec<HashSet<ReleaseKey>> = rest.iter().map(|set| release_keys(set)).collect();
    first
        .iter()
        .filter(|(r, _)| {
            let key = (r.source, r.release_id.clone());
            rest_keys.iter().all(|keys| keys.contains(&key))
        })
        .cloned()
        .collect()
}

/// Every release any set names, in signal order, each release once — every
/// answer the run returned, which is what pairing runs over. A release two
/// signals both named is kept as the earlier signal returned it.
fn union_all(sets: &[&Results]) -> Results {
    let mut seen: HashSet<ReleaseKey> = HashSet::new();
    let mut out = Results::new();
    for set in sets {
        for pair in set.iter() {
            if seen.insert((pair.0.source, pair.0.release_id.clone())) {
                out.push(pair.clone());
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "combine_tests.rs"]
mod tests;
