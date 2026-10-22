//! A candidate's stored draft: every value the import writes, as plain
//! values.
//!
//! Applying a source — a catalog release, the files' tags, or nothing — writes
//! what it reads here once: the album fields, the pressing, one track per audio
//! unit with its title, credits, place and the length the source lists for it,
//! and the roles and works the source credits. Edits change the editable
//! fields; the import writes the draft as it is. Nothing points back into the
//! source, so nothing reads it again.

use super::*;

/// A candidate's draft: the editor's album fields, one [`CandidateTrack`] per
/// audio unit of the folder in the folder's order, and the release-level
/// values the source stated. Track `i` plays unit `i`; the folder's order, not
/// an edit, decides which audio a track plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateDraft {
    pub album_title: String,
    pub album_artist_assignments: Vec<ArtistAssignment>,
    pub album_year: String,
    pub pressing: RawPressingEdit,
    /// The mediums the folder's audio is on, in order, as the source read them.
    /// Not edited: only applying a source writes them.
    pub mediums: Vec<crate::mediums::ReleaseMedium>,
    pub tracks: Vec<CandidateTrack>,
    /// Who the source credits in a role on the release itself rather than on
    /// one track.
    pub release_roles: Vec<CreditedRole>,
    /// Every work the tracks perform, once, with the works they are parts of
    /// or hold.
    pub works: WorkGraph,
}

/// A track of the candidate's draft. Its audio and its place are required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateTrack {
    pub edit: RawTrackEdit<AudioFile>,
    /// Where it sits on the draft's mediums, as the source read it. Not
    /// edited: only applying a source writes it.
    pub place: crate::mediums::TrackPlace,
    /// The length the source the draft was read from lists for this track,
    /// shown beside its audio's own; unstated where the source lists none,
    /// and for a track read from the files or from nothing.
    pub stated_length_ms: Option<u64>,
    /// Who the source credits on this track in a role, in its order.
    pub roles: Vec<CreditedRole>,
    /// The MusicBrainz ids of the works this track performs, in the order it
    /// performs them; each is one of the draft's [`WorkGraph::works`].
    pub works: Vec<String>,
}

/// One artist a source credits in a role: who, which catalog said so, and
/// the words it credits the role with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditedRole {
    pub artist: ArtistCredit,
    pub source: Catalog,
    pub role: Option<String>,
}

/// The works a draft's tracks perform, and how they nest. Works come from
/// MusicBrainz, which names them; each is held once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkGraph {
    pub works: Vec<GraphWork>,
    /// Which work is part of which, in the order the source relates them.
    pub parts: Vec<WorkPart>,
}

/// One MusicBrainz work as its source states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphWork {
    pub musicbrainz_work_id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    /// Its composers, in the work's order.
    pub composers: Vec<ArtistCredit>,
}

/// One work that is part of another, both named by their MusicBrainz ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkPart {
    pub parent: String,
    pub child: String,
}

impl WorkGraph {
    /// The graph holding only the works `performed` names and the works they
    /// nest among, transitively: what a set of tracks performs once the
    /// tracks that performed the rest are gone.
    pub fn keeping_performed<'a>(&self, performed: impl IntoIterator<Item = &'a str>) -> Self {
        let mut kept: std::collections::HashSet<&str> = performed.into_iter().collect();
        loop {
            let before = kept.len();
            for part in &self.parts {
                if kept.contains(part.parent.as_str()) || kept.contains(part.child.as_str()) {
                    kept.insert(&part.parent);
                    kept.insert(&part.child);
                }
            }
            if kept.len() == before {
                break;
            }
        }
        Self {
            works: self
                .works
                .iter()
                .filter(|work| kept.contains(work.musicbrainz_work_id.as_str()))
                .cloned()
                .collect(),
            parts: self
                .parts
                .iter()
                .filter(|part| kept.contains(part.parent.as_str()))
                .cloned()
                .collect(),
        }
    }
}

impl RawTrackEdit<AudioFile> {
    /// Cross the editor boundary without weakening the stored draft's shape.
    pub fn as_edit(&self) -> RawTrackEdit {
        RawTrackEdit {
            id: self.id.clone(),
            title: self.title.clone(),
            artist_assignments: self.artist_assignments.clone(),
            file: Some(self.file.clone()),
        }
    }
}

impl CandidateTrack {
    /// A track read from no source but the folder: what the person types, or
    /// a track the folder gained after a source was applied. The source
    /// states nothing about it but where the folder puts it.
    pub fn unsourced(edit: RawTrackEdit<AudioFile>, place: crate::mediums::TrackPlace) -> Self {
        Self {
            edit,
            place,
            stated_length_ms: None,
            roles: Vec::new(),
            works: Vec::new(),
        }
    }
}

impl CandidateDraft {
    /// The editor's form: the draft's editable fields.
    pub fn release_edit(&self) -> RawReleaseEdit {
        RawReleaseEdit {
            album_title: self.album_title.clone(),
            album_artist_assignments: self.album_artist_assignments.clone(),
            album_year: self.album_year.clone(),
            pressing: self.pressing.clone(),
            tracks: self
                .tracks
                .iter()
                .map(|track| track.edit.as_edit())
                .collect(),
        }
    }

    /// Every artist credit the draft's album and track artists name, the
    /// album's first, then each track's own; picked library artists are not
    /// credits.
    pub(crate) fn credits(&self) -> impl Iterator<Item = &ArtistCredit> {
        self.album_artist_assignments
            .iter()
            .chain(
                self.tracks
                    .iter()
                    .flat_map(|track| track.edit.artist_assignments.explicit()),
            )
            .filter_map(ArtistAssignment::credit)
    }

    /// The Discogs identity of every artist the import may create, and so may
    /// need a picture for: each credit of the album and track artists, of the
    /// roles, and of the works' composers. A picked library artist is never
    /// created.
    pub fn discogs_artist_ids(&self) -> std::collections::BTreeSet<String> {
        let roles = self
            .release_roles
            .iter()
            .chain(self.tracks.iter().flat_map(|track| &track.roles))
            .map(|role| &role.artist);
        let composers = self.works.works.iter().flat_map(|work| &work.composers);
        self.credits()
            .chain(roles)
            .chain(composers)
            .filter_map(|credit| credit.discogs_artist_id.clone())
            .collect()
    }

    /// This draft with `tracks` in place of its own, and only the works those
    /// tracks perform.
    pub fn with_tracks(&self, tracks: Vec<CandidateTrack>) -> Self {
        let works = self.works.keeping_performed(
            tracks
                .iter()
                .flat_map(|track| track.works.iter().map(String::as_str)),
        );
        Self {
            tracks,
            works,
            ..self.clone()
        }
    }
}
