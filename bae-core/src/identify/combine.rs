//! Combine logic for the triangulation pipeline.
//!
//! Once the checked signals settle, the reducer hands their result sets to
//! `combine_results`, which ranks them and offers the best-supported rows.
//! Pure: no I/O, no state.
//!
//! **The pressing is what is offered or set aside, not the release.** Two
//! sources' records of one physical object are one row a person picks whole,
//! and the two rarely arrive by the same route: a disc ID answers on
//! MusicBrainz alone, so the Discogs record of that same pressing can only
//! ever be a barcode's answer. So every answer the run returned is paired
//! first — [`group_results`] — and the ranking then reads whole rows.
//!
//! **Every row is scored, and the rows tied at the top are offered.** The
//! score is `Support`: how many lookups returned the row, then how many of
//! the two facts that name one pressing hold, then whether the folder's text
//! mentions the row at all. Every other row is set aside under "N more
//! releases", which a person can open.
//!
//! Taking the highest score is what used to be three separate rules. Two
//! lookups naming one release outrank one lookup naming another, which is the
//! intersection of the answering lookups. A row nothing else tells apart but
//! the folder's catalog number is the pressing on the desk. A row the folder
//! never mentions, beside rows it does, came from a misread barcode. And a
//! score always has a highest value, so the list is shortened and never
//! emptied.

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
    /// The title search returned it. By construction the search runs only when
    /// the three identifiers named nothing, so this is never true beside any
    /// of the others.
    pub by_search: bool,
}

/// The rows the ranking did not offer, as the releases they are made of.
///
/// A short list is what makes identification worth having: a disc ID that
/// named three releases and a barcode that named two settle on the one they
/// share, and the other four never reach the person. Each of those four is a
/// real answer from a real lookup, and one of them may be the disc on the
/// desk, so combine hands them back beside the matches instead of dropping
/// them.
///
/// The rows the folder's own text never mentions are here too: a barcode
/// lookup that comes back naming somebody else's record answered a question
/// the folder never asked.
///
/// A row is here whole or not at all — every record of a set-aside pressing,
/// and none of an offered one — and `pressings` says which row each release
/// belongs to, so a reader reads the rows the run built rather than forming
/// its own from a list that no longer holds what they were decided against.
///
/// Empty when every row tied at the highest score: one lookup answering alone
/// with nothing to tell its answers apart, or a candidate carrying no text to
/// read them against.
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
/// A signal the user left unchecked arrives empty and takes no part. So does
/// a checked signal whose lookup found nothing: it returned no row, so it
/// raises no row's score.
///
/// The title search is a fourth set on the same footing. It never meets the
/// other three: it is asked only when all of them came back empty, so a run
/// that reaches it ranks what the search alone returned.
///
/// Every answer the run returned is paired into pressing rows first, then:
///
/// 1. **Nothing.** Every set empty: `NotFoundAnywhere`.
/// 2. **Every row is scored** by `Support`, and the rows tied at the
///    highest score are offered. Every other row is set aside, and a person
///    can open the list it is on.
///
/// The offered rows come back most-agreed-with first, as records, each
/// carrying the row of its list it belongs to: a row is offered whole or set
/// aside whole, and which rows those are is this run's answer, stored with
/// its releases rather than re-derived from either list alone.
pub fn combine_results(
    discid_results: Results,
    barcode_results: Results,
    catalog_results: Results,
    search_results: Results,
    text: &CandidateText,
) -> CombineOutcome {
    let by_signal = [
        &discid_results,
        &barcode_results,
        &catalog_results,
        &search_results,
    ];
    let keys: Vec<HashSet<ReleaseKey>> = by_signal.iter().map(|set| release_keys(set)).collect();

    let present: Vec<&Results> = by_signal
        .into_iter()
        .filter(|set| !set.is_empty())
        .collect();
    if present.is_empty() {
        return CombineOutcome::NotFoundAnywhere;
    }

    // Every answer the run returned, each release once, in signal order.
    // Pairing runs over all of them, so two sources' records of one pressing
    // are one row whichever lookup returned each of them.
    let all = union_all(&present);

    let lookup_of = |result: &MetadataResult| {
        let key = (result.source, result.release_id.clone());
        LookupProvenance {
            by_disc_id: keys[0].contains(&key),
            by_barcode: keys[1].contains(&key),
            by_catalog: keys[2].contains(&key),
            by_search: keys[3].contains(&key),
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
    let returned_by: HashMap<ReleaseKey, LookupProvenance> = all
        .iter()
        .map(|(result, _)| {
            (
                (result.source, result.release_id.clone()),
                lookup_of(result),
            )
        })
        .collect();
    let rows: Vec<Pressing> = group_results(judged)
        .into_iter()
        .flat_map(|group| group.pressings)
        .collect();
    let (offered, set_aside) = split_rows(rows, &judgements, &returned_by);

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

/// How much of what the run found stands behind one row. Rows are compared
/// field by field in declaration order, and the rows tied at the highest
/// value are the ones offered.
///
/// Each field answers a different question, and a lower one is read only
/// between rows the field above it cannot tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
struct Support {
    /// How many of the run's lookups returned this row: the disc ID, the
    /// barcodes, the chosen catalog numbers, the title search. A lookup that
    /// returned nothing counts for no row, so an unchecked lookup and one
    /// that found nothing both change the ranking in no way.
    ///
    /// Two lookups returning one release outrank one lookup returning
    /// another. Where no row was returned twice, every row ties here and the
    /// fields below decide.
    lookups: u32,
    /// How many of the two facts that name a single pressing hold: the disc
    /// ID returned this row, and the folder's text states this row's catalog
    /// number.
    ///
    /// A disc ID is computed from the audio on disk, and a catalog number is
    /// printed on the disc itself. Each names one pressing rather than one
    /// album, which is why they are read above the fields below and why the
    /// disc ID counts here as well as above.
    pressing: u32,
    /// Whether there is any reason to show this row at all — see
    /// [`super::agreements::Agreements::offered`].
    ///
    /// One value rather than a count of the fields behind it, and that is
    /// what keeps three pressings of one album on the list together: the
    /// folder states one pressing's year and not the other two's, and a
    /// folder is usually named by the year the album came out rather than the
    /// year the disc was pressed. A label covers every pressing of an album
    /// and a country covers most of them, so none of the three may separate
    /// one row from another. They separate a row the folder describes from a
    /// row nothing stands behind.
    offered: bool,
}

/// What stands behind one row: its records' lookups taken together, and what
/// the folder's text states about the row as a whole.
fn support_of(
    row: &Pressing,
    judgements: &Judgements,
    provenance: &HashMap<ReleaseKey, LookupProvenance>,
) -> Support {
    let mut returned = LookupProvenance {
        by_disc_id: false,
        by_barcode: false,
        by_catalog: false,
        by_search: false,
    };
    for release in &row.releases {
        let found = provenance
            .get(&(release.source, release.release_id.clone()))
            .expect("a pressing is built from the run's own answers");
        returned.by_disc_id |= found.by_disc_id;
        returned.by_barcode |= found.by_barcode;
        returned.by_catalog |= found.by_catalog;
        returned.by_search |= found.by_search;
    }
    let agreements = row.agreements(judgements);
    Support {
        lookups: [
            returned.by_disc_id,
            returned.by_barcode,
            returned.by_catalog,
            returned.by_search,
        ]
        .into_iter()
        .filter(|returned| *returned)
        .count() as u32,
        pressing: u32::from(returned.by_disc_id) + u32::from(agreements.catalog),
        offered: agreements.offered(),
    }
}

/// Split the ranked rows into the ones offered and the ones set aside, each
/// keeping the ranked order: the rows tied at the highest [`Support`] are
/// offered, and every other row is set aside.
fn split_rows(
    rows: Vec<Pressing>,
    judgements: &Judgements,
    provenance: &HashMap<ReleaseKey, LookupProvenance>,
) -> (Vec<Pressing>, Vec<Pressing>) {
    let support: Vec<Support> = rows
        .iter()
        .map(|row| support_of(row, judgements, provenance))
        .collect();
    let Some(best) = support.iter().copied().max() else {
        return (Vec::new(), Vec::new());
    };
    let mut offered = Vec::new();
    let mut set_aside = Vec::new();
    for (row, support) in rows.into_iter().zip(&support) {
        match *support == best {
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
