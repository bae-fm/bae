//! The values import and the release editor share: a candidate's draft,
//! the editor's form and the edit it shapes, artist credits, and (desktop)
//! an import run's command and progress.

#[cfg(desktop)]
mod import_run;
#[cfg(desktop)]
pub use import_run::{
    CueAnalyzedAudioFile, CueFlacAnalysis, ImportCommand, ImportDestination, TrackAudio, TrackFile,
};
#[cfg(desktop)]
mod progress;
#[cfg(desktop)]
pub use progress::*;
mod candidate_edit_field;
pub use candidate_edit_field::{CandidateEditField, DraftFieldEdit, PressingFactEdit};
mod artist_assignment;
pub use artist_assignment::{
    artists_standing, credited_line, ArtistAssignment, ArtistStanding, ArtistsStanding, Credited,
    TrackArtistAssignments,
};
mod raw_release_edit;
mod release_edit_display;
pub use raw_release_edit::{
    EditValidationError, RawLabelEdit, RawPressingEdit, RawReleaseEdit, RawTrackEdit,
};
pub use release_edit_display::{
    ReleaseEditDisplayContext, ReleaseEditLoadError, ReleaseEditTrackContext, ReleaseEditTrackRun,
    ReleaseEditTrackSide, ReleaseEditTrackSource,
};
#[cfg(desktop)]
mod release_link;
#[cfg(desktop)]
pub use release_link::ReleaseLink;
mod release_edit;
pub use release_edit::{
    DraftAlbum, ReleaseEditDraft, ReleaseEditSeed, ReleaseUserEdit, TrackUserEdit,
};
mod candidate_draft;
#[cfg(desktop)]
pub use candidate_draft::CandidateMetadataDraft;
pub use candidate_draft::{
    AudioFile, CandidateDraft, CandidateTrack, DraftTrack, LibraryDraft, LibraryTrack,
    MetadataProvenance, ReleaseDraft, TrackBinding,
};
