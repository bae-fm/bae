//! What a folder's own file tags make of it.
//!
//! Discovery and Reset initialize a candidate from this projection when tag
//! prefill is enabled, and applying file metadata uses it to replace metadata
//! while retaining the current rows' identities. The projected draft, its
//! source reading, and embedded cover are produced together.

use crate::import::file_tag_snapshot::FileTagSnapshot;
use crate::import::folder_scanner::FolderCandidate;
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
        let audio_files = candidate.files.audio().cloned().collect::<Vec<_>>();
        let snapshot = crate::import::file_tag_snapshot::extract_file_tag_snapshot(
            &audio_files,
            scan_generation,
            candidate.file_edit_revision,
            reader,
        )?;
        Self::project(candidate, snapshot, None, clock, ids)
    }

    /// Project `snapshot` into the draft a candidate stores for it.
    ///
    /// `keeping` are the draft's current track rows, whose identities the
    /// tracks read afresh take. A candidate with no draft yet passes none.
    ///
    /// Applying tags replaces previously entered metadata values.
    pub(crate) fn project(
        candidate: &FolderCandidate,
        snapshot: FileTagSnapshot,
        keeping: Option<&[CandidateTrack]>,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ImportError> {
        let edit = crate::import::pane::file_metadata_edit(candidate, &snapshot, clock, ids)?;
        let mut draft = crate::import::pane::candidate_draft_from_edit(edit)?.draft;
        if let Some(keeping) = keeping {
            crate::import::pane::keep_row_identities(&mut draft.tracks, keeping)?;
        }
        let cover = crate::import::local_artwork::file_tags_cover(
            crate::import::file_tag_snapshot::embedded_cover_selection(&snapshot),
            candidate.files.artwork(),
        );
        Ok(Self {
            snapshot,
            draft,
            cover,
        })
    }
}
