//! Ranks what a run's lookups returned and offers the best-supported rows.
//! Pure: no I/O, no state.
//!
//! Every record is first paired into pressing rows — two sources' records of
//! one object are one row, picked whole — and each row is scored by
//! `Support`. The rows tied at the top are offered; the rest are set aside
//! under "N more releases". A record read through another release's link
//! rather than returned by a lookup counts for no lookup.

use super::agreements::{agreements_of, CandidateText};
use super::medium::{agrees_with_mono, FolderAudio, RippedFrom};
use super::row_facts::{Fact, FolderFacts};
use crate::db::LibraryStatus;
use crate::import::album_links::Twin;
use crate::import::release_group::{group_results, Judged, Judgements, Pressing, ReleaseGroup};
use crate::import::search::MetadataResult;
use crate::import::Catalog;
use std::collections::{HashMap, HashSet};

/// Which lookups returned one result. Stored with the [`Findings`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LookupProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
    /// Never true beside the others: the title search runs only when the
    /// identifiers named nothing.
    pub by_search: bool,
    /// The MusicBrainz release that names this one as itself, when no lookup
    /// returned it.
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

/// What a run's lookups found once combined: the offered rows and the rows
/// set aside, each release with its lookups and its pressing row. A failed
/// run's findings are stored the same way as a found one's.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Findings {
    /// The offered rows' releases, most-agreed-with first.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`.
    pub provenance: Vec<LookupProvenance>,
    /// Index-aligned with `matches`: each release's pressing row, numbered
    /// from zero. Kept rather than re-formed, since grouping one list alone
    /// can merge records the run kept apart.
    pub pressings: Vec<u32>,
    pub narrowed_out: NarrowedOut,
    /// What the folder's files prove when they rule out every row. The rows
    /// are still offered, but nothing picks one unattended.
    pub medium_conflict: Option<super::MediumConflict>,
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
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NarrowedOut {
    /// In signal order, each release once.
    pub matches: Vec<MetadataResult>,
    /// Index-aligned with `matches`.
    pub provenance: Vec<LookupProvenance>,
    /// Index-aligned with `matches`: each release's row in this list.
    pub pressings: Vec<u32>,
}

impl NarrowedOut {
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
}

/// Whether each release a [`Findings`] names is already in the library,
/// index-aligned with its two lists. Checked live, never stored.
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

/// Combine each lookup's results into what the run found. An empty set takes
/// no part. `twins` are releases no lookup returned, each placed beside the
/// release that names it.
pub fn combine_results(
    discid_results: Results,
    barcode_results: Results,
    catalog_results: Results,
    search_results: Results,
    twins: Vec<Twin>,
    text: &CandidateText,
    folder: FolderAudio<'_>,
) -> (Findings, LibraryStatuses) {
    let ripped_from = RippedFrom::of(folder.origin, !discid_results.is_empty());
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

    // Every release once, in signal order, then the twins.
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
        split_rows(rows, &judgements, &returned_by, ripped_from, folder, text);

    let statuses: HashMap<ReleaseKey, LibraryStatus> = all
        .into_iter()
        .map(|(result, status)| ((result.source, result.release_id), status))
        .collect();
    // Each list's records in row order, with the row each belongs to.
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

/// What stands behind one row, compared field by field in declaration order.
/// The rows tied at the highest value are offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Support {
    /// Whether the row's carrier could have given the folder its audio — see
    /// [`RippedFrom::admits`]. First: a vinyl pressing every lookup returned
    /// is still not the CD the rip log was read off.
    medium: bool,
    /// How many of the run's lookups returned this row.
    lookups: u32,
    /// How many facts that name one pressing hold: the folder states its
    /// catalog number, and a barcode lookup returned it — the barcode only
    /// for a row the text also describes, since misread bars name some other
    /// record.
    names_pressing: u32,
    /// Whether the disc ID returned this row. Below what names one pressing,
    /// since every pressing cut from one master shares a table of contents.
    shares_toc: bool,
    /// How many of the album's title, artist and first year the folder's text
    /// states. They name the album, not the pressing, so they only tell apart
    /// a row returned for some other album. Agreements only: the text does not
    /// say which of its lines is the title or the artist, so a row's title it
    /// does not write contradicts nothing.
    names_album: u32,
    /// Whether nothing read says the row holds other tracks than the folder:
    /// a record of it lists as many tracks as the folder holds, as its full
    /// document reads against the folder's audio, or a record's tracklist is
    /// not read — a row whose document is not in, or could not be had, never
    /// loses to one whose is. Below the album's names, so a row for some
    /// other album that happens to hold as many tracks never passes the
    /// right album; above the channels and the country, because a different
    /// tracklist is a different edition, where those only say where or how
    /// one edition was cut.
    fits_the_tracks: bool,
    /// Whether the row was released as a download, where the folder is one —
    /// see [`super::medium::download`]. Above the edition year: a download is
    /// a copy of one digital release, where a year a catalog states is often
    /// the original's.
    download: Fact,
    /// Whether the row was released in the year the folder names its edition
    /// by — see [`FolderFacts::edition_year`].
    edition_year: Fact,
    /// Whether the row states mono and the folder's audio is one channel: a
    /// tiebreak only. Agreement only: catalogs list mono pressings as stereo,
    /// so a row stating stereo contradicts nothing.
    states_the_channels: bool,
    /// Whether the row was released where the folder's text says, which tells
    /// apart pressings a barcode names alike — see [`FolderFacts::country`].
    country: Fact,
    /// Whether the row was released where the folder's recordings were
    /// registered, as their ISRCs say. Below the folder naming the country:
    /// a recording is registered where its producer is, not where a copy was
    /// pressed, so it only tells apart rows nothing else does.
    registration: Fact,
    /// Whether anything but a barcode and a year stands behind the row — see
    /// [`super::agreements::Agreements::offered`]. One value, so a fact one
    /// pressing states does not split it from its siblings. The label and the
    /// catalog number count only here and toward `names_pressing`, as
    /// agreements: a folder does not say which of its words are its label or
    /// its number, and a label keeps a number across reissues, so a row
    /// stating another contradicts nothing.
    offered: bool,
}

/// What stands behind one row, over all its records.
fn support_of(
    row: &Pressing,
    judgements: &Judgements,
    provenance: &HashMap<ReleaseKey, LookupProvenance>,
    ripped_from: RippedFrom,
    folder: FolderAudio<'_>,
    text: &CandidateText,
    facts: &FolderFacts,
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
    // With no first year to weigh it against, a year the folder states stands
    // behind the row like any other fact of its pressing.
    let offered = agreements.offered()
        || (agreements.year && facts.album_first_year(&row.releases).is_none());
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
        names_album: agreements.names_album()
            + u32::from(facts.names_the_album_year(&row.releases)),
        fits_the_tracks: row
            .releases
            .iter()
            .any(|release| match release.source_tracks {
                None => true,
                Some(crate::import::search::SourceTracks::Listed { count }) => {
                    count == folder.track_count
                }
                Some(crate::import::search::SourceTracks::Nothing) => false,
            }),
        download: super::medium::download(
            folder.origin,
            row.releases.iter().map(|release| &release.media),
        ),
        edition_year: facts.edition_year(&row.releases),
        states_the_channels: agrees_with_mono(
            folder.mono,
            row.releases
                .iter()
                .flat_map(|release| &release.discogs_details),
        ),
        country: facts.country(&row.releases, text),
        registration: super::row_facts::registration(&row.releases, folder.registered_in),
        offered,
    }
}

/// Offer the rows tied at the highest [`Support`] and set the rest aside,
/// each in ranked order, with the medium conflict when every row fails it.
fn split_rows(
    rows: Vec<Pressing>,
    judgements: &Judgements,
    provenance: &HashMap<ReleaseKey, LookupProvenance>,
    ripped_from: RippedFrom,
    folder: FolderAudio<'_>,
    text: &CandidateText,
) -> (Vec<Pressing>, Vec<Pressing>, Option<super::MediumConflict>) {
    let facts = FolderFacts::of(text, rows.iter().flat_map(|row| &row.releases));
    let support: Vec<Support> = rows
        .iter()
        .map(|row| {
            support_of(
                row,
                judgements,
                provenance,
                ripped_from,
                folder,
                text,
                &facts,
            )
        })
        .collect();
    let Some(best) = support.iter().copied().max() else {
        return (Vec::new(), Vec::new(), None);
    };
    // The best row failing the medium means every row does.
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

/// Every release any set names, once, in signal order.
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

#[cfg(test)]
#[path = "combine_year_tests.rs"]
mod year_tests;
