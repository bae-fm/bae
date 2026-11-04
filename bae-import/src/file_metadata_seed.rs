//! What a folder's own file tags make of it.
//!
//! Discovery and Reset initialize a candidate from this projection, and
//! applying file metadata uses it to replace metadata
//! while retaining the current rows' identities. The projected draft, its
//! source reading, and embedded cover are produced together.

use crate::cover_art::PreparedCover;
use crate::file_tag_snapshot::FileTagSnapshot;
use crate::folder_scanner::FolderCandidate;
use crate::{CandidateDraft, CandidateTrack, ImportError};

/// A folder read as its own files describe it.
pub(crate) struct FileMetadataSeed {
    /// The exact reading the draft was projected from.
    pub snapshot: FileTagSnapshot,
    pub draft: CandidateDraft,
    /// The cover applying these tags selects, where the tags embed artwork:
    /// that artwork, or a folder image named as the front cover, which ranks
    /// ahead of it (`local_artwork::file_tags_cover`).
    pub cover: Option<PreparedCover>,
}

impl FileMetadataSeed {
    /// Project what `candidate`'s audio files' tags say into the draft a
    /// candidate starts from: what `reads` holds of them and `stored` — the
    /// reading stored for the candidate, taken under the same folder — says,
    /// and a read of whatever neither knows.
    pub(crate) fn read(
        candidate: &crate::folder_scanner::FolderCandidate,
        scan_generation: u64,
        reads: &crate::folder_scanner::FileReads,
        stored: Option<&FileTagSnapshot>,
    ) -> Result<Self, ImportError> {
        let audio_files = candidate.files.audio().cloned().collect::<Vec<_>>();
        let snapshot = reads.tag_snapshot(
            &audio_files,
            scan_generation,
            candidate.file_edit_revision,
            stored,
        )?;
        Self::project(candidate, snapshot, None)
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
    ) -> Result<Self, ImportError> {
        let mut draft = crate::pane::file_metadata_draft(candidate, &snapshot)?;
        if let Some(keeping) = keeping {
            crate::pane::keep_row_identities(&mut draft.tracks, keeping)?;
        }
        let cover = crate::local_artwork::file_tags_cover(
            crate::cover_art::embedded_cover_selection(&snapshot),
            candidate.files.artwork(),
        );
        Ok(Self { snapshot, draft, cover })
    }
}
