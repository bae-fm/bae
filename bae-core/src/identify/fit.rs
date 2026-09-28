//! Whether what a run found fits the folder, decided once.
//!
//! [`TracklistFit`] is the one reading of a record's tracklist against the
//! folder's audio, and [`unattended_pick`] the one rule for whether a found
//! verdict picks its release without a person: the release the settle applies
//! to the draft is the release automatic import takes, and the reason it
//! picks none is the check the pane states.

use super::verdict_summary::FolderCheck;
use super::MediumConflict;
use crate::import::search::{MetadataResult, SourceTracks};

/// What a record's tracklist, as its source lists it for the folder's audio,
/// says about the folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracklistFit {
    /// It lists as many tracks as the folder holds.
    Fits,
    /// Its document is not read, so it lists nothing to count yet.
    Unread,
    /// It lists another number of tracks than the folder holds.
    Disagrees { source: u32 },
    /// Its source answered and lists no tracks for this audio.
    ListsNothing,
}

impl TracklistFit {
    /// The count, never the lengths. A source's lengths are whatever it
    /// transcribed — rounded to whole seconds, counted with or without a
    /// pre-gap, missing for some tracks — so disagreeing lengths say more
    /// about the source than about the match. The mapping pane shows both
    /// durations per row for a person who wants to read them.
    pub fn of(source_tracks: Option<&SourceTracks>, folder_track_count: u32) -> Self {
        match source_tracks {
            None => Self::Unread,
            Some(SourceTracks::Nothing) => Self::ListsNothing,
            Some(SourceTracks::Listed { count }) if *count == folder_track_count => Self::Fits,
            Some(SourceTracks::Listed { count }) => Self::Disagrees { source: *count },
        }
    }

    /// Whether nothing read rules the record out. A record whose document is
    /// not in stands beside one whose document fits: what is offered errs on
    /// showing a release, and only a fit that was read is ever picked.
    pub fn admits(self) -> bool {
        match self {
            Self::Fits | Self::Unread => true,
            Self::Disagrees { .. } | Self::ListsNothing => false,
        }
    }

    /// Where the record stands among its pressing's records as the one the
    /// draft is read from: one that fits, then one not read yet, then one
    /// that cannot be laid onto the folder's audio. A pressing whose lead is
    /// ruled out has no record `admits`.
    pub(crate) fn lead_rank(self) -> u8 {
        match self {
            Self::Fits => 0,
            Self::Unread => 1,
            Self::Disagrees { .. } | Self::ListsNothing => 2,
        }
    }
}

/// Why a found verdict picks none of its releases unattended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declined {
    /// It found no release.
    NothingFound,
    /// It found several pressings: picking between them is the person's.
    Several,
    /// A record of its one pressing could not be read in full, so what it
    /// states was never checked.
    UnreadDocument,
    /// A check of its release against the folder failed.
    FolderCheck(FolderCheck),
}

/// The one pressing a found verdict picks: its records, the one the draft is
/// read from first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnattendedPick<'a> {
    pub records: &'a [MetadataResult],
}

impl UnattendedPick<'_> {
    pub fn pressing(&self) -> crate::import::release_group::Pressing {
        crate::import::release_group::Pressing {
            releases: self.records.to_vec(),
        }
    }
}

/// What a found verdict picks for the folder without a person, or why it
/// picks nothing. `matches` are its offered records in row order, the lead of
/// each row first, and `rows` the row each belongs to.
pub fn unattended_pick<'a>(
    matches: &'a [MetadataResult],
    rows: &[u32],
    medium_conflict: Option<MediumConflict>,
    folder_track_count: u32,
) -> Result<UnattendedPick<'a>, Declined> {
    let declined = decline(
        medium_conflict,
        crate::import::release_group::row_count(rows),
        matches
            .first()
            .map(|lead| TracklistFit::of(lead.source_tracks.as_ref(), folder_track_count)),
        matches
            .iter()
            .any(|record| record.document_failure.is_some()),
        folder_track_count,
    );
    match declined {
        Some(declined) => Err(declined),
        None => Ok(UnattendedPick { records: matches }),
    }
}

/// Why a found verdict picks nothing, or `None` when it picks its one
/// pressing: `pressing_count` rows found, the lead record's fit, and whether
/// any record's document went unread.
///
/// "An exact signal is not the same as a unique result" — a disc ID or a
/// barcode routinely returns several pressings of one release group, and
/// picking between them is the person's job. Two sources' records of the same
/// pressing are not that choice: they are one row, so they count once.
pub(crate) fn decline(
    medium_conflict: Option<MediumConflict>,
    pressing_count: usize,
    lead: Option<TracklistFit>,
    unread_document: bool,
    folder_track_count: u32,
) -> Option<Declined> {
    // A release the folder's own files rule out is never picked, however well
    // the lookups agree on it: the person reads the evidence and picks, or
    // does not. Checked before the pressing count, so it is named over
    // several pressings too.
    if let Some(folder) = medium_conflict {
        return Some(Declined::FolderCheck(FolderCheck::MediumDisagrees {
            folder,
        }));
    }
    let Some(lead) = lead else {
        return Some(Declined::NothingFound);
    };
    if pressing_count != 1 {
        return Some(Declined::Several);
    }
    match (lead, unread_document) {
        (TracklistFit::Disagrees { source }, _) => {
            Some(Declined::FolderCheck(FolderCheck::TrackCountDisagrees {
                local: folder_track_count,
                source,
            }))
        }
        (_, true) => Some(Declined::UnreadDocument),
        (TracklistFit::Fits, false) => None,
        // Not read, or read and listing nothing: the count is unchecked, and
        // a release is never admitted unverified.
        (TracklistFit::Unread | TracklistFit::ListsNothing, false) => {
            Some(Declined::FolderCheck(FolderCheck::SourceTracksUnknown))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Catalog;

    fn listing(release_id: &str, count: u32) -> MetadataResult {
        MetadataResult {
            source_tracks: Some(SourceTracks::Listed { count }),
            ..MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some("g"))
        }
    }

    /// The pick is the pressing itself, lead first: what the settle applies
    /// is what the rule judged.
    #[test]
    fn a_sole_fitting_pressing_is_picked_whole() {
        let records = vec![
            listing("mb-1", 11),
            MetadataResult::for_test(Catalog::Discogs, "dg-1", None),
        ];
        let pick = unattended_pick(&records, &[0, 0], None, 11).expect("it fits");
        assert_eq!(pick.records, records.as_slice());
        assert_eq!(pick.pressing().lead().release_id, "mb-1");
    }

    /// A count that disagrees is the reason named, even beside a document
    /// that went unread; an unread document is named over a count nobody
    /// could check.
    #[test]
    fn a_disagreeing_count_is_named_before_an_unread_document() {
        let mut unread = listing("mb-1", 12);
        unread.document_failure = Some(crate::signals::LookupFailure::Network);
        assert_eq!(
            unattended_pick(std::slice::from_ref(&unread), &[0], None, 11),
            Err(Declined::FolderCheck(FolderCheck::TrackCountDisagrees {
                local: 11,
                source: 12
            }))
        );
        unread.source_tracks = None;
        assert_eq!(
            unattended_pick(std::slice::from_ref(&unread), &[0], None, 11),
            Err(Declined::UnreadDocument)
        );
    }
}
