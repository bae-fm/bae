//! The gates a command on a candidate refuses early by, and the task each
//! durable write runs to completion on. The write itself reads the
//! candidate's entry and standing again inside its transaction, and refuses
//! by them; [`crate::FolderStateCommit`] says which writes also take the
//! commit lock.

use super::*;

impl ImportServiceHandle {
    /// The release at `key` when it may be worked on and its preparation may
    /// still be changed.
    pub(super) async fn editable_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<crate::tables::CandidateEntry, crate::ImportError> {
        let entry = self.services.tables.require_workable(key).await?;
        entry.standing().editable()?;
        Ok(entry)
    }

    /// The candidate at `key` as an identification can still answer it: a
    /// stored, actionable release that is not set aside, imported or claimed
    /// by an import.
    pub(crate) async fn answerable_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<crate::folder_scanner::FolderCandidate>, crate::ImportError> {
        Ok(self
            .services
            .tables
            .load_candidate_entry(key)
            .await?
            .filter(|entry| entry.actionable() && entry.standing().answerable())
            .map(|entry| entry.candidate))
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
        let write = bae_util::task_failure::caught("a candidate write", write);
        #[expect(
            clippy::disallowed_methods,
            reason = "joined at once: the write's panic comes back at its join"
        )]
        let write = self.runtime_handle.spawn(write);
        bae_util::task_failure::joined("a candidate write", write)
            .await
            .map_err(|failed| self.services.diagnostics.bug(failed))?
    }
}

/// Where a stored candidate stands in the queue: whether it is set aside,
/// whether this device imported its files and whether an import claims it,
/// read with the candidate's entry as one statement. Every gate that decides
/// what may be done to a candidate reads this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateStanding {
    /// The person set it aside.
    pub skipped: bool,
    /// The release this device imported from its files, if it did.
    pub imported: Option<String>,
    /// An import has claimed it and is running.
    pub claimed: bool,
}

impl CandidateStanding {
    /// Whether identification may still answer for it: not set aside, not
    /// imported here, not being imported.
    pub fn answerable(&self) -> bool {
        !self.skipped && self.imported.is_none() && !self.claimed
    }

    /// Whether its preparation may still be changed. A skipped candidate may
    /// be: skipping sets it aside, it does not freeze it. An imported one
    /// holds no preparation: its files are the release they became.
    pub fn editable(&self) -> Result<(), crate::ImportError> {
        if let Some(release_id) = &self.imported {
            return Err(crate::ImportError::FilesAlreadyImported {
                release_id: release_id.clone(),
            });
        }
        if self.claimed {
            return Err(crate::ImportError::CandidateImportInProgress);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "commit_tests.rs"]
mod tests;
