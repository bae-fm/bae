//! A release may come from one scanned folder or an explicit folder selection.

use super::combination::{CandidateCombination, CombinationTrackOrder};
use super::folder_scanner::{
    CategorizedFiles, FolderCandidate, FolderReleaseDecisionKey, ResolvedFolderReleaseBoundary,
};
use std::borrow::Cow;

/// The source shape an admitted import must still have when it commits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateSource {
    Folder {
        path: std::path::PathBuf,
        scope: super::folder_scanner::ReleaseFileScope,
    },
    Combination,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CombinedCandidate {
    pub key: String,
    pub name: String,
    /// The watched-root section containing the combined row. Each part retains
    /// its own source key; this does not claim that its files share one root.
    pub watched_folder_path: String,
    pub order: CombinationTrackOrder,
    pub combination: CandidateCombination,
    pub file_edit_revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseCandidate {
    Folder(FolderCandidate),
    Combined(CombinedCandidate),
}

impl From<FolderCandidate> for ReleaseCandidate {
    fn from(candidate: FolderCandidate) -> Self {
        Self::Folder(candidate)
    }
}

impl ReleaseCandidate {
    pub(crate) fn file_tag_edit(
        &self,
        snapshot: &super::file_tag_snapshot::FileTagSnapshot,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<super::ReleaseUserEdit, super::ImportError> {
        let parsed = super::file_tag_mapper::map_file_tag_snapshot_to_db(
            self.files(),
            snapshot,
            Some(self.name()),
            clock,
            ids,
        )?;
        let mut edit = super::parsed_album_to_user_edit(&parsed);
        if let Self::Combined(candidate) = self {
            if edit.tracks.len() != candidate.combination.tracks.len() {
                return Err(super::ImportError::Internal {
                    detail: "file metadata does not match the reviewed combination track count"
                        .into(),
                });
            }
            for (track, reviewed) in edit.tracks.iter_mut().zip(&candidate.combination.tracks) {
                track.side = reviewed.side;
                track.track_number = reviewed.track_number;
            }
        }
        Ok(edit)
    }

    pub fn source_file_edits_allowed(&self) -> bool {
        matches!(self, Self::Folder(_))
    }

    pub fn source_folders(&self) -> Vec<std::path::PathBuf> {
        match self {
            Self::Folder(candidate) => vec![candidate.path.clone()],
            Self::Combined(candidate) => candidate
                .combination
                .parts
                .iter()
                .map(|part| std::path::PathBuf::from(&part.candidate_key))
                .collect(),
        }
    }

    pub(crate) fn blank_source(&self) -> super::pane::CandidateSourceDraft {
        match self {
            Self::Folder(candidate) => super::pane::blank_candidate_source(&candidate.files),
            Self::Combined(candidate) => {
                super::pane::blank_source_for_tracks(candidate.combination.tracks.clone())
            }
        }
    }

    pub fn source(&self) -> CandidateSource {
        match self {
            Self::Folder(candidate) => CandidateSource::Folder {
                path: candidate.file_root.clone(),
                scope: candidate.scope,
            },
            Self::Combined(_) => CandidateSource::Combination,
        }
    }

    pub fn key(&self) -> Cow<'_, str> {
        match self {
            Self::Folder(candidate) => candidate.path.to_string_lossy(),
            Self::Combined(candidate) => Cow::Borrowed(&candidate.key),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Folder(candidate) => &candidate.name,
            Self::Combined(candidate) => &candidate.name,
        }
    }

    pub fn files(&self) -> &CategorizedFiles {
        match self {
            Self::Folder(candidate) => &candidate.files,
            Self::Combined(candidate) => &candidate.combination.files,
        }
    }

    pub fn into_files(self) -> CategorizedFiles {
        match self {
            Self::Folder(candidate) => candidate.files,
            Self::Combined(candidate) => candidate.combination.files,
        }
    }

    pub fn file_edit_revision(&self) -> u64 {
        match self {
            Self::Folder(candidate) => candidate.file_edit_revision,
            Self::Combined(candidate) => candidate.file_edit_revision,
        }
    }

    pub fn watched_folder_path(&self) -> &str {
        match self {
            Self::Folder(candidate) => &candidate.watched_folder_path,
            Self::Combined(candidate) => &candidate.watched_folder_path,
        }
    }

    pub fn display_path(&self) -> &str {
        match self {
            Self::Folder(candidate) => &candidate.display_path,
            Self::Combined(candidate) => &candidate.name,
        }
    }

    pub fn resolved_boundaries(&self) -> &[ResolvedFolderReleaseBoundary] {
        match self {
            Self::Folder(candidate) => &candidate.resolved_boundaries,
            Self::Combined(_) => &[],
        }
    }

    pub fn combine_ancestor_key(&self) -> Option<&FolderReleaseDecisionKey> {
        match self {
            Self::Folder(candidate) => candidate.combine_ancestor_key.as_ref(),
            Self::Combined(_) => None,
        }
    }
}
