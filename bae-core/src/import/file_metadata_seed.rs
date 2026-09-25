//! What a folder's own file tags make of it.
//!
//! Discovery and Reset initialize a candidate from this projection when tag
//! prefill is enabled. Applying file metadata uses it to replace metadata while
//! retaining the current audio rows; adding unused audio takes the new row
//! from the same initializer. The projected draft, its source reading, and
//! embedded cover are produced together.

use crate::import::file_tag_snapshot::FileTagSnapshot;
use crate::import::pane::file_metadata_pane;
use crate::import::probe::SourceDurations;
use crate::import::release_candidate::ReleaseCandidate;
use crate::import::{CandidateDraft, CandidateTrack, CoverSelection, ImportError};

/// A folder read as its own files describe it.
pub(crate) struct FileMetadataSeed {
    /// The exact reading the draft was projected from.
    pub snapshot: FileTagSnapshot,
    pub draft: CandidateDraft,
    /// The cover applying these tags selects, where the tags embed artwork:
    /// that artwork, or a folder image named as the front cover, which ranks
    /// ahead of it (`local_artwork::file_tags_cover`).
    pub cover: Option<CoverSelection>,
}

impl FileMetadataSeed {
    /// Read `candidate`'s audio files and project what they say into the draft
    /// a candidate starts from.
    pub(crate) fn read(
        candidate: &crate::import::folder_scanner::FolderCandidate,
        scan_generation: u64,
        reader: &dyn crate::import::file_tag_snapshot::FileTagReader,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ImportError> {
        let durations = crate::import::probe::source_durations(&candidate.files)?;
        let audio_files = candidate.files.audio().cloned().collect::<Vec<_>>();
        let snapshot = crate::import::file_tag_snapshot::extract_file_tag_snapshot(
            &audio_files,
            scan_generation,
            candidate.file_edit_revision,
            reader,
        )?;
        Self::project(
            &candidate.clone().into(),
            snapshot,
            &durations,
            None,
            clock,
            ids,
        )
    }

    /// Project `snapshot` into the draft a candidate stores for it.
    ///
    /// `keeping` are the draft's current track rows: the decisions a person
    /// made about which file becomes which track outlive the metadata read
    /// over them. A candidate with no draft yet passes none.
    ///
    /// Applying tags replaces previously entered metadata values.
    pub(crate) fn project(
        candidate: &ReleaseCandidate,
        snapshot: FileTagSnapshot,
        durations: &SourceDurations,
        keeping: Option<&[CandidateTrack]>,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ImportError> {
        let pane = file_metadata_pane(candidate, &snapshot, durations, clock, ids)?;
        let mut draft = crate::import::pane::candidate_draft_from_source(pane)?.draft;
        if let Some(keeping) = keeping {
            draft.tracks = crate::import::pane::file_metadata_tracks(&draft.tracks, keeping);
        }
        let cover = crate::import::local_artwork::file_tags_cover(
            crate::import::file_tag_snapshot::embedded_cover_selection(&snapshot),
            candidate.files().artwork(),
        );
        Ok(Self {
            snapshot,
            draft,
            cover,
        })
    }
}
