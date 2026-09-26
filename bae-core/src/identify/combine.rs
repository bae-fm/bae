//! Combine logic for the triangulation pipeline.
//!
//! Once the checked signals settle, the reducer hands their result sets to
//! `combine_results`, which ranks them and offers the best-supported rows.
//! Pure: no I/O, no state.
//!
//! **The pressing is what is offered or set aside, not the release.** Two
//! sources' records of one physical object are one row a person picks whole,
//! and the two rarely arrive by the same route: a disc ID answers on
//! MusicBrainz alone, so the Discogs record of that pressing comes from a
//! barcode or catalog number lookup — or from no lookup at all, read because
//! the MusicBrainz release names it as itself (see
//! [`crate::import::album_links`]). So every record the run holds is paired
//! first — [`group_results`] — and the ranking then reads whole rows.
//!
//! **A record no lookup returned counts for no lookup.** A twin read through a
//! MusicBrainz release's link is on the row because that release names it,
//! not because a lookup of the folder's codes found it, so it raises no row's
//! lookup count and says so in its provenance: [`LookupProvenance::named_by`].
//! What the folder's text states about its fields is still what the text
//! states about the row, which is one object whichever record says it.
//!
//! **Every row is scored, and the rows tied at the top are offered.** The
//! score is `Support`: whether the row's media could have given the folder
//! its audio, how many lookups returned the row, how many of the facts that
//! name one pressing hold, whether the disc ID returned it, and whether the
//! folder's text mentions the row at all. Every other row is set aside under
//! "N more releases", which a person can open.
//!
//! Taking the highest score is what would otherwise be separate rules. A row
//! made of vinyl beside a folder its rip log proves is a CD rip describes some
//! other object. Two lookups naming one release outrank one lookup naming
//! another, which is the intersection of the answering lookups. A barcode and
//! a catalog number name one pressing, where a disc ID names every pressing
//! cut from one master. A row the folder never mentions, beside rows it does,
//! came from a misread barcode. And a score always has a highest value, so
//! the list is shortened and never emptied.

use super::agreements::{agreements_of, CandidateText};
use super::medium::RippedFrom;
use crate::db::LibraryStatus;
use crate::import::album_links::Twin;
use crate::import::release_group::{group_results, Judged, Judgements, Pressing, ReleaseGroup};
use crate::import::search::MetadataResult;
use crate::import::Catalog;
use crate::signals::RipEvidence;
use std::collections::{HashMap, HashSet};

/// Which lookup produced one result: the result came back from that signal's
/// lookup. The other half of a row's badges — what the folder's own text says
/// about the result — is derived from this and the text (see
/// [`agreements_of`]), never stored.
///
/// `Serialize`/`Deserialize`: carried on the [`Findings`] a stored
/// `identify::TerminalVerdict` holds, which `import_candidate_match` persists.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LookupProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
    /// The title search returned it. By construction the search runs only when
    /// the three identifiers named nothing, so this is never true beside any
    /// of the others.
    pub by_search: bool,
    /// Returned by no lookup: the MusicBrainz release whose own document
    /// names this one as the same release, which the run read to learn its
    /// album. `None` for a release a lookup returned or a person chose; never
    /// set beside any of the four above.
    pub named_by: Option<crate::import::MetadataRef>,
}

impl LookupProvenance {
    /// Returned by no lookup and named by nothing: a release a person chose.
    pub const CHOSEN: Self = Self {
        by_disc_id: false,
        by_barcode: false,
        by_catalog: false,
        by_search: false,
        named_by: None,
    };
}

/// What a run's lookups found, once combined: the rows offered and the rows
/// set aside, each release with the lookups that returned it and the pressing
/// row it belongs to.
///
/// One shape for every state that holds answers. A run that settled on them
/// carries it, and so does a run where some lookup failed: one provider not
/// answering never invalidates what the others returned, so a failed run's
/// findings are as real as a found one's and are stored the same way. A run
/// that found nothing has none, which is why `NotFoundAnywhere` carries no
/// findings at all rather than an empty one.
///
/// Whether each release is already in the library is not here: that is a
/// live check a reader repeats, never a fact about the run — see
/// [`LibraryStatuses`].
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Findings {
    /// The offered rows' releases, most-agreed-with first.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`: which signals named each one — the
    /// sidebar's "matched on disc ID / barcode / text" evidence line.
    pub provenance: Vec<LookupProvenance>,
    /// Index-aligned with `matches`: which pressing row of this list each
    /// release belongs to, numbered from zero in row order.
    ///
    /// The rows the run built, kept rather than re-formed on read: a record
    /// the run settled as ambiguous because of a record in the other list
    /// rolls up when this list is grouped without it, so a reader that
    /// re-groups shows rows the run never offered.
    pub pressings: Vec<u32>,
    pub narrowed_out: NarrowedOut,
    /// Set when the folder's own files rule out every row, offered ones
    /// included: what they prove. The rows are still offered for a person to
    /// pick, and nothing picks one for them.
    pub medium_conflict: Option<super::MediumConflict>,
}

impl Findings {
    /// Nothing came back from any lookup. Combine never empties the offered
    /// list while anything was returned, so no offered match means no answer.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    /// Every release named, the offered ones first — what a reader checks
    /// live library status for.
    pub fn releases(&self) -> impl Iterator<Item = &MetadataResult> {
        self.matches.iter().chain(&self.narrowed_out.matches)
    }
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
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NarrowedOut {
    /// In signal order, each release once.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`: which signals named each one.
    pub provenance: Vec<LookupProvenance>,
    /// Index-aligned with `matches`: which row of this list each release
    /// belongs to, numbered from zero in row order. Each list numbers its own
    /// rows.
    pub pressings: Vec<u32>,
}

impl NarrowedOut {
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
}

/// Whether each release a [`Findings`] names is already in the library,
/// index-aligned with its two lists.
///
/// Mutable local state — another import landing can flip it — so it rides
/// beside the findings while a state is live and is never stored with them: a
/// reader standing a stored verdict back up checks it again.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibraryStatuses {
    /// Index-aligned with [`Findings::matches`].
    pub matches: Vec<LibraryStatus>,
    /// Index-aligned with [`NarrowedOut::matches`].
    pub narrowed_out: Vec<LibraryStatus>,
}

impl LibraryStatuses {
    /// Check every release `findings` names with `status_of`.
    pub fn of(findings: &Findings, status_of: impl Fn(&MetadataResult) -> LibraryStatus) -> Self {
        Self {
            matches: findings.matches.iter().map(&status_of).collect(),
            narrowed_out: findings
                .narrowed_out
                .matches
                .iter()
                .map(&status_of)
                .collect(),
        }
    }
}

type Results = Vec<(MetadataResult, LibraryStatus)>;
type ReleaseKey = (Catalog, String);

/// Settle the checked signals' results into what the run found, with each
/// release's library status as the lookups reported it.
///
/// A signal the user left unchecked arrives empty and takes no part. So does
/// a checked signal whose lookup found nothing: it returned no row, so it
/// raises no row's score.
///
/// The title search is a fourth set on the same footing. It never meets the
/// other three: it is asked only when all of them came back empty, so a run
/// that reaches it ranks what the search alone returned.
///
/// `twins` are the releases no lookup returned, each read because a release a
/// lookup did return names it as itself. Each joins the list beside the
/// release that names it, and raises no row's lookup count.
///
/// `rip` is what the folder's files say about the medium its audio was
/// ripped from; with whether the disc ID returned anything, it is what a
/// row's stated media are held against.
///
/// Every answer the run returned is paired into pressing rows first, then:
///
/// 1. **Nothing.** Every set empty: empty findings.
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
    twins: Vec<Twin>,
    text: &CandidateText,
    rip: &RipEvidence,
) -> (Findings, LibraryStatuses) {
    let ripped_from = RippedFrom::of(rip, !discid_results.is_empty());
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
        return (Findings::default(), LibraryStatuses::default());
    }

    // Every answer the run returned, each release once, in signal order, and
    // then each twin beside the release that names it. Pairing runs over all
    // of them, so two sources' records of one pressing are one row whichever
    // way each of them came.
    let mut all = union_all(&present);
    let answered: Vec<&MetadataResult> = all.iter().map(|(result, _)| result).collect();
    let twins: Vec<(MetadataResult, LibraryStatus, crate::import::MetadataRef)> =
        crate::import::album_links::beside(&twins, &answered)
            .into_iter()
            .map(|twin| {
                (
                    twin.result.clone(),
                    twin.status.clone(),
                    twin.named_by.clone(),
                )
            })
            .collect();
    let named_by: HashMap<ReleaseKey, crate::import::MetadataRef> = twins
        .iter()
        .map(|(result, _, by)| ((result.source, result.release_id.clone()), by.clone()))
        .collect();
    all.extend(
        twins
            .into_iter()
            .map(|(result, status, _)| (result, status)),
    );

    let lookup_of = |result: &MetadataResult| {
        let key = (result.source, result.release_id.clone());
        LookupProvenance {
            by_disc_id: keys[0].contains(&key),
            by_barcode: keys[1].contains(&key),
            by_catalog: keys[2].contains(&key),
            by_search: keys[3].contains(&key),
            named_by: named_by.get(&key).cloned(),
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
        .flat_map(ReleaseGroup::into_pressings)
        .collect();
    let (offered, set_aside, medium_conflict) =
        split_rows(rows, &judgements, &returned_by, ripped_from);

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
                    .expect("a pressing is built from the run's own records");
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
    (
        Findings {
            matches,
            provenance,
            pressings,
            narrowed_out: NarrowedOut {
                matches: narrowed_matches,
                provenance: narrowed_out_provenance,
                pressings: narrowed_out_pressings,
            },
            medium_conflict,
        },
        LibraryStatuses {
            matches: library_statuses,
            narrowed_out: narrowed_statuses,
        },
    )
}

/// How much of what the run found stands behind one row. Rows are compared
/// field by field in declaration order, and the rows tied at the highest
/// value are the ones offered.
///
/// Each field answers a different question, and a lower one is read only
/// between rows the field above it cannot tell apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
struct Support {
    /// Whether what the row's records say it is made of could have given
    /// the folder its audio — see [`RippedFrom::admits`].
    ///
    /// Read first because it is the one field that speaks to the object
    /// rather than to how well the lookups agree: a vinyl pressing every
    /// lookup returned is still not the CD the folder's rip log was read
    /// off. It leaves every row admitted when the folder proves nothing and
    /// when every row states nothing.
    medium: bool,
    /// How many of the run's lookups returned this row: the disc ID, the
    /// barcodes, the chosen catalog numbers, the title search. A lookup that
    /// returned nothing counts for no row, so an unchecked lookup and one
    /// that found nothing both change the ranking in no way.
    ///
    /// Two lookups returning one release outrank one lookup returning
    /// another. Where no row was returned twice, every row ties here and the
    /// fields below decide.
    lookups: u32,
    /// How many of the facts that name this one pressing hold: the folder
    /// states the row's catalog number, and a barcode lookup returned the
    /// row.
    ///
    /// A catalog number and a barcode are printed on one pressing's sleeve
    /// and disc, and a later pressing is given its own. The barcode counts
    /// only for a row the folder's text also describes (see `offered`): an
    /// image's bars misread into another valid code name some other
    /// record entirely, which the text then says nothing about.
    names_pressing: u32,
    /// Whether the disc ID returned this row.
    ///
    /// A disc ID is computed from the audio on disk, so it names the disc —
    /// but every pressing cut from one master has the same table of
    /// contents, so it names all of them alike. That is why it is read below
    /// what names one pressing, and why it counts here as well as among the
    /// lookups.
    shares_toc: bool,
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

/// What stands behind one row: whether its media fit the folder, its
/// records' lookups taken together, and what the folder's text states about
/// the row as a whole.
fn support_of(
    row: &Pressing,
    judgements: &Judgements,
    provenance: &HashMap<ReleaseKey, LookupProvenance>,
    ripped_from: RippedFrom,
) -> Support {
    let mut returned = LookupProvenance::CHOSEN;
    // A twin on the row states no lookup of its own: every field is false.
    for release in &row.releases {
        let found = provenance
            .get(&(release.source, release.release_id.clone()))
            .expect("a pressing is built from the run's own records");
        returned.by_disc_id |= found.by_disc_id;
        returned.by_barcode |= found.by_barcode;
        returned.by_catalog |= found.by_catalog;
        returned.by_search |= found.by_search;
    }
    let agreements = row.agreements(judgements);
    let offered = agreements.offered();
    Support {
        medium: ripped_from.admits(row.releases.iter().map(|release| &release.media)),
        lookups: [
            returned.by_disc_id,
            returned.by_barcode,
            returned.by_catalog,
            returned.by_search,
        ]
        .into_iter()
        .filter(|returned| *returned)
        .count() as u32,
        names_pressing: u32::from(agreements.catalog) + u32::from(returned.by_barcode && offered),
        shares_toc: returned.by_disc_id,
        offered,
    }
}

/// Split the ranked rows into the ones offered and the ones set aside, each
/// keeping the ranked order: the rows tied at the highest [`Support`] are
/// offered, and every other row is set aside. The medium is read first, so
/// the offered rows are ruled out only when every row is — which is the
/// conflict returned beside them.
fn split_rows(
    rows: Vec<Pressing>,
    judgements: &Judgements,
    provenance: &HashMap<ReleaseKey, LookupProvenance>,
    ripped_from: RippedFrom,
) -> (Vec<Pressing>, Vec<Pressing>, Option<super::MediumConflict>) {
    let support: Vec<Support> = rows
        .iter()
        .map(|row| support_of(row, judgements, provenance, ripped_from))
        .collect();
    let Some(best) = support.iter().copied().max() else {
        return (Vec::new(), Vec::new(), None);
    };
    let medium_conflict = if best.medium {
        None
    } else {
        ripped_from.conflict()
    };
    let mut offered = Vec::new();
    let mut set_aside = Vec::new();
    for (row, support) in rows.into_iter().zip(&support) {
        match *support == best {
            true => offered.push(row),
            false => set_aside.push(row),
        }
    }
    (offered, set_aside, medium_conflict)
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

#[cfg(test)]
#[path = "combine_evidence_tests.rs"]
mod evidence_tests;
