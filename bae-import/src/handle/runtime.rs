//! What is happening right now for the candidates, as the handle reads it:
//! each key's runtime from the standings table and the runtime, the import
//! percents, one candidate by key with its runtime joined, and the test
//! hooks that move or watch them.

use super::*;
use crate::tables::ScannedEntry;

impl ImportServiceHandle {
    /// What is in flight for one key right now, read from the standings table
    /// and the runtime.
    pub async fn candidate_runtime(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<CandidateRuntimeSnapshot>, crate::ImportError> {
        self.runtime.snapshot(key).await
    }

    /// Where one key stands, as its row reads it from the standings table, for
    /// a test.
    #[cfg(test)]
    pub(crate) async fn standing_facts_for_test(
        &self,
        key: &crate::CandidateKey,
    ) -> crate::TriageRuntimeFacts {
        self.runtime.standing_facts(key).await.expect("a test reads the standings table")
    }

    /// Put `key` where a run's answer being written puts it — queued, `run`
    /// started and then answered with `answer` — as the queue and the
    /// identify driver would, for a test of what disposes of that answer.
    #[cfg(test)]
    pub(crate) async fn hold_answer_for_test(
        &self,
        key: &crate::CandidateKey,
        run: crate::IdentifyRunId,
        answer: &bae_identify::IdentifyState,
    ) {
        let candidate =
            self.get_release_candidate(key).await.unwrap().expect("the candidate is stored");
        let files = crate::CandidateFiles::of(&candidate);
        self.runtime
            .admit(vec![(key.clone(), files.clone())], crate::Admission::Requested)
            .await
            .unwrap();
        assert!(self.runtime.run_started(key, &files, run, answer).await.unwrap());
        assert!(self.runtime.run_answered(key, run, answer.picks_unattended()).await.unwrap());
    }

    /// Each running import's percent, by candidate key, as its import last
    /// reported.
    pub fn import_percents(&self) -> HashMap<crate::CandidateKey, u32> {
        self.runtime.percents()
    }

    /// Each running import's percent, by candidate key, now and as it moves.
    pub fn watch_import_percents(&self) -> crate::ImportPercentsWatch {
        self.runtime.watch_import_percents()
    }

    /// Every key's runtime as each write of its standing leaves it, in the
    /// order they land, from now on, for a test that checks that order.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_runtime_change_for_test(&self) -> crate::CandidateRuntimeChanges {
        self.runtime.every_change()
    }

    /// Every move of the identification count from now on, for a test.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn every_identification_count_for_test(&self) -> crate::IdentificationCounts {
        self.runtime.every_count()
    }

    /// One candidate by key, read from the tables with its runtime joined. A
    /// key with runtime but no scanned folder (a library release being
    /// re-identified) answers with its runtime alone.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn get_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<ImportCandidateSnapshot>, crate::ImportError> {
        let entry = match self.services.tables.load_scanned_entry(key).await? {
            Some(ScannedEntry::InvalidFolder) => {
                return Ok(self
                    .services
                    .tables
                    .load_invalid_row(key)
                    .await?
                    .map(ImportCandidateSnapshot::Invalid));
            }
            Some(ScannedEntry::Release(entry)) => Some(*entry),
            None => None,
        };
        let runtime = self.runtime.snapshot(key).await?;
        if let Some(entry) = entry {
            return Ok(Some(ImportCandidateSnapshot::Folder {
                candidate: entry.candidate,
                runtime,
                actionable: entry.settled,
                skipped: entry.skipped,
                is_added: entry.imported.is_some(),
            }));
        }
        Ok(runtime
            .map(|runtime| ImportCandidateSnapshot::Runtime { key: key.to_string(), runtime }))
    }

    /// Whether the import tables hold a candidate under `key`: a release the
    /// import list shows, or a folder that failed validation and that the
    /// list shows.
    pub async fn holds_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<bool, crate::ImportError> {
        Ok(match self.services.tables.load_scanned_entry(key).await? {
            Some(ScannedEntry::Release(entry)) => entry.listed(),
            Some(ScannedEntry::InvalidFolder) => {
                self.services.tables.load_invalid_row(key).await?.is_some()
            }
            None => false,
        })
    }
}

/// One candidate by key, with its runtime joined: the read behind every
/// "what is this key right now" question.
#[derive(Debug, Clone)]
pub enum ImportCandidateSnapshot {
    Folder {
        candidate: FolderCandidate,
        /// `None` when nothing is running for this key.
        runtime: Option<CandidateRuntimeSnapshot>,
        actionable: bool,
        skipped: bool,
        is_added: bool,
    },
    Invalid(crate::triage::InvalidRow),
    /// A key with runtime but no scanned folder — a library release being
    /// re-identified.
    Runtime {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
}
