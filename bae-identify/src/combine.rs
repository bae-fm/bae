//! Ranks what a run's lookups returned by what each row states against the
//! folder, and offers the rows that state the most. Which searches returned
//! a row only brought it in and adds no weight. Pure: no I/O, no state.
//!
//! Every record is first paired into pressing rows — two sources' records of
//! one object are one row, picked whole. A row whose read tracklist holds
//! other tracks than the folder is not a match and is left out (see
//! `fit::rules_out`); each row left is scored by `Support`. The rows
//! tied at the top are offered; the rest are set aside under "N more
//! releases".

use super::agreements::{agreements_of, CandidateText};
use super::medium::{agrees_with_mono, FolderAudio, RippedFrom};
use super::row_facts::{Fact, FolderFacts};
use crate::release_group::{group_results, Judged, Judgements, Pressing, ReleaseGroup};
use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::MetadataResult;
use std::collections::{HashMap, HashSet};

mod support;
use support::split_rows;

/// What a run's lookups found once combined: the offered rows and the rows
/// set aside, each release with its pressing row. Which lookups returned a
/// release is the run's ledger's to say. A failed run's findings are stored
/// the same way as a found one's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Findings {
    /// The offered rows' releases, most-agreed-with first.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`: each release's pressing row, numbered
    /// from zero. Kept rather than re-formed, since grouping one list alone
    /// can merge records the run kept apart.
    pub pressings: Vec<u32>,
    pub narrowed_out: NarrowedOut,
    /// What the folder's files prove when they rule out every row. The rows
    /// are still offered, but nothing picks one unattended.
    pub medium_conflict: Option<super::MediumConflict>,
    /// The note of each offered release that writes a word the folder states
    /// and no other row tied with its row writes — why the ranking's notes
    /// point went to its row (see `identify::notes`), and what the Notes badge
    /// shows. Kept rather than read again, since which rows it was weighed
    /// among is the ranking's alone. Empty where the folder named none.
    pub named_notes: Vec<NamedNote>,
}

/// A note of an offered release the folder's text names its row by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedNote {
    pub release: bae_catalogs::records::LookupRef,
    pub note: String,
}

impl Findings {
    /// Nothing came back from any lookup.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Every release named, the offered ones first.
    pub fn releases(&self) -> impl Iterator<Item = &MetadataResult> {
        self.matches.iter().chain(&self.narrowed_out.matches)
    }
}

/// The rows the ranking did not offer, whole, as the releases they are made
/// of. Empty when every row tied.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NarrowedOut {
    /// In signal order, each release once.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`: each release's row in this list.
    pub pressings: Vec<u32>,
}

impl NarrowedOut {
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
}

type Results = Vec<MetadataResult>;
type ReleaseKey = (LookupCatalog, String);

/// What each kind of a run's keys returned.
#[derive(Debug, Clone, Default)]
pub struct LookupAnswers {
    /// One set per disc ID the folder's files hash to, in their order.
    pub disc_id: Vec<Results>,
    pub barcode: Results,
    pub catalog: Results,
    pub isrc: Results,
    pub search: Results,
    /// The offered rows' pressings, looked up on the catalogs they had no
    /// record of.
    pub pressing: Results,
}

impl LookupAnswers {
    /// Every key's results, in the order above.
    pub(crate) fn all(&self) -> impl Iterator<Item = &MetadataResult> {
        self.disc_id
            .iter()
            .flatten()
            .chain(&self.barcode)
            .chain(&self.catalog)
            .chain(&self.isrc)
            .chain(&self.search)
            .chain(&self.pressing)
    }
}

/// Pool every key's results into what the run found: each release once. An
/// empty set takes no part.
pub fn combine_results(
    answers: LookupAnswers,
    text: &CandidateText,
    folder: FolderAudio<'_>,
) -> Findings {
    let disc_id = union_all(&answers.disc_id);
    let ripped_from = RippedFrom::of(folder.origin, !disc_id.is_empty());
    // How many of the folder's discs each release holds: each disc ID's
    // lookup returns a release only where exactly one of its mediums carries
    // that disc, and the folder's discs are distinct discs, so each disc ID
    // that returned it is another of its mediums. A release no disc ID's
    // lookup returned holds none.
    let mut discs_held: HashMap<ReleaseKey, u32> = HashMap::new();
    for disc in &answers.disc_id {
        for key in release_keys(disc) {
            *discs_held.entry(key).or_default() += 1;
        }
    }
    let by_isrc = release_keys(&answers.isrc);

    let present: Vec<&Results> = [
        &disc_id,
        &answers.barcode,
        &answers.catalog,
        &answers.isrc,
        &answers.search,
        &answers.pressing,
    ]
    .into_iter()
    .filter(|set| !set.is_empty())
    .collect();
    if present.is_empty() {
        return Findings::default();
    }

    // Every release once, in key order.
    let all = union_all(present);

    let facts = FolderFacts::of(text, all.iter());
    let judged: Vec<Judged> = all
        .iter()
        .map(|result| {
            let by_disc_id = discs_held.contains_key(&(result.source, result.release_id.clone()));
            let agreements = agreements_of(result, text, &facts, by_disc_id);
            (result.clone(), agreements)
        })
        .collect();
    let judgements = Judgements::of(&judged, Some(folder.track_count));
    // Neither offered nor set aside: a row the folder cannot be is no answer.
    let rows: Vec<Pressing> = group_results(judged, Some(folder.track_count))
        .into_iter()
        .flat_map(ReleaseGroup::into_pressings)
        .filter(|row| !super::fit::rules_out(&row.releases, folder.track_count))
        .collect();
    let (offered, set_aside, medium_conflict, notes) =
        split_rows(rows, &judgements, &by_isrc, &discs_held, ripped_from, folder, text, &facts);
    // Offered rows that would look alike on screen are listed once.
    let offered = super::look_alike::keep_one_of_each_look(offered, &judgements, &notes);

    // Each list's records in row order, with the row each belongs to.
    let records = |rows: Vec<Pressing>| -> (Results, Vec<u32>) {
        let mut records = Results::new();
        let mut pressings = Vec::new();
        for (row, pressing) in rows.into_iter().enumerate() {
            let row = u32::try_from(row).expect("a run answers fewer rows than u32 counts");
            for result in pressing.releases {
                records.push(result);
                pressings.push(row);
            }
        }
        (records, pressings)
    };

    let (combined, pressings) = records(offered);
    let named_notes = combined
        .iter()
        .filter_map(|result| {
            notes
                .get(&(result.source, result.release_id.clone()))
                .map(|note| NamedNote { release: result.release(), note: note.clone() })
        })
        .collect();
    let (left_out, narrowed_out_pressings) = records(set_aside);
    Findings {
        matches: combined,
        pressings,
        narrowed_out: NarrowedOut { matches: left_out, pressings: narrowed_out_pressings },
        medium_conflict,
        named_notes,
    }
}

fn release_keys(results: &Results) -> HashSet<ReleaseKey> {
    results.iter().map(|r| (r.source, r.release_id.clone())).collect()
}

/// Every release any set names, once, in signal order.
pub(crate) fn union_all<'a>(sets: impl IntoIterator<Item = &'a Results>) -> Results {
    let mut seen: HashSet<ReleaseKey> = HashSet::new();
    let mut out = Results::new();
    for set in sets {
        for pair in set.iter() {
            if seen.insert((pair.source, pair.release_id.clone())) {
                out.push(pair.clone());
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "combine_tests.rs"]
mod tests;
