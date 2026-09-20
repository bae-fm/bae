//! What a folder's own file tags make of it.
//!
//! One projection, two callers: discovery seeds a new candidate's draft from
//! it when the library pre-fills with tags, and "Reset to tags" replaces an
//! existing draft with it. Both store the same four things — the draft, the
//! reading it was projected from, the provenance naming those tags, and the
//! cover the tags embed — so both build them here.

use crate::import::file_tag_snapshot::FileTagSnapshot;
use crate::import::pane::file_tags_pane;
use crate::import::probe::SourceDurations;
use crate::import::release_candidate::ReleaseCandidate;
use crate::import::{CandidateDraft, CandidateTrack, CoverSelection, FieldOrigin, ImportError};

/// A folder read as its own files describe it.
pub(crate) struct FileTagsSeed {
    /// The exact reading the draft was projected from.
    pub snapshot: FileTagSnapshot,
    pub draft: CandidateDraft,
    /// The artwork the tags embed, where they embed any. Folder artwork stays
    /// the candidate's source-neutral fallback.
    pub cover: Option<CoverSelection>,
}

impl FileTagsSeed {
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
    /// The fields do not: reading a folder as its own tags is a reset, and a
    /// reset is the one thing that drops what a person typed.
    pub(crate) fn project(
        candidate: &ReleaseCandidate,
        snapshot: FileTagSnapshot,
        durations: &SourceDurations,
        keeping: Option<&[CandidateTrack]>,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ImportError> {
        let pane = file_tags_pane(candidate, &snapshot, durations, clock, ids)?;
        let mut draft = crate::import::pane::candidate_draft_from_source(pane)?
            .draft
            .read_from(FieldOrigin::Tags);
        if let Some(keeping) = keeping {
            draft.tracks = crate::import::pane::file_metadata_tracks(&draft.tracks, keeping);
        }
        let cover = crate::import::file_tag_snapshot::embedded_cover_selection(&snapshot);
        Ok(Self {
            snapshot,
            draft,
            cover,
        })
    }
}
