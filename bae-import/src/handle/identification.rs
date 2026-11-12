//! The identification a handle runs for its candidates: the queue that
//! decides which candidates are identified, starting and ending a run of the
//! identify driver with the extraction feeding it, and the runtime marks the
//! identification queue writes around each run.

use super::*;

impl ImportServiceHandle {
    /// Start the identification queue over this import service. There is
    /// one per service, since it takes the service's event feed.
    pub fn start_identification_queue(&self) -> crate::IdentificationHandle {
        crate::identification::start(
            self.clone(),
            self.services.store.clone(),
            self.services.tables.clone(),
            self.services.diagnostics.clone(),
            self.services.catalogs.clone(),
        )
    }

    /// The id of a run about to start. Separate from
    /// [`Self::start_identification`] so a consumer can subscribe to this
    /// handle's bus knowing which run it is waiting for before that run's
    /// first state is sent.
    pub fn new_identification_run(&self) -> crate::IdentifyRunId {
        self.identify.new_run()
    }

    /// Identify `key` as `run`: the driver that asks the providers, and the
    /// extraction that feeds it the disc ID, barcodes and text `source`
    /// yields. Fire-and-forget; both report on this handle's event bus.
    ///
    /// One candidate is identified at a time, so this supersedes whatever was
    /// identifying `key` already. Extraction starts first and hands out the
    /// watch the driver reads its snapshots off: the watch holds the latest
    /// snapshot, so nothing the extraction says before the driver is up is
    /// lost, and nothing another extraction says reaches it.
    ///
    /// Reports whether a run started. A candidate with no source to ask gets
    /// none, and nothing will ever report on `run`, so whoever asked for it
    /// has to stop waiting for it — and the extraction started to feed that
    /// run is stopped here, since nothing is left to read it, as it is when
    /// the sources to ask can't be read, which fails.
    pub async fn start_identification(
        &self,
        run: crate::IdentifyRunId,
        key: crate::CandidateKey,
        source: crate::extraction::ExtractionSource,
        class: bae_catalogs::rate_limiter::CallClass,
        choices: bae_identify::LookupChoices,
        title_search: Option<bae_identify::TitleSearch>,
    ) -> Result<bool, crate::ImportError> {
        let subject = match &source {
            crate::extraction::ExtractionSource::Candidate { candidate } => {
                crate::candidate_runtime::RunSubject::Candidate(crate::CandidateFiles::of(
                    candidate,
                ))
            }
            crate::extraction::ExtractionSource::Release { .. } => {
                crate::candidate_runtime::RunSubject::Release
            }
        };
        let snapshots = self.extraction.start(run, key.clone(), source);
        let started = self
            .identify
            .start(run, key.clone(), subject, class, choices, title_search, snapshots)
            .await;
        if !matches!(started, Ok(true)) {
            self.extraction.cancel(&key);
        }
        Ok(started?)
    }

    /// Stop identifying this candidate: the run and the extraction feeding it.
    ///
    /// A decision about a candidate ends its identification inside the
    /// decision's own write and then calls [`Self::identification_ended`];
    /// this is the cancel a person asks for, or the queue makes, with no
    /// other write to end it in. A candidate with nothing running is
    /// unchanged by this.
    ///
    /// The one cancellation that is not a command is a reshaped or vanished
    /// candidate: it has no single decision point, so both halves cancel it
    /// off their own listeners on the scan's events.
    ///
    /// A cancel that cannot write where the candidate stands fails, and the
    /// candidate still reads as identifying: whoever asked for it is told.
    pub(crate) async fn cancel_identification(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        self.identify.cancel(candidate_key);
        self.extraction.cancel(candidate_key);
        // Nothing is going to write what the cancelled run reached, including
        // when it had already answered: a run left holding an answer nobody
        // disposes of reads as a commit still pending, for good. It is also the
        // only ending a library release re-identified in its own sheet gets —
        // its run stores no verdict, so no write would end it.
        Ok(self.runtime.cancel_identification(candidate_key).await?)
    }

    /// A decision about this candidate ended its identification, and what
    /// its run published, inside the decision's own write: the run and the
    /// extraction feeding it stop.
    pub(crate) async fn identification_ended(&self, candidate_key: &crate::CandidateKey) {
        self.identify.cancel(candidate_key);
        self.extraction.cancel(candidate_key);
        #[cfg(any(test, feature = "test-utils"))]
        self.runtime.tell_every_change(std::iter::once(candidate_key)).await;
    }

    /// Stop only the extraction behind `key`, for a run that reached its
    /// answer and ended on its own. Ending the run here would tear down
    /// whatever has taken the key over since that answer landed.
    pub(crate) fn cancel_candidate_extraction(&self, candidate_key: &crate::CandidateKey) {
        self.extraction.cancel(candidate_key);
    }

    /// Whether a run is in flight for `key`. A run that reached its verdict
    /// and one that was cancelled are both gone: the driver deregisters itself
    /// the moment it stops working.
    ///
    /// Nothing in the app asks: the identification queue is the only thing that
    /// starts a candidate's run, and its own entry says what that run is doing.
    /// A test asks to check that from the outside.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn is_identifying(&self, key: &crate::CandidateKey) -> bool {
        self.identify.is_running(key)
    }

    /// Every key with a run in flight right now. What a change to the inputs
    /// every run reads — the library's provider list — has to act on: those
    /// runs answer the list as it was, and nothing else does.
    pub fn identifying_keys(&self) -> Vec<crate::CandidateKey> {
        self.identify.running_keys()
    }

    /// Register the platform's artwork analyzer, which extraction reads
    /// barcodes and text off a candidate's images with. Called once at boot,
    /// on the platforms that ship one.
    pub fn register_artwork_analyzer(&self, analyzer: Arc<dyn crate::extraction::ArtworkAnalyzer>) {
        self.extraction.register_analyzer(analyzer);
    }

    /// These candidates are waiting on `admission` for runs that have not
    /// started yet.
    pub(crate) async fn admit_identification(
        &self,
        candidates: Vec<(crate::CandidateKey, crate::CandidateFiles)>,
        admission: crate::Admission,
    ) -> Result<(), crate::ImportError> {
        Ok(self.runtime.admit(candidates, admission).await?)
    }

    /// This key is not waiting any more.
    pub(crate) async fn withdraw_identification(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        Ok(self.runtime.withdraw(candidate_key).await?)
    }

    /// A terminal result `run` reached for `files` could not be committed.
    pub(crate) async fn fail_identification(
        &self,
        candidate_key: &crate::CandidateKey,
        run: crate::IdentifyRunId,
        files: &crate::CandidateFiles,
        failure: crate::SaveFailure,
    ) -> Result<(), crate::ImportError> {
        Ok(self.runtime.fail_identification(candidate_key, run, files, failure).await?)
    }

    /// No run of `files` could start for `candidate_key`: the standing says
    /// why, and the pane's Retry identifies it again.
    pub(crate) async fn fail_identification_start(
        &self,
        candidate_key: &crate::CandidateKey,
        files: &crate::CandidateFiles,
        failure: crate::SaveFailure,
    ) -> Result<(), crate::ImportError> {
        Ok(self.runtime.fail_identification_start(candidate_key, files, failure).await?)
    }

    /// `run`'s answer is disposed of with no write ever asked for it. An
    /// Import asked for while its pick was read goes with it: no pick is
    /// stored, so nothing imports.
    pub(crate) async fn end_identification_answer(
        &self,
        candidate_key: &crate::CandidateKey,
        run: crate::IdentifyRunId,
    ) -> Result<(), crate::ImportError> {
        let import_asked = self.runtime.end_identification_answer(candidate_key, run).await?;
        if import_asked {
            info!("identification: {candidate_key}'s answer was given up; the Import asked of it is dropped");
        }
        Ok(())
    }

    /// The signals `run` was judged against, as its rows hold them — what a
    /// settle of that run's answer stores beside its verdict.
    pub(crate) async fn candidate_run_signals(
        &self,
        candidate_key: &crate::CandidateKey,
        run: crate::IdentifyRunId,
    ) -> Result<Option<bae_identify::signals::Signals>, bae_store::DbError> {
        self.services.tables.load_candidate_run_signals(candidate_key, run).await
    }
}

#[cfg(test)]
#[path = "identification_tests.rs"]
mod tests;
