//! Import type definitions.
//!
//! Every import follows the same flow, whether the tracks are individual files
//! or one container plus a CUE sheet, and whether the metadata came from
//! MusicBrainz, Discogs, the files' own tags, or direct entry:
//!
//! 1. **Preparation** ([`PrepareStep`]) — validate that the stored candidate's
//!    source files still have the identities recorded by the scan.
//! 2. **Running** ([`ImportPhase`]) — read and hash each file where it already
//!    sits (no bytes move, no transcode), measure per-track loudness by decoding,
//!    and write every row in one transaction.
//!
//! An import always lands as a local, playable release. An
//! [`ImportDestination::Remote`] import then uploads to the cloud in the background.
use bae_catalogs::credits::{ArtistCredit, CreditedRole};
use bae_catalogs::works::WorkGraph;
use serde::{Deserialize, Serialize};
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
pub use release_edit::{DraftAlbum, ReleaseEditSeed, ReleaseUserEdit, TrackUserEdit};
mod candidate_draft;
pub use candidate_draft::{
    AudioFile, CandidateDraft, CandidateTrack, DraftTrack, LibraryDraft, LibraryTrack,
    MetadataProvenance, ReleaseDraft, TrackBinding,
};
#[cfg(desktop)]
pub use candidate_draft::{CandidateMetadataDraft, CandidatePreparedAssets};

/// Trim a raw pressing field; empty (after trim) becomes `None`.
fn trim_to_option(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
