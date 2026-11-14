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
    ) -> Result<bae_catalogs::release_values::ReleaseValues, super::ImportError> {
        let mut values =
            super::file_tag_mapper::file_metadata_values(&self.files, snapshot, Some(&self.name))?;
        if !self.files.parts.is_empty() {
            let layout = super::audio_layout::folder_tracks(&self.files);
            if values.tracks.len() != layout.len() {
                return Err(super::ImportError::Internal {
                    detail: "file metadata does not match the release's track count".into(),
                });
            }
            let numbering: Vec<_> =
                layout.iter().map(|track| (track.disc, Some(track.number))).collect();
            let (mediums, places) = bae_types::mediums::by_disc(&numbering);
            values.mediums = mediums;
            for (track, place) in values.tracks.iter_mut().zip(places) {
                track.place = place;
            }
        }
        Ok(values)
    }

    pub fn source(&self) -> CandidateSource {
        CandidateSource {
            path: self.file_root.clone(),
            scope: self.scope,
            parts: self.files.parts.iter().map(|part| part.folder.clone()).collect(),
        }
    }
}
