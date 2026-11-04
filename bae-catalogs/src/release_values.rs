//! What one reading of a catalog release says a release is, as plain values.
//!
//! A catalog release laid against the folder's audio
//! (`SourceRelease::values`) reads as the album, the pressing, one track per
//! track of the audio in play order, and the roles and works the source
//! credits — values with no ids and no pointer back into the source.

use crate::credits::{ArtistCredit, CreditedRole, SourceCredit};
use crate::source_release::{PartDirection, SourceWork, SourceWorkEvent};
use crate::works::{GraphWork, WorkGraph, WorkPart};
use bae_types::pressing::Pressing;
use std::collections::HashSet;

/// A release as one source reads it, laid onto the audio's tracks.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseValues {
    pub album_title: String,
    /// The album's artists, in credit order, with what the source prints
    /// after each.
    pub album_credits: Vec<SourceCredit>,
    pub album_year: Option<i32>,
    pub pressing: Pressing,
    /// The mediums the audio is on, in order.
    pub mediums: Vec<bae_types::mediums::ReleaseMedium>,
    /// Who the source credits in a role on the release itself.
    pub release_roles: Vec<CreditedRole>,
    /// Every work a track performs, once.
    pub works: WorkGraph,
    /// One per track of the audio, in play order.
    pub tracks: Vec<TrackValues>,
}

/// One track as its source reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackValues {
    pub title: String,
    /// Where it sits on the reading's mediums.
    pub place: bae_types::mediums::TrackPlace,
    /// The length the source lists for the track.
    pub stated_length_ms: Option<u64>,
    pub credits: TrackCredits,
    pub roles: Vec<CreditedRole>,
    /// The MusicBrainz ids of the works it performs, in its order.
    pub works: Vec<String>,
}

/// Who a source credits a track to.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackCredits {
    /// The album's artists: the source names none of the track's own.
    AlbumArtists,
    /// The track's own artists, in credit order.
    Own(Vec<SourceCredit>),
}

/// The track number a catalog position prints: the digits after its last
/// `-` (`2-7`, `CD1-7`, `BD-7`), else after its leading letters (`A7`,
/// `DVD7`), else the whole position (`7`). `None` for a position whose number
/// is not all digits (`1-10a`, a sub-track) or is zero.
pub(crate) fn position_number(position: &str) -> Option<std::num::NonZeroU32> {
    let number = match position.rsplit_once('-') {
        Some((_, number)) => number,
        None => position.trim_start_matches(|c: char| c.is_ascii_alphabetic()),
    };
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

/// The side a position on a record or a cassette names: one letter, alone or
/// before a track number (`A`, `A1`, `b2`, `A1a`, `B1ii`), uppercased. Two
/// letters (`AA1`, `CD1`) name no side.
pub(crate) fn side_letter(position: &str) -> Option<u8> {
    let (&letter, rest) = position.as_bytes().split_first()?;
    (letter.is_ascii_alphabetic() && (rest.is_empty() || is_track_number(&position[1..])))
        .then(|| letter.to_ascii_uppercase())
}

/// A track number, with the sub-track letters a source may print after it
/// (`7`, `10a`, `1ii`).
pub(crate) fn is_track_number(text: &str) -> bool {
    let digits_end = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
    digits_end > 0 && text[digits_end..].bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// The side each of one medium's tracks is on, from the [`side_letter`]
/// each one's position names, in track order: counted from the medium's
/// lowest letter, so a second record lettered C/D has sides 1 and 2 of its
/// own, and a track naming none on the side of the track before it (the
/// first side for one leading the medium). `None` when no track names a
/// letter.
pub(crate) fn sides_by_letter(letters: &[Option<u8>]) -> Option<Vec<std::num::NonZeroU8>> {
    let base = letters.iter().flatten().copied().min()?;
    let mut current = std::num::NonZeroU8::MIN;
    Some(
        letters
            .iter()
            .map(|letter| {
                if let Some(letter) = letter {
                    current = bae_types::mediums::side_number(usize::from(letter - base));
                }
                current
            })
            .collect(),
    )
}

/// Builds a [`WorkGraph`] from the works a release's recordings perform.
/// Every reference to a MusicBrainz work carries its whole sub-graph; each
/// work is held once and its relations read once, however many tracks or
/// parents reference it.
#[derive(Default)]
pub(crate) struct WorkGraphBuilder {
    graph: WorkGraph,
    /// The works whose relations have been read.
    expanded: HashSet<String>,
}

impl WorkGraphBuilder {
    /// Add `work`, which a track performs, and return its id.
    pub(crate) fn performed(&mut self, work: &SourceWork) -> String {
        self.add(work, None)
    }

    pub(crate) fn finish(self) -> WorkGraph {
        self.graph
    }

    /// Hold `work` once, under `parent` when it is a part of one, then read
    /// its relations once. A work with no relations reads nothing, so a later
    /// reference that carries them still adds them.
    fn add(&mut self, work: &SourceWork, parent: Option<&str>) -> String {
        let id = work.musicbrainz_work_id.clone();
        if !self.graph.works.iter().any(|held| held.musicbrainz_work_id == id) {
            self.graph.works.push(GraphWork {
                musicbrainz_work_id: id.clone(),
                title: work.title.clone(),
                disambiguation: work.disambiguation.clone(),
                work_type: work.work_type.clone(),
                composers: Vec::new(),
            });
        }
        if let Some(parent) = parent {
            self.link(parent, &id);
        }
        if work.events.is_empty() || !self.expanded.insert(id.clone()) {
            return id;
        }
        for event in &work.events {
            match event {
                SourceWorkEvent::Composer(composer) => self.compose(&id, composer),
                SourceWorkEvent::Part { direction: PartDirection::Backward, work: parent } => {
                    let parent = self.add(parent, None);
                    self.link(&parent, &id);
                }
                SourceWorkEvent::Part { direction: PartDirection::Forward, work: child } => {
                    self.add(child, Some(&id));
                }
            }
        }
        id
    }

    fn link(&mut self, parent: &str, child: &str) {
        if self.graph.parts.iter().any(|part| part.parent == parent && part.child == child) {
            return;
        }
        self.graph.parts.push(WorkPart { parent: parent.to_string(), child: child.to_string() });
    }

    /// Credit `composer` on the work once. An id-less MusicBrainz credit is
    /// the same composer as an id-less one of the same name only.
    fn compose(&mut self, work_id: &str, composer: &ArtistCredit) {
        let work = self
            .graph
            .works
            .iter_mut()
            .find(|work| work.musicbrainz_work_id == work_id)
            .expect("a work is held before its relations are read");
        let held = work.composers.iter().any(|known| {
            match (&known.musicbrainz_artist_id, &composer.musicbrainz_artist_id) {
                (Some(known), Some(composer)) => known == composer,
                (None, None) => {
                    bae_types::text_match::same_artist_name(&known.name, &composer.name)
                }
                (Some(_), None) | (None, Some(_)) => false,
            }
        });
        if !held {
            work.composers.push(composer.clone());
        }
    }
}

/// How each of `values`' tracks reads on its mediums: `A1` on a lettered
/// side, `2-3` for track 3 of disc 2, `5` on a release of one medium. What a
/// reader's test checks places with.
#[cfg(any(test, feature = "test-utils"))]
pub fn place_labels(
    mediums: &[bae_types::mediums::ReleaseMedium],
    places: impl IntoIterator<Item = bae_types::mediums::TrackPlace>,
) -> Vec<String> {
    use bae_types::mediums::TrackPosition;
    places
        .into_iter()
        .map(|place| match bae_types::mediums::track_position(&place, mediums) {
            TrackPosition::Sided { side_letter, number } => format!("{side_letter}{number}"),
            TrackPosition::Disc { disc, number } => format!("{disc}-{number}"),
            TrackPosition::Flat { number } => number.to_string(),
        })
        .collect()
}

/// [`place_labels`] of a reading's tracks.
#[cfg(any(test, feature = "test-utils"))]
pub fn value_labels(values: &ReleaseValues) -> Vec<String> {
    place_labels(&values.mediums, values.tracks.iter().map(|track| track.place))
}

#[cfg(test)]
#[path = "release_values_tests.rs"]
mod tests;
