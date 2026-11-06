//! What is happening right now for the candidates, as the handle reads it:
//! each key's runtime from the standings table and the runtime, the import
//! percents, one candidate by key with its runtime joined, and the test
//! hooks that move or watch them.

use super::*;

impl ImportServiceHandle {
    /// Every key with something in flight right now, read from the standings
    /// table and the runtime.
    pub async fn candidate_runtimes(
        &self,
    ) -> Result<HashMap<crate::CandidateKey, CandidateRuntimeSnapshot>, crate::ImportError> {
        self.runtime.snapshots().await
    }

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
    pub async fn get_candidate(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<ImportCandidateSnapshot>, crate::ImportError> {
        let stored = self.tables.load_folder_scan_item(key).await?;
        let candidate = match stored {
            Some(crate::folder_scanner::ScanItem::Valid(candidate)) => Some((candidate, true)),
            Some(crate::folder_scanner::ScanItem::Discovered(candidate)) => {
                Some((candidate, false))
            }
            Some(crate::folder_scanner::ScanItem::Invalid(candidate)) => {
                return Ok(Some(ImportCandidateSnapshot::Invalid(candidate)))
            }
            None => None,
        };
        let runtime = self.runtime.snapshot(key).await?;
        if let Some((candidate, actionable)) = candidate {
            let standing = self.candidate_standing(key, &candidate).await?;
            return Ok(Some(ImportCandidateSnapshot::Folder {
                candidate,
                runtime,
                actionable,
                skipped: standing.skipped,
                is_added: standing.imported.is_some(),
            }));
        }
        Ok(runtime
            .map(|runtime| ImportCandidateSnapshot::Runtime { key: key.to_string(), runtime }))
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
    Invalid(InvalidCandidate),
    /// A key with runtime but no scanned folder — a library release being
    /// re-identified.
    Runtime {
        key: String,
        runtime: CandidateRuntimeSnapshot,
    },
}
