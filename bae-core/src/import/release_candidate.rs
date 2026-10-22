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
    /// What the release's own files describe, with the track layout every
    /// draft of it keeps: a release read from several folders numbers its
    /// discs folder by folder, whatever the files' own tags say.
    pub(crate) fn file_tag_values(
        &self,
        snapshot: &super::file_tag_snapshot::FileTagSnapshot,
    ) -> Result<super::release_values::ReleaseValues, super::ImportError> {
        let mut values =
            super::file_tag_mapper::file_metadata_values(&self.files, snapshot, Some(&self.name))?;
        if !self.files.parts.is_empty() {
            let layout = super::audio_layout::direct_entry_track_rows(&self.files);
            if values.tracks.len() != layout.len() {
                return Err(super::ImportError::Internal {
                    detail: "file metadata does not match the release's track count".into(),
                });
            }
            for (track, laid_out) in values.tracks.iter_mut().zip(&layout) {
                track.side = laid_out.side;
                track.number = laid_out.track_number;
            }
        }
        Ok(values)
    }

    /// The draft a release starts from before anyone describes it: one blank
    /// track per audio unit, in the release's own track layout. A release
    /// read from several folders starts titled by the name it is listed as —
    /// no one folder's name speaks for it on its own.
    pub(crate) fn blank_draft(&self) -> super::CandidateDraft {
        let mut draft = super::pane::blank_candidate_draft(&self.files);
        if self.grouping.is_some() {
            draft.album_title.clone_from(&self.name);
        }
        draft
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
