//! A release candidate: one folder, or several a grouping reads as one.

use super::folder_scanner::{FolderCandidate, ReleaseFileScope};

/// The source shape an admitted import must still have when it commits: the
/// folder its files are read from, how much of it, and the folders it is made
/// of when it is several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSource {
    pub path: std::path::PathBuf,
    pub scope: ReleaseFileScope,
    pub parts: Vec<std::path::PathBuf>,
}

impl FolderCandidate {
    /// The draft the release's own files describe, with the track layout
    /// every draft of it keeps: a release read from several folders numbers
    /// its discs folder by folder, whatever the files' own tags say.
    pub(crate) fn file_tag_edit(
        &self,
        snapshot: &super::file_tag_snapshot::FileTagSnapshot,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<super::ReleaseUserEdit, super::ImportError> {
        let parsed = super::file_tag_mapper::map_file_metadata_to_db(
            &self.files,
            snapshot,
            Some(&self.name),
            clock,
            ids,
        )?;
        let mut edit = super::parsed_album_to_user_edit(&parsed);
        if !self.files.parts.is_empty() {
            let layout = super::track_slots::direct_entry_track_rows(&self.files);
            if edit.tracks.len() != layout.len() {
                return Err(super::ImportError::Internal {
                    detail: "file metadata does not match the release's track count".into(),
                });
            }
            for (track, laid_out) in edit.tracks.iter_mut().zip(&layout) {
                track.side = laid_out.side;
                track.track_number = laid_out.track_number;
            }
        }
        Ok(edit)
    }

    /// The draft a release starts from before anyone describes it: one blank
    /// track per audio unit, in the release's own track layout. A release
    /// read from several folders starts titled by the name it is listed as —
    /// no one folder's name speaks for it on its own.
    pub(crate) fn blank_source(&self) -> super::pane::CandidateSourceDraft {
        let mut source = super::pane::blank_candidate_source(&self.files);
        if self.grouping.is_some() {
            source.draft.album_title.clone_from(&self.name);
        }
        source
    }

    pub fn source(&self) -> CandidateSource {
        CandidateSource {
            path: self.file_root.clone(),
            scope: self.scope,
            parts: self
                .files
                .parts
                .iter()
                .map(|part| part.folder.clone())
                .collect(),
        }
    }
}
