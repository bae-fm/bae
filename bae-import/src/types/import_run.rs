//! One import run: the command that asks the import worker for it, where
//! its files go, and the tracks it works from, each with the audio that
//! supplies its samples.

use crate::cue_sheet::CueSheet;
use bae_audio::ProbeResult;
use bae_records::DbTrack;
use std::{path::PathBuf, sync::Arc};

/// Import command sent to the service worker.
///
/// Carries only identifiers, never provider payloads. The worker consumes the
/// candidate's stored draft and its prepared cover bytes; the one provider
/// work it does is fetching the Discogs pictures of the artists it creates.
///
#[derive(Debug)]
pub struct ImportCommand {
    pub import_id: String,
    pub candidate_key: crate::CandidateKey,
    pub source: crate::release_candidate::CandidateSource,
    #[cfg(any(test, feature = "test-utils"))]
    pub selected_cover: Option<crate::CoverSelection>,
    pub destination: ImportDestination,
    /// The release a test import picks for the candidate first.
    #[cfg(any(test, feature = "test-utils"))]
    pub pick: Option<bae_identify::PressingLink>,
    /// Whether a test import then reads the draft from the files' tags.
    #[cfg(any(test, feature = "test-utils"))]
    pub read_file_tags: bool,
    #[cfg(any(test, feature = "test-utils"))]
    pub user_edit: Option<crate::ReleaseUserEdit>,
}

/// Whether an import attaches its files for background upload. Every release's
/// rows sync; a local import keeps its file paths only on this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportDestination {
    /// Files stay in place on this device; never uploaded.
    Local,
    /// Stage files with coven for the cloud in the import transaction and
    /// upload them in the background.
    Remote {
        /// Whether this device keeps the files whole once they are stored,
        /// pinned rather than evictable.
        pin: bool,
    },
}

impl ImportDestination {
    /// Where an import goes: to the cloud, pinned as chosen, when the stored
    /// choice is the cloud and the library syncs; locally otherwise.
    pub fn of(
        storage: bae_preferences::ImportStoragePreferences,
        storage_configured: bool,
    ) -> Self {
        if storage.cloud && storage_configured {
            Self::Remote { pin: storage.pinned }
        } else {
            Self::Local
        }
    }
}

/// One import track's metadata and the audio that supplies its samples.
/// The metadata has the same shape whether the audio is a whole file or a
/// track described by a CUE sheet.
#[derive(Debug, Clone)]
pub struct TrackFile {
    pub db_track: DbTrack,
    pub audio: TrackAudio,
}

/// The resolved audio source for an import track. CUE sources share their
/// sheet's analysis and identify the playable track within it.
#[derive(Debug, Clone)]
pub enum TrackAudio {
    Standalone { file_path: PathBuf, source_audio: crate::folder_scanner::ScannedAudio },
    CueBacked { cue_pair: Arc<CueFlacAnalysis>, cue_index: usize },
}

/// Parsed CUE sheet plus probed container analysis, shared across all tracks
/// that live inside the same file.
#[derive(Debug)]
pub struct CueFlacAnalysis {
    pub cue_sheet: CueSheet,
    pub audio_files: Vec<CueAnalyzedAudioFile>,
}

impl CueFlacAnalysis {
    /// The audio a `FILE` directive of the sheet names, as the scan resolved
    /// and probed it.
    pub(crate) fn audio_file(&self, file_reference: &str) -> Option<&CueAnalyzedAudioFile> {
        self.audio_files.iter().find(|file| file.file_reference == file_reference)
    }
}

#[derive(Debug)]
pub struct CueAnalyzedAudioFile {
    pub file_reference: String,
    pub path: PathBuf,
    pub probe: ProbeResult,
}

#[cfg(test)]
#[path = "import_run_tests.rs"]
mod tests;
