use super::*;
use crate::import::file_tag_snapshot::FileTagSnapshot;
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::release_candidate::ReleaseCandidate;
use crate::import::CandidateMetadataDraft;

impl CandidatePreparations {
    /// Replace the complete import setup, including the decisions and evidence
    /// that described its previous audio, under the caller's source revisions.
    pub(crate) async fn reset_setup(
        &self,
        candidate: &ReleaseCandidate,
        read: &CandidateAsRead,
        scan_generation: u64,
        lookup_choices: crate::import::LookupChoices,
        metadata: CandidateMetadataDraft,
        snapshot: Option<FileTagSnapshot>,
        settled_folders: Vec<(String, CategorizedFiles)>,
    ) -> Result<Vec<ReleaseCandidate>, LibraryError> {
        let mut prep = self.loaded_at(read).await?;
        let expected = CandidateSaveExpectation {
            edit_revision: read.file_edit_revision,
            metadata_revision: read.metadata_revision,
            scanned: Some(crate::db::CandidateScanExpectation::AtGeneration {
                key: ScannedCandidateKey {
                    watched_folder_path: candidate.watched_folder_path().into(),
                    candidate_path: candidate.key().into_owned(),
                },
                generation: scan_generation,
            }),
        };
        prep.folder_path = candidate.key().into_owned();
        prep.file_edits = CandidateFileEdits {
            revision: read.file_edit_revision.checked_add(1).ok_or_else(|| {
                LibraryError::Import("candidate file revision exhausted the u64 range".into())
            })?,
            ..CandidateFileEdits::default()
        };
        prep.metadata_revision = read.metadata_revision.checked_add(1).ok_or_else(|| {
            LibraryError::Import("candidate metadata revision exhausted the u64 range".into())
        })?;
        // The person asked for the setup back: what it starts from now is
        // theirs, whether the folder's tags or a blank draft.
        prep.author = MetadataAuthor::Person;
        prep.metadata = metadata;
        prep.assets_prepared = true;
        prep.identification = None;
        prep.signals = None;
        let extras = CandidateSaveExtras {
            file_tag_snapshot: snapshot,
            reshaped_files: Some(settled_folders),
            lookup_update: CandidateLookupUpdate::Reset {
                expected: lookup_choices,
            },
            result: CandidateResultWrite::Keep,
        };
        match self
            .database
            .save_candidate_preparation(prep, expected, extras)
            .await?
        {
            CandidateSaved::Landed(candidates) => Ok(candidates),
            CandidateSaved::Superseded => Err(LibraryError::Import(
                "candidate changed before its import setup was reset".into(),
            )),
        }
    }
}
