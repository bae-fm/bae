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
    /// The mediums the audio is on, in order.
    pub mediums: Vec<crate::mediums::ReleaseMedium>,
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
    /// Where it sits on the reading's mediums.
    pub place: crate::mediums::TrackPlace,
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
                    artist_assignments: track.credits.assignments(),
                    file: None,
                })
                .collect(),
        }
    }

    /// Every value, laid onto the folder's audio `units`: track `i` plays
    /// unit `i`, under the draft's row identity for that place.
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
                    file,
                },
                place: track.place,
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
            mediums: self.mediums,
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

/// How each of `values`' tracks reads on its mediums: `A1` on a lettered
/// side, `2-3` for track 3 of disc 2, `5` on a release of one medium. What a
/// reader's test checks places with.
#[cfg(test)]
pub(crate) fn place_labels(
    mediums: &[crate::mediums::ReleaseMedium],
    places: impl IntoIterator<Item = crate::mediums::TrackPlace>,
) -> Vec<String> {
    use crate::album_detail::TrackPosition;
    places
        .into_iter()
        .map(
            |place| match crate::mediums::track_position(&place, mediums) {
                TrackPosition::Sided {
                    side_letter,
                    number,
                } => format!("{side_letter}{number}"),
                TrackPosition::Disc { disc, number } => format!("{disc}-{number}"),
                TrackPosition::Flat { number } => number.to_string(),
            },
        )
        .collect()
}

/// [`place_labels`] of a reading's tracks.
#[cfg(test)]
pub(crate) fn value_labels(values: &ReleaseValues) -> Vec<String> {
    place_labels(&values.mediums, values.tracks.iter().map(|track| track.place))
}

/// [`place_labels`] of an import's track rows.
#[cfg(test)]
pub(crate) fn row_labels(rows: &crate::import::ReleaseRows) -> Vec<String> {
    let mediums: Vec<_> = rows.mediums.iter().map(|medium| medium.medium).collect();
    place_labels(
        &mediums,
        rows.tracks.iter().map(|track| {
            let medium = rows
                .mediums
                .iter()
                .find(|medium| medium.id == track.medium_id)
                .expect("a track row's medium is one of the release's");
            crate::mediums::TrackPlace {
                medium: medium.position,
                side: track.side,
                number: track.track_number,
            }
        }),
    )
}
