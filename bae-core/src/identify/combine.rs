//! Ranks what a run's lookups returned and offers the best-supported rows.
//! Pure: no I/O, no state.
//!
//! Every record is first paired into pressing rows — two sources' records of
//! one object are one row, picked whole. A row whose read tracklist holds
//! other tracks than the folder is not a match and is left out (see
//! `fit::rules_out`); each row left is scored by `Support`. The rows
//! tied at the top are offered; the rest are set aside under "N more
//! releases". A record read through another release's link rather than
//! returned by a lookup counts for no lookup.

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
    /// The search for the recordings the audio's ISRCs are registered to.
    pub by_isrc: bool,
    /// Never true beside the disc ID, the barcode or the catalog number: the
    /// title search runs only when those named nothing.
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
        by_isrc: false,
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
    /// The note of each offered release that writes a word the folder states
    /// and no other row tied with its row writes — why the ranking's notes
    /// point went to its row (see `identify::notes`), and what the Notes badge
    /// shows. Kept rather than read again, since which rows it was weighed
    /// among is the ranking's alone. Empty where the folder named none.
    pub named_notes: Vec<NamedNote>,
}

/// A note of an offered release the folder's text names its row by.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NamedNote {
    pub release: crate::import::MetadataRef,
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

/// What each of a run's lookups returned, each release with its library
/// status.
#[derive(Debug, Clone, Default)]
pub struct LookupAnswers {
    pub disc_id: Results,
    pub barcode: Results,
    pub catalog: Results,
    pub isrc: Results,
    pub search: Results,
}

impl LookupAnswers {
    /// Every lookup's results, in the order above.
    pub(crate) fn all(&self) -> impl Iterator<Item = &(MetadataResult, LibraryStatus)> {
        self.disc_id
            .iter()
            .chain(&self.barcode)
            .chain(&self.catalog)
            .chain(&self.isrc)
            .chain(&self.search)
    }
}

/// Combine each lookup's results into what the run found. An empty set takes
/// no part. `twins` are releases no lookup returned, each placed beside the
/// release that names it.
pub fn combine_results(
    answers: LookupAnswers,
    twins: Vec<Twin>,
    text: &CandidateText,
    folder: FolderAudio<'_>,
) -> (Findings, LibraryStatuses) {
    let ripped_from = RippedFrom::of(folder.origin, !answers.disc_id.is_empty());
    let by_signal = [
        &answers.disc_id,
        &answers.barcode,
        &answers.catalog,
        &answers.isrc,
        &answers.search,
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
            by_isrc: keys[3].contains(&key),
            by_search: keys[4].contains(&key),
            named_by: named_by.get(&key).cloned(),
        }
    };
    let facts = FolderFacts::of(text, all.iter().map(|(result, _)| result));
    let judged: Vec<Judged> = all
        .iter()
        .map(|(result, _)| {
            let agreements = agreements_of(result, text, &facts, &lookup_of(result));
            (result.clone(), agreements)
        })
        .collect();
    let judgements = Judgements::of(&judged, Some(folder.track_count));
    let returned_by: HashMap<ReleaseKey, LookupProvenance> = all
        .iter()
        .map(|(result, _)| {
            (
                (result.source, result.release_id.clone()),
                lookup_of(result),
            )
        })
        .collect();
    // Neither offered nor set aside: a row the folder cannot be is no answer.
    let rows: Vec<Pressing> = group_results(judged, Some(folder.track_count))
        .into_iter()
        .flat_map(ReleaseGroup::into_pressings)
        .filter(|row| !super::fit::rules_out(&row.releases, folder.track_count))
        .collect();
    let in_library: HashSet<ReleaseKey> = all
        .iter()
        .filter(|(_, status)| status.release_in_library)
        .map(|(result, _)| (result.source, result.release_id.clone()))
        .collect();
    let (offered, set_aside, medium_conflict, notes) = split_rows(
        rows,
        &judgements,
        &returned_by,
        ripped_from,
        folder,
        text,
        &facts,
    );
    // Offered rows that would look alike on screen are listed once.
    let offered =
        super::look_alike::keep_one_of_each_look(offered, &judgements, &notes, &in_library);

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
    let named_notes = combined
        .iter()
        .filter_map(|(result, _)| {
            notes
                .get(&(result.source, result.release_id.clone()))
                .map(|note| NamedNote {
                    release: crate::import::MetadataRef::new(result.source, &result.release_id),
                    note: note.clone(),
                })
        })
        .collect();
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
            named_notes,
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
    /// Whether the row states tracks the folder could be, by
    /// [`super::fit::TracklistFit::admits`] of the row's lead — the record
    /// that fits best. A row that lists other tracks than the folder is not
    /// here at all (see [`super::fit::rules_out`]), so what this sets below
    /// is a row whose records list no tracks: it proves nothing, and ranks
    /// under one whose tracklist fits or is not read yet. A row whose
    /// document is not in, or could not be had, never loses to one whose is.
    /// Below the album's names, so a row for some other album that happens
    /// to hold as many tracks never passes the right album; above the
    /// channels and the country, which only say where or how one edition was
    /// cut.
    fits_the_tracks: bool,
    /// Whether the titles a row's read document lists run in the order the
    /// folder's do — see [`super::row_facts::track_titles`]. Agreeing counts
    /// only where every row tied with it above lists titles to compare — see
    /// [`weigh_title_agreement`].
    ///
    /// Directly below the track count: a row whose tracks number other than
    /// the folder's is already another edition, and the titles' order only
    /// tells apart editions that hold the same tracks, such as one that runs
    /// them in another order; above the download, the year and the country,
    /// which only say how or where one tracklist was issued.
    track_titles: Fact,
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
    /// Whether anything but a barcode stands behind the row — see
    /// [`super::agreements::Agreements::offered`]. One value, so a fact one
    /// pressing states does not split it from its siblings. The label and the
    /// catalog number count only here and toward `names_pressing`, as
    /// agreements: a folder does not say which of its words are its label or
    /// its number, and a label keeps a number across reissues, so a row
    /// stating another contradicts nothing.
    offered: bool,
    /// Whether the folder's text names a word only this row's notes write
    /// among the rows tied with it on every field above — see
    /// [`super::notes`] and [`weigh_notes`]. Last, so it only breaks a tie:
    /// words shared by every tied row cancel out, and a word matched by chance
    /// costs only the order of rows that were equal anyway.
    names_what_sets_it_apart: bool,
    /// Whether any of the row's records states a release year. Last: where
    /// nothing the folder says decides between look-alike rows, the person
    /// picks the entry that tells them the most, and an undated duplicate of
    /// a dated entry is never that pick.
    states_a_year: bool,
}

/// What stands behind one row, over all its records.
fn support_of(
    row: &Pressing,
    judgements: &Judgements,
    provenance: &HashMap<ReleaseKey, LookupProvenance>,
    ripped_from: RippedFrom,
    folder: FolderAudio<'_>,
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
        returned.by_isrc |= found.by_isrc;
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
            returned.by_isrc,
            returned.by_search,
        ]
        .into_iter()
        .filter(|returned| *returned)
        .count() as u32,
        names_pressing: u32::from(agreements.catalog) + u32::from(returned.by_barcode && offered),
        shares_toc: returned.by_disc_id,
        names_album: agreements.names_album()
            + u32::from(facts.names_the_album_year(&row.releases)),
        fits_the_tracks: super::fit::TracklistFit::of(
            row.lead().source_tracks.as_ref(),
            folder.track_count,
        )
        .admits(),
        track_titles: super::row_facts::track_titles(&row.releases, folder.track_titles),
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
        country: facts.country(&row.releases),
        registration: super::row_facts::registration(&row.releases, folder.registered_in),
        offered,
        // Weighed once every row's other fields are.
        names_what_sets_it_apart: false,
        states_a_year: row.releases.iter().any(|release| release.year.is_some()),
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
    facts: &FolderFacts,
) -> (
    Vec<Pressing>,
    Vec<Pressing>,
    Option<super::MediumConflict>,
    HashMap<ReleaseKey, String>,
) {
    let mut support: Vec<Support> = rows
        .iter()
        .map(|row| support_of(row, judgements, provenance, ripped_from, folder, facts))
        .collect();
    weigh_title_agreement(&rows, &mut support);
    let notes = weigh_notes(&rows, &mut support, text);
    let Some(best) = support.iter().copied().max() else {
        return (Vec::new(), Vec::new(), None, notes);
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
    (offered, set_aside, medium_conflict, notes)
}

impl Support {
    /// The fields declared above `names_what_sets_it_apart`, the rest as
    /// though no row had them.
    fn above_notes(&self) -> Self {
        Self {
            names_what_sets_it_apart: false,
            states_a_year: false,
            ..*self
        }
    }

    /// The fields declared above `track_titles`: what a row ties with the
    /// others on before its titles are weighed.
    fn above_track_titles(&self) -> (bool, u32, u32, bool, u32, bool) {
        (
            self.medium,
            self.lookups,
            self.names_pressing,
            self.shares_toc,
            self.names_album,
            self.fits_the_tracks,
        )
    }
}

/// Titles in the folder's order lift a row above the rows it ties with only
/// where each of those lists titles to compare. A run reads only some rows'
/// documents, and a row it left unread, or read with a track untitled, cannot
/// be told to agree or not — which rows were read says nothing about them.
/// Where one of the rows tied at the top lists none, agreeing counts for
/// nothing, and only a row listing the folder's titles in another order is
/// set below the rest.
fn weigh_title_agreement(rows: &[Pressing], support: &mut [Support]) {
    let Some(best) = support.iter().map(Support::above_track_titles).max() else {
        return;
    };
    let undecided = rows.iter().zip(support.iter()).any(|(row, support)| {
        support.above_track_titles() == best
            && row
                .releases
                .iter()
                .all(|release| release.track_titles.is_empty())
    });
    if undecided {
        for support in support
            .iter_mut()
            .filter(|support| support.track_titles == Fact::Agrees)
        {
            support.track_titles = Fact::StatesNothing;
        }
    }
}

/// Give each row tied at the top on every field above the notes the point
/// for the folder naming what sets it apart from the others tied there, and
/// return the note of each of their records it names. The rows below are out
/// already, and what their notes share with these says nothing about which
/// of these is on the desk.
fn weigh_notes(
    rows: &[Pressing],
    support: &mut [Support],
    text: &CandidateText,
) -> HashMap<ReleaseKey, String> {
    let mut notes = HashMap::new();
    let Some(best) = support.iter().map(Support::above_notes).max() else {
        return notes;
    };
    let tied: Vec<usize> = (0..rows.len())
        .filter(|row| support[*row].above_notes() == best)
        .collect();
    let named =
        super::notes::named_notes(tied.iter().map(|row| rows[*row].releases.as_slice()), text);
    for (row, named) in tied.into_iter().zip(named) {
        for (release, note) in rows[row].releases.iter().zip(named) {
            if let Some(note) = note {
                support[row].names_what_sets_it_apart = true;
                notes.insert((release.source, release.release_id.clone()), note);
            }
        }
    }
    notes
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

#[cfg(test)]
#[path = "combine_notes_tests.rs"]
mod notes_tests;
