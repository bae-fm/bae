//! The identification a handle runs for its candidates: starting and ending
//! a run of the identify driver with the extraction feeding it, and the
//! runtime marks the identification queue writes around each run.

use super::*;

impl ImportServiceHandle {
    /// The id of a run about to start. Separate from
    /// [`Self::start_identification`] so a consumer can subscribe to this
    /// handle's bus knowing which run it is waiting for before that run's
    /// first state is broadcast.
    pub(crate) fn new_identification_run(&self) -> crate::identify::IdentifyRunId {
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
    /// run is stopped here, since nothing is left to read it.
    #[must_use]
    pub(crate) fn start_identification(
        &self,
        run: crate::identify::IdentifyRunId,
        key: String,
        source: crate::signals::ExtractionSource,
        priority: crate::util::rate_limiter::CallPriority,
        choices: crate::import::LookupChoices,
        title_search: Option<crate::identify::TitleSearch>,
    ) -> bool {
        let snapshots = self.extraction.start(run, key.clone(), source, priority);
        if self
            .identify
            .start(run, key.clone(), priority, choices, title_search, snapshots)
        {
            return true;
        }
        self.extraction.cancel(&key);
        false
    }

    /// Stop identifying this candidate: the run and the extraction feeding it.
    ///
    /// A decision about a candidate ends its identification, and ends it at
    /// the command that decides — inside that command's own write, so an
    /// answer the superseded run was about to reach cannot land after the
    /// decision it was superseded by. A candidate with nothing running is
    /// unchanged by this.
    ///
    /// The one cancellation that is not a command is a reshaped or vanished
    /// candidate: it has no single decision point, so both halves cancel it
    /// off their own listeners on the scan's events.
    pub(crate) fn cancel_identification(&self, candidate_key: &str) {
        self.identify.cancel(candidate_key);
        self.extraction.cancel(candidate_key);
        // Nothing is going to write what the cancelled run reached, including
        // when it had already answered: a run left holding an answer nobody
        // disposes of reads as a commit still pending, for good. It is also the
        // only ending a library release re-identified in its own sheet gets —
        // its run stores no verdict, so no write would end it.
        self.runtime.end_identification(candidate_key);
    }

    /// Stop only the extraction behind `key`, for a run that reached its
    /// answer and ended on its own. Ending the run here would tear down
    /// whatever has taken the key over since that answer landed.
    pub(crate) fn cancel_candidate_extraction(&self, candidate_key: &str) {
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
    pub fn is_identifying(&self, key: &str) -> bool {
        self.identify.is_running(key)
    }

    /// Every key with a run in flight right now. What a change to the inputs
    /// every run reads — the library's provider list — has to act on: those
    /// runs answer the list as it was, and nothing else does.
    pub(crate) fn identifying_keys(&self) -> Vec<String> {
        self.identify.running_keys()
    }

    /// Register the platform's artwork analyzer, which extraction reads
    /// barcodes and text off a candidate's images with. Called once at boot,
    /// on the platforms that ship one.
    pub(crate) fn register_artwork_analyzer(
        &self,
        analyzer: Arc<dyn crate::signals::ArtworkAnalyzer>,
    ) {
        self.extraction.register_analyzer(analyzer);
    }

    /// This key is waiting on `admission` for a run that has not started yet.
    pub(crate) fn admit_identification(
        &self,
        candidate_keys: Vec<String>,
        admission: crate::import::Admission,
    ) {
        self.runtime.admit(candidate_keys, admission);
    }

    /// This key is not waiting any more.
    pub(crate) fn withdraw_identification(&self, candidate_key: &str) {
        self.runtime.withdraw(candidate_key);
    }

    /// A terminal result `run` reached could not be committed.
    pub(crate) fn fail_identification(
        &self,
        candidate_key: &str,
        run: crate::identify::IdentifyRunId,
        error: String,
    ) {
        self.runtime.fail_identification(candidate_key, run, error);
    }

    /// `run`'s answer is disposed of: its row landed, was refused as stale, or
    /// no write was ever asked for it.
    pub(crate) fn end_identification_answer(
        &self,
        candidate_key: &str,
        run: crate::identify::IdentifyRunId,
    ) {
        self.runtime.end_identification_answer(candidate_key, run);
    }

    /// The settled snapshot `run` was judged against — what a settle of that
    /// run's answer stores beside its verdict.
    pub(crate) fn candidate_run_signals(
        &self,
        candidate_key: &str,
        run: crate::identify::IdentifyRunId,
    ) -> Option<crate::signals::Signals> {
        self.runtime.run_signals(candidate_key, run)
    }

    /// The signals extraction has found for one key so far. `None` before the
    /// first snapshot, and for a key whose run settled in an earlier session —
    /// what that run stored is on the candidate's row instead.
    ///
    /// The read a form does once when it opens, after it has subscribed to
    /// the UI bus, so a form opened partway through a run has the pool the
    /// run has built rather than an empty one.
    pub fn candidate_signals(&self, key: &str) -> Option<crate::signals::Signals> {
        self.runtime.signals(key)
    }
}
