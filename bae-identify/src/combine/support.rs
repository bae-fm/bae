//! How each row is scored against the folder: the facts it states that the
//! folder confirms or rules out, the title, year and notes that break a tie,
//! and the split of the rows into those offered and those set aside.

use super::*;

/// What one row states against the folder, compared field by field in
/// declaration order. The rows tied at the highest value are offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Support {
    /// Whether the row's carrier could have given the folder its audio — see
    /// [`RippedFrom::admits`]. First: a vinyl pressing every lookup returned
    /// is still not the CD the rip log was read off.
    medium: bool,
    /// How many facts that name one pressing hold: the folder's text prints
    /// its catalog number, and it prints a barcode the folder's files carry —
    /// the barcode only for a row the text also describes, since misread bars
    /// name some other record. Which searches returned the row adds nothing:
    /// a search only brings releases in.
    names_pressing: u32,
    /// How many of the folder's discs the row holds: how many of its disc
    /// IDs' lookups returned the row's release, so a release holding every
    /// disc of the folder ranks above one holding only some. No record states
    /// the disc IDs it carries, so a row no lookup returned — a Discogs one,
    /// which holds none — states nothing about them rather than others.
    /// Below what names one pressing, since every pressing cut from one
    /// master shares a table of contents.
    shares_toc: u32,
    /// How many of the album's title, artist and first year the folder's text
    /// states, and whether its recordings are the folder's, as the search by
    /// the audio's ISRCs says. They name the album, not the pressing, so they
    /// only tell apart a row returned for some other album. Agreements only:
    /// the text does not say which of its lines is the title or the artist,
    /// so a row's title it does not write contradicts nothing.
    names_album: u32,
    /// Whether the row states tracks the folder could be, by
    /// [`crate::fit::TracklistFit::admits`] of the row's lead — the record
    /// that fits best. A row that lists other tracks than the folder is not
    /// here at all (see [`crate::fit::rules_out`]), so what this sets below
    /// is a row whose records list no tracks: it proves nothing, and ranks
    /// under one whose tracklist fits or is not read yet. A row whose
    /// document is not in, or could not be had, never loses to one whose is.
    /// Below the album's names, so a row for some other album that happens
    /// to hold as many tracks never passes the right album; above the
    /// channels and the country, which only say where or how one edition was
    /// cut.
    fits_the_tracks: bool,
    /// Whether the titles a row's read document lists run in the order the
    /// folder's do — see [`crate::row_facts::track_titles`]. Agreeing counts
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
    /// see [`crate::medium::download`]. Above the edition year: a download is
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
    /// [`crate::agreements::Agreements::offered`]. One value, so a fact one
    /// pressing states does not split it from its siblings. The label and the
    /// catalog number count only here and toward `names_pressing`, as
    /// agreements: a folder does not say which of its words are its label or
    /// its number, and a label keeps a number across reissues, so a row
    /// stating another contradicts nothing.
    offered: bool,
    /// Whether any of the row's records states a release year. Below every
    /// fact the folder states: where none of those tells look-alike rows
    /// apart, the person picks the entry that tells them the most, and an
    /// undated duplicate of a dated entry is never that pick. Above the
    /// notes: a year is a field the row states and shows, where the notes
    /// point rests on one word of free text the folder may write by chance,
    /// so the notes are weighed only among the rows the year leaves tied.
    states_a_year: bool,
    /// Whether the folder's text names a word only this row's notes write
    /// among the rows tied with it on every field above — see
    /// [`crate::notes`] and [`weigh_notes`]. Last, so it only breaks a tie:
    /// words shared by every tied row cancel out, and a word matched by chance
    /// costs only the order of rows that were equal anyway.
    names_what_sets_it_apart: bool,
}

/// What stands behind one row, over all its records.
fn support_of(
    row: &Pressing,
    judgements: &Judgements,
    by_isrc: &HashSet<ReleaseKey>,
    discs_held: &HashMap<ReleaseKey, u32>,
    ripped_from: RippedFrom,
    folder: FolderAudio<'_>,
    facts: &FolderFacts,
) -> Support {
    let returned_by_isrc = row
        .releases
        .iter()
        .any(|release| by_isrc.contains(&(release.source, release.release_id.clone())));
    // A row holds one record per catalog, and only MusicBrainz answers disc
    // IDs, so at most one of its records holds any.
    let shares_toc = row
        .releases
        .iter()
        .filter_map(|release| discs_held.get(&(release.source, release.release_id.clone())))
        .copied()
        .max()
        .unwrap_or(0);
    let agreements = row.agreements(judgements);
    let offered = agreements.offered();
    Support {
        medium: ripped_from.admits(row.releases.iter().map(|release| &release.media)),
        names_pressing: u32::from(agreements.catalog) + u32::from(agreements.barcode && offered),
        shares_toc,
        names_album: agreements.names_album()
            + u32::from(facts.names_the_album_year(&row.releases))
            + u32::from(returned_by_isrc),
        fits_the_tracks: crate::fit::TracklistFit::of(
            row.lead().source_tracks.as_ref(),
            folder.track_count,
        )
        .admits(),
        track_titles: crate::row_facts::track_titles(&row.releases, folder.track_titles),
        download: crate::medium::download(
            folder.origin,
            row.releases.iter().map(|release| &release.media),
        ),
        edition_year: facts.edition_year(&row.releases),
        states_the_channels: agrees_with_mono(
            folder.mono,
            row.releases.iter().flat_map(|release| &release.discogs_details),
        ),
        country: facts.country(&row.releases),
        registration: crate::row_facts::registration(&row.releases, folder.registered_in),
        offered,
        states_a_year: row.releases.iter().any(|release| release.year.is_some()),
        // Weighed once every row's other fields are.
        names_what_sets_it_apart: false,
    }
}

/// Offer the rows tied at the highest [`Support`] and set the rest aside,
/// each in ranked order, with the medium conflict when every row fails it.
pub(super) fn split_rows(
    rows: Vec<Pressing>,
    judgements: &Judgements,
    by_isrc: &HashSet<ReleaseKey>,
    discs_held: &HashMap<ReleaseKey, u32>,
    ripped_from: RippedFrom,
    folder: FolderAudio<'_>,
    text: &CandidateText,
    facts: &FolderFacts,
) -> (Vec<Pressing>, Vec<Pressing>, Option<crate::MediumConflict>, HashMap<ReleaseKey, String>) {
    let mut support: Vec<Support> = rows
        .iter()
        .map(|row| support_of(row, judgements, by_isrc, discs_held, ripped_from, folder, facts))
        .collect();
    weigh_title_agreement(&rows, &mut support);
    let notes = weigh_notes(&rows, &mut support, text);
    let Some(best) = support.iter().copied().max() else {
        return (Vec::new(), Vec::new(), None, notes);
    };
    // The best row failing the medium means every row does.
    let medium_conflict = if best.medium { None } else { ripped_from.conflict() };
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
    /// The fields declared above `names_what_sets_it_apart`, the notes as
    /// though no row had them.
    fn above_notes(&self) -> Self {
        Self { names_what_sets_it_apart: false, ..*self }
    }

    /// The fields declared above `track_titles`: what a row ties with the
    /// others on before its titles are weighed.
    fn above_track_titles(&self) -> (bool, u32, u32, u32, bool) {
        (self.medium, self.names_pressing, self.shares_toc, self.names_album, self.fits_the_tracks)
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
            && row.releases.iter().all(|release| release.track_titles.is_empty())
    });
    if undecided {
        for support in support.iter_mut().filter(|support| support.track_titles == Fact::Agrees) {
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
    let tied: Vec<usize> =
        (0..rows.len()).filter(|row| support[*row].above_notes() == best).collect();
    let named =
        crate::notes::named_notes(tied.iter().map(|row| rows[*row].releases.as_slice()), text);
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

#[cfg(test)]
#[path = "support_tests.rs"]
mod tests;
