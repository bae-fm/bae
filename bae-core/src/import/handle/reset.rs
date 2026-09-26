use super::*;
use crate::import::file_metadata_seed::FileMetadataSeed;
use crate::import::file_tag_snapshot::extract_file_tag_snapshot;
use crate::import::folder_scanner::CandidateFileEdits;
use crate::import::folder_scanner::FolderCandidate;
use crate::import::{
    CandidateAsRead, CandidateMetadataDraft, CandidatePreparedAssets, ImportError,
};

impl ImportServiceHandle {
    /// Restore the candidate's source tracks and initial metadata using the
    /// current tag-prefill preference. Source files and combination membership
    /// remain unchanged.
    pub async fn reset_candidate_setup(&self, candidate_key: &str) -> Result<(), ImportError> {
        let this = self.clone();
        let candidate_key = candidate_key.to_owned();
        self.committed(async move { this.reset_candidate_setup_write(&candidate_key).await })
            .await
    }

    async fn reset_candidate_setup_write(&self, candidate_key: &str) -> Result<(), ImportError> {
        let (candidate, read, generation, lookup_choices, matching_folders, prefill) = {
            let _commit = self.folder_state_commit.lock("read a candidate to reset").await;
            let candidate = self.editable_candidate_for_commit(candidate_key).await?;
            let content_hash = candidate.files.content_hash();
            let state = self
                .library_manager
                .load_import_candidate_state(&content_hash)
                .await?
                .ok_or_else(|| ImportError::Internal {
                    detail: format!("{candidate_key} has no stored import preparation"),
                })?;
            if state.file_edits.revision != candidate.file_edit_revision {
                return Err(ImportError::Internal {
                    detail: format!("{candidate_key} preparation does not match its scanned files"),
                });
            }
            let stored = self
                .library_manager
                .load_candidate_file_tag_snapshot(&candidate.watched_folder_path, candidate_key)
                .await?
                .ok_or_else(|| ImportError::Internal {
                    detail: format!("{candidate_key} has no scanned source identity"),
                })?;
            let matching_folders = crate::import::candidates::files_for_identity(
                &self.library_manager.load_all_folder_scan_items().await?,
                &content_hash,
                candidate.file_edit_revision,
            );
            let read = CandidateAsRead {
                content_hash,
                file_edit_revision: state.file_edits.revision,
                metadata_revision: state.metadata_revision,
            };
            (
                candidate,
                read,
                stored.scan_generation,
                state.lookup_choices,
                matching_folders,
                self.library_manager
                    .get_config()
                    .prefs
                    .prefill_with_file_metadata,
            )
        };
        let next_revision =
            read.file_edit_revision
                .checked_add(1)
                .ok_or_else(|| ImportError::Internal {
                    detail: "candidate file revision exhausted the u64 range".into(),
                })?;
        let mut initialized_candidate = candidate.clone();
        let reader = self.file_tags.clone();
        let clock = self.clock.clone();
        let ids = self.ids.clone();
        let (metadata, snapshot, settled_folders) = tokio::task::spawn_blocking(move || {
            let mut settled_folders = Vec::with_capacity(matching_folders.len());
            let decisions = CandidateFileEdits {
                revision: next_revision,
                ..CandidateFileEdits::default()
            };
            for (key, mut files) in matching_folders {
                files.apply_candidate_file_edits(&decisions)?;
                settled_folders.push((key, files));
            }
            initialized_candidate
                .files
                .apply_candidate_file_edits(&decisions)?;
            initialized_candidate.file_edit_revision = next_revision;
            let identity_files = initialized_candidate
                .files
                .release_files()
                .cloned()
                .collect::<Vec<_>>();
            crate::import::file_identity::validate_scanned_file_identities(&identity_files)?;
            let (draft, provenance, cover, snapshot) = if prefill {
                let audio = initialized_candidate
                    .files
                    .audio()
                    .cloned()
                    .collect::<Vec<_>>();
                let snapshot =
                    extract_file_tag_snapshot(&audio, generation, next_revision, reader.as_ref())?;
                let durations =
                    crate::import::probe::source_durations(&initialized_candidate.files)?;
                let seed = FileMetadataSeed::project(
                    &initialized_candidate,
                    snapshot,
                    &durations,
                    None,
                    clock.as_ref(),
                    ids.as_ref(),
                )?;
                (
                    seed.draft,
                    Some(crate::import::MetadataProvenance::FileMetadata),
                    seed.cover,
                    Some(seed.snapshot),
                )
            } else {
                (initialized_candidate.blank_source().draft, None, None, None)
            };
            // A reset unmakes the whole setup, so the candidate starts again
            // with the cover its folder gives it — the same one a scan of a
            // new candidate stores.
            let cover = crate::import::local_artwork::folder_cover(
                cover,
                initialized_candidate.files.artwork(),
            );
            // A CUE or artwork file can change while its audio's tags are read.
            // Check the whole scanned source again before committing its seed.
            crate::import::file_identity::validate_scanned_file_identities(&identity_files)?;
            Ok::<_, ImportError>((
                CandidateMetadataDraft {
                    draft,
                    provenance,
                    cover,
                    source_discogs_artist_ids: Default::default(),
                    assets: CandidatePreparedAssets::default(),
                },
                snapshot,
                settled_folders,
            ))
        })
        .await
        .map_err(|error| ImportError::Internal {
            detail: format!("candidate reset preparation failed: {error}"),
        })??;
        let _commit = self
            .commit_lock_for_revision(
"reset a candidate",candidate_key, &read.content_hash, read.file_edit_revision)
            .await?;
        let candidates = self
            .preparations
            .reset_setup(
                &candidate,
                &read,
                generation,
                lookup_choices,
                metadata,
                snapshot,
                settled_folders,
            )
            .await?;
        for candidate in candidates {
            self.cancel_identification(&candidate.key());
            self.announce_source_candidate(candidate);
        }
        Ok(())
    }

    pub(super) fn announce_source_candidate(&self, candidate: FolderCandidate) {
        self.event_tx.send(ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
            candidate,
        }));
    }
}
