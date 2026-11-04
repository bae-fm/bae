//! The commit lock a candidate's writes take, the standing they check under
//! it, and the task each durable write runs to completion on.

use super::*;

impl ImportServiceHandle {
    /// The stored candidate whose preparation may still be changed. A caller
    /// about to write holds `folder_state_commit` across this and the write so
    /// an import claim cannot land between them; without the lock it only
    /// refuses early.
    pub(super) async fn editable_candidate_for_commit(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<crate::folder_scanner::FolderCandidate, crate::ImportError> {
        let candidate = self.require_release_candidate(key).await?;
        self.candidate_standing(key, &candidate).await?.editable()?;
        Ok(candidate)
    }

    /// Whether the stored candidate at `key` is set aside, already in the
    /// library, or claimed by a running import — the claim read from its
    /// standing, which a caller about to write reads under the commit lock.
    /// Read from the tables because the list's query lands after the commit
    /// it reflects.
    pub(crate) async fn candidate_standing(
        &self,
        key: &crate::CandidateKey,
        candidate: &crate::folder_scanner::FolderCandidate,
    ) -> Result<crate::CandidateStanding, crate::ImportError> {
        Ok(crate::CandidateStanding {
            skipped: self.tables.is_release_candidate_skipped(candidate).await?,
            imported: self
                .tables
                .release_imported_from(&candidate.files.content_hash())
                .await?
                .is_some(),
            claimed: self.runtime.import_claimed(key).await?,
        })
    }

    /// Take `folder_state_commit` and recheck under it that the candidate is
    /// still at the revision the edit was prepared from. The caller holds the
    /// guard across its write; `operation` names it in the lock's log lines.
    pub(super) async fn commit_lock_for_revision(
        &self,
        operation: &'static str,
        key: &crate::CandidateKey,
        expected_content_hash: &str,
        expected_file_edit_revision: u64,
    ) -> Result<crate::FolderStateCommitGuard, crate::ImportError> {
        let commit = self.folder_state_commit.lock(operation).await;
        let candidate = self.editable_candidate_for_commit(key).await?;
        if candidate.files.content_hash() != expected_content_hash
            || candidate.file_edit_revision != expected_file_edit_revision
        {
            return Err(crate::ImportError::Internal {
                detail: format!("{key} changed before its edit could be stored"),
            });
        }
        Ok(commit)
    }

    /// Run a durable write to completion once it has been asked for.
    ///
    /// The write runs as a task on the import runtime and this only waits on
    /// it, so dropping the caller's future ends the wait and nothing else.
    /// Coven commits a write whether or not the future that asked survives, so
    /// the bookkeeping and lock release after it must run too. Fails through
    /// `From<TaskFailed>` only when the task panicked, which is reported as a
    /// bug, or the runtime is gone.
    pub(crate) async fn committed<V, E>(
        &self,
        write: impl std::future::Future<Output = Result<V, E>> + Send + 'static,
    ) -> Result<V, E>
    where
        V: Send + 'static,
        E: From<bae_util::task_failure::TaskFailed> + Send + 'static,
    {
        bae_util::task_failure::joined("a candidate write", self.runtime_handle.spawn(write))
            .await
            .map_err(|failed| self.diagnostics.bug(failed))?
    }
}

#[cfg(test)]
#[path = "commit_tests.rs"]
mod tests;
