//! A candidate's file tags: the stored reading while its audio is unchanged,
//! a new reading otherwise.

use super::*;

#[derive(PartialEq, Eq)]
enum FileTagSnapshotMatch {
    Current,
    CandidateChanged,
    AudioChanged,
}

fn file_tag_snapshot_match(
    snapshot: &crate::file_tag_snapshot::FileTagSnapshot,
    scan_generation: u64,
    file_edit_revision: u64,
    observations: &[crate::file_tag_snapshot::FileObservation],
) -> FileTagSnapshotMatch {
    if snapshot.scan_generation != scan_generation
        || snapshot.file_edit_revision != file_edit_revision
    {
        return FileTagSnapshotMatch::CandidateChanged;
    }
    if snapshot.files.iter().map(|fact| &fact.observation).eq(observations) {
        FileTagSnapshotMatch::Current
    } else {
        FileTagSnapshotMatch::AudioChanged
    }
}

/// A candidate's file tags as [`ImportServiceHandle::read_file_tag_snapshot`]
/// found them, not yet stored.
pub(super) struct FileTagSnapshotRead {
    /// The candidate as stored when its reading was looked up.
    pub(super) candidate: crate::folder_scanner::FolderCandidate,
    pub(super) snapshot: crate::file_tag_snapshot::FileTagSnapshot,
}

impl ImportServiceHandle {
    /// The stored reading while the candidate and its audio files are unchanged,
    /// a new reading otherwise. Stores nothing; the caller decides whether the
    /// reading is kept.
    pub(super) async fn read_file_tag_snapshot(
        &self,
        candidate_key: &crate::CandidateKey,
        reader: std::sync::Arc<dyn crate::file_tag_reader::FileTagReader>,
    ) -> Result<FileTagSnapshotRead, crate::ImportError> {
        self.require_release_candidate(candidate_key).await?;
        let Some(stored) =
            self.services.tables.load_candidate_file_tag_snapshot(candidate_key).await?
        else {
            return Err(crate::ImportError::Internal {
                detail: format!("{candidate_key} is not an actionable folder candidate"),
            });
        };
        let crate::tables::DbCandidateFileTagSnapshot {
            scan_generation,
            candidate,
            snapshot: stored_snapshot,
        } = stored;
        let audio_files = candidate.files.audio().cloned().collect::<Vec<_>>();
        let file_edit_revision = candidate.file_edit_revision;
        let snapshot = self
            .services
            .diagnostics
            .blocking("read a candidate's file tags", move || {
                let observations = crate::file_tag_snapshot::observe_audio_files(&audio_files)?;
                if let Some(snapshot) = stored_snapshot.filter(|snapshot| {
                    file_tag_snapshot_match(
                        snapshot,
                        scan_generation,
                        file_edit_revision,
                        &observations,
                    ) == FileTagSnapshotMatch::Current
                }) {
                    return Ok::<_, crate::ImportError>(snapshot);
                }
                crate::file_tag_snapshot::extract_file_tag_snapshot(
                    &audio_files,
                    scan_generation,
                    file_edit_revision,
                    reader.as_ref(),
                )
            })
            .await??;
        Ok(FileTagSnapshotRead { candidate, snapshot })
    }
}

#[cfg(test)]
#[path = "file_tags_tests.rs"]
mod tests;
