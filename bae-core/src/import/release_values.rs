//! What one reading of a source says a release is, as plain values.
//!
//! Each source has one reader: a catalog release laid against the folder's
//! audio (`SourceRelease::values`), the files' tags (`file_tag_mapper`), or
//! nothing. Its output is the album, the pressing, one track per track of the
//! audio in play order, and the roles and works the source credits — values
//! with no ids and no pointer back into the source. Applying it writes them
//! into a candidate's draft ([`ReleaseValues::candidate_draft`]); the library
//! editor's reset projects the editable ones ([`ReleaseValues::user_edit`]).

use crate::import::source_release::{PartDirection, SourceWork, SourceWorkEvent};
use crate::import::{
    ArtistAssignment, ArtistCredit, AudioFile, CandidateDraft, CandidateTrack, CreditedRole,
    GraphWork, RawPressingEdit, RawTrackEdit, ReleaseUserEdit, TrackArtistAssignments,
    TrackUserEdit, WorkGraph, WorkPart,
};
use crate::pressing::Pressing;
use std::collections::HashSet;

/// A release as one source reads it, laid onto the audio's tracks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReleaseValues {
    pub album_title: String,
    /// The album's artists, in credit order.
    pub album_credits: Vec<ArtistCredit>,
    pub album_year: Option<i32>,
    pub pressing: Pressing,
    /// Who the source credits in a role on the release itself.
    pub release_roles: Vec<CreditedRole>,
    /// Every work a track performs, once.
    pub works: WorkGraph,
    /// One per track of the audio, in play order.
    pub tracks: Vec<TrackValues>,
}

/// One track as its source reads it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TrackValues {
    pub title: String,
    pub side: Option<i32>,
    /// The number the source states, `None` where it states none usable.
    pub number: Option<i32>,
    /// The length the source lists for the track.
    pub stated_length_ms: Option<u64>,
    pub credits: TrackCredits,
    pub roles: Vec<CreditedRole>,
    /// The MusicBrainz ids of the works it performs, in its order.
    pub works: Vec<String>,
}

/// Who a source credits a track to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TrackCredits {
    /// The album's artists: the source names none of the track's own.
    AlbumArtists,
    /// The track's own artists, in credit order.
    Own(Vec<ArtistCredit>),
}

impl TrackCredits {
    fn assignments(&self) -> TrackArtistAssignments {
        match self {
            Self::AlbumArtists => TrackArtistAssignments::AlbumArtists,
            Self::Own(credits) => TrackArtistAssignments::Explicit(
                credits.iter().cloned().map(credit_assignment).collect(),
            ),
        }
    }
}

fn credit_assignment(credit: ArtistCredit) -> ArtistAssignment {
    ArtistAssignment::Credit { credit }
}

impl ReleaseValues {
    /// The editable fields, as the library editor's form is seeded with them.
    /// A track's number is the one its source states, else its place in the
    /// release.
    pub(crate) fn user_edit(&self) -> ReleaseUserEdit {
        ReleaseUserEdit {
            album_title: self.album_title.clone(),
            album_artist_assignments: self.album_assignments(),
            album_year: self.album_year,
            pressing: self.pressing.clone(),
            tracks: self
                .tracks
                .iter()
                .map(|track| TrackUserEdit {
                    title: track.title.clone(),
                    side: track.side,
                    track_number: track.number,
                    artist_assignments: track.credits.assignments(),
                    file: None,
                })
                .collect(),
        }
    }

    /// Every value, laid onto the folder's audio `units`: track `i` plays
    /// unit `i`, under the draft's row identity for that place. A track whose
    /// source states no number is numbered by its place in the release.
    ///
    /// The reader laid the source out against these units, so they are as
    /// many as its tracks.
    pub(crate) fn candidate_draft(self, units: Vec<AudioFile>) -> CandidateDraft {
        assert_eq!(
            self.tracks.len(),
            units.len(),
            "a reading lays one track on each audio unit"
        );
        let album_artist_assignments = self.album_assignments();
        let tracks = self
            .tracks
            .into_iter()
            .zip(units)
            .enumerate()
            .map(|(position, (track, file))| CandidateTrack {
                edit: RawTrackEdit {
                    id: crate::import::pane::candidate_track_id(position),
                    title: track.title,
                    artist_assignments: track.credits.assignments(),
                    side: track.side,
                    track_number: track.number.unwrap_or_else(|| {
                        i32::try_from(position + 1).expect("track position fits i32")
                    }),
                    file,
                },
                stated_length_ms: track.stated_length_ms,
                roles: track.roles,
                works: track.works,
            })
            .collect();
        CandidateDraft {
            album_title: self.album_title,
            album_artist_assignments,
            album_year: self
                .album_year
                .map(|year| year.to_string())
                .unwrap_or_default(),
            pressing: RawPressingEdit::from_pressing(&self.pressing),
            tracks,
            release_roles: self.release_roles,
            works: self.works,
        }
    }

    /// The album credits as assignments. A credit with no name — tags that
    /// name no album artist — is no assignment at all.
    fn album_assignments(&self) -> Vec<ArtistAssignment> {
        self.album_credits
            .iter()
            .filter(|credit| !credit.name.trim().is_empty())
            .cloned()
            .map(credit_assignment)
            .collect()
    }
}

/// Read a positive number from a provider position: 7, A7, or 2-7.
pub(crate) fn position_number(position: &str) -> Option<i32> {
    let number = match position.split_once('-') {
        Some((disc, number)) if disc.bytes().all(|byte| byte.is_ascii_digit()) => number,
        _ if position
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphabetic) =>
        {
            &position[1..]
        }
        _ => position,
    };
    number.parse::<i32>().ok().filter(|number| *number > 0)
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
        if !self
            .graph
            .works
            .iter()
            .any(|held| held.musicbrainz_work_id == id)
        {
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
                SourceWorkEvent::Part {
                    direction: PartDirection::Backward,
                    work: parent,
                } => {
                    let parent = self.add(parent, None);
                    self.link(&parent, &id);
                }
                SourceWorkEvent::Part {
                    direction: PartDirection::Forward,
                    work: child,
                } => {
                    self.add(child, Some(&id));
                }
            }
        }
        id
    }

    fn link(&mut self, parent: &str, child: &str) {
        if self
            .graph
            .parts
            .iter()
            .any(|part| part.parent == parent && part.child == child)
        {
            return;
        }
        self.graph.parts.push(WorkPart {
            parent: parent.to_string(),
            child: child.to_string(),
        });
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
                (None, None) => crate::text_match::same_artist_name(&known.name, &composer.name),
                (Some(_), None) | (None, Some(_)) => false,
            }
        });
        if !held {
            work.composers.push(composer.clone());
        }
    }
}

/// The rows a reading's draft commits as: the reading laid onto one loose
/// file per track and built into rows the way the import builds them. What a
/// reader's test checks a reading's rows with.
#[cfg(test)]
pub(crate) fn committed_rows(values: ReleaseValues) -> crate::import::ReleaseRows {
    let units = (0..values.tracks.len())
        .map(|index| AudioFile::Standalone {
            file_id: format!("{index:02}.flac"),
        })
        .collect();
    let draft = values.candidate_draft(units);
    let clock = coven::FixedClock::new(
        chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .expect("a fixed instant")
            .with_timezone(&chrono::Utc)
            .into(),
    );
    crate::import::assemble::assemble_release(&draft, None, &clock, &coven::SequentialIds::new())
        .expect("the reading's draft commits")
        .parsed
}

/// The library artist each of `parsed`'s artist rows becomes when its import
/// commits into an empty library, by row id: which credits the commit makes
/// one artist.
#[cfg(test)]
pub(crate) fn committed_artists(
    parsed: &crate::import::ReleaseRows,
) -> std::collections::HashMap<String, String> {
    parsed
        .artists
        .iter()
        .map(|artist| artist.id.clone())
        .zip(crate::db::resolved_in_an_empty_library(&parsed.artists))
        .collect()
}
