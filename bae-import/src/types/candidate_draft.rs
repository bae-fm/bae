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
#[cfg(desktop)]
use crate::cover_art::PreparedCover;
#[cfg(desktop)]
use bae_catalogs::credits::ArtistCredit;
use bae_catalogs::credits::CreditedRole;
use bae_catalogs::works::WorkGraph;
use serde::{Deserialize, Serialize};

/// A release's draft: the editor's album fields, one [`DraftTrack`] per
/// track, and the release-level values the source stated. Its tracks are
/// bound to what they play by `Binding`: an import candidate's to its audio
/// units (track `i` plays unit `i`; the folder's order, not an edit, decides
/// which audio a track plays), a library release's to its track rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseDraft<Binding> {
    pub album_title: String,
    pub album_artist_assignments: Vec<Credited>,
    pub album_year: String,
    pub pressing: RawPressingEdit,
    /// The mediums the folder's audio is on, in order, as the source read them.
    /// Not edited: only applying a source writes them.
    pub mediums: Vec<bae_types::mediums::ReleaseMedium>,
    pub tracks: Vec<DraftTrack<Binding>>,
    /// Who the source credits in a role on the release itself rather than on
    /// one track.
    pub release_roles: Vec<CreditedRole>,
    /// Every work the tracks perform, once, with the works they are parts of
    /// or hold.
    pub works: WorkGraph,
}

/// An import candidate's draft: each track bound to the audio unit it plays.
pub type CandidateDraft = ReleaseDraft<AudioFile>;

/// A library release's draft: each track bound to its track row's id.
pub type LibraryDraft = ReleaseDraft<LibraryTrack>;

/// The track row a library draft's track is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryTrack(pub String);

/// What a draft's track is bound to, as the editor's form carries it: the
/// audio an import track plays, nothing for a library track.
pub trait TrackBinding: Clone {
    fn form_file(&self) -> Option<AudioFile>;
}

impl TrackBinding for AudioFile {
    fn form_file(&self) -> Option<AudioFile> {
        Some(self.clone())
    }
}

impl TrackBinding for LibraryTrack {
    fn form_file(&self) -> Option<AudioFile> {
        None
    }
}

/// An import candidate's track.
pub type CandidateTrack = DraftTrack<AudioFile>;

/// A track of a draft. What it is bound to and its place are required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftTrack<Binding> {
    pub edit: RawTrackEdit<Binding>,
    /// Where it sits on the draft's mediums, as the source read it. Not
    /// edited: only applying a source writes it.
    pub place: bae_types::mediums::TrackPlace,
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

impl<Binding: TrackBinding> RawTrackEdit<Binding> {
    /// Cross the editor boundary without weakening the stored draft's shape.
    pub fn as_edit(&self) -> RawTrackEdit {
        RawTrackEdit {
            id: self.id.clone(),
            title: self.title.clone(),
            artist_assignments: self.artist_assignments.clone(),
            file: self.file.form_file(),
        }
    }
}

impl CandidateTrack {
    /// A track read from no source but the folder: what the person types, or
    /// a track the folder gained after a source was applied. The source
    /// states nothing about it but where the folder puts it.
    pub fn unsourced(edit: RawTrackEdit<AudioFile>, place: bae_types::mediums::TrackPlace) -> Self {
        Self { edit, place, stated_length_ms: None, roles: Vec::new(), works: Vec::new() }
    }
}

impl<Binding: TrackBinding> ReleaseDraft<Binding> {
    /// The editor's form: the draft's editable fields.
    pub fn release_edit(&self) -> RawReleaseEdit {
        RawReleaseEdit {
            album_title: self.album_title.clone(),
            album_artist_assignments: self.album_artist_assignments.clone(),
            album_year: self.album_year.clone(),
            pressing: self.pressing.clone(),
            tracks: self.tracks.iter().map(|track| track.edit.as_edit()).collect(),
        }
    }

    /// Every artist credit the draft's album and track artists name, the
    /// album's first, then each track's own; picked library artists are not
    /// credits.
    #[cfg(desktop)]
    pub(crate) fn credits(&self) -> impl Iterator<Item = &ArtistCredit> {
        self.album_artist_assignments
            .iter()
            .chain(self.tracks.iter().flat_map(|track| track.edit.artist_assignments.explicit()))
            .filter_map(|credited| credited.artist.credit())
    }

    /// This draft with `tracks` in place of its own, and only the works those
    /// tracks perform.
    pub fn with_tracks(&self, tracks: Vec<DraftTrack<Binding>>) -> Self {
        let works = self.works.keeping_performed(
            tracks.iter().flat_map(|track| track.works.iter().map(String::as_str)),
        );
        Self { tracks, works, ..self.clone() }
    }
}

/// What the current draft's values were read from. Direct entry and a
/// cleared draft carry no provenance.
///
/// This is the draft's own history, not a claim about which release the
/// folder is: that claim is the candidate's [`ReleaseLink`], which no draft
/// write changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetadataProvenance {
    ExternalRelease {
        /// The catalog's release the draft is read from.
        record: bae_catalogs::records::LookupRef,
    },
    FileMetadata,
}

/// One candidate's editable metadata, independent of the source that last
/// populated it. The selected cover belongs to the draft, with the bytes a
/// remote one was fetched with; candidate files and mapping decisions do not.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateMetadataDraft {
    /// Ordered included tracks, each with audio and a required number.
    pub draft: CandidateDraft,
    pub provenance: Option<MetadataProvenance>,
    pub cover: Option<PreparedCover>,
}

/// The audio a track's samples come from.
///
/// The audio is named by its identity within the release
/// ([`ScannedFile::relative_path`](crate::folder_scanner::ScannedFile::relative_path)),
/// never by absolute path. The draft owns this choice independently of applied
/// metadata. Import resolves it against the persisted scan after validating the
/// physical file identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioFile {
    /// The whole file holds this one track.
    Standalone { file_id: String },
    /// One of several tracks a bound track sheet carves out of one container.
    /// `index` counts that sheet's playable tracks from zero.
    SheetSlice { file_id: String, sheet_id: String, index: u32 },
}

impl AudioFile {
    /// The audio file holding this track's samples.
    pub fn file_id(&self) -> &str {
        match self {
            Self::Standalone { file_id } | Self::SheetSlice { file_id, .. } => file_id,
        }
    }
}

#[cfg(test)]
#[path = "candidate_draft_tests.rs"]
mod tests;
