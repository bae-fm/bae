use super::*;

/// The running queue sweep.
#[derive(Clone)]
pub struct QueueSweepHandle {
    context: SweepContext,
    token: CancellationToken,
    tasks: TaskTracker,
    runtime_handle: tokio::runtime::Handle,
    executor_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
}

impl QueueSweepHandle {
    pub(super) fn new(
        context: SweepContext,
        token: CancellationToken,
        tasks: TaskTracker,
        runtime_handle: tokio::runtime::Handle,
        executor_thread: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            context,
            token,
            tasks,
            runtime_handle,
            executor_thread: Arc::new(Mutex::new(Some(executor_thread))),
        }
    }

    /// Stop sweeping and cancel every in-flight candidate it owns.
    pub fn stop(&self) {
        self.token.cancel();
        self.tasks.close();
        let Some(executor_thread) = self.executor_thread.lock().unwrap().take() else {
            return;
        };
        if executor_thread.join().is_err() {
            warn!("queue sweep executor thread panicked during shutdown");
        }
    }

    /// Identify a candidate after a person explicitly enters Lookup,
    /// regardless of the automatic-lookup setting.
    ///
    /// The key is marked as queued for the whole time this is deciding, so a
    /// person who clicked sees their candidate waiting rather than nothing —
    /// and the mark goes whether or not a run comes of it.
    pub fn identify_for_explicit_lookup(&self, candidate_key: String) {
        if self.token.is_cancelled() {
            return;
        }
        let this = self.clone();
        self.tasks.spawn_on(
            async move {
                this.context
                    .import
                    .queue_explicit_identification(&candidate_key);
                this.enter_explicit_lookup(&candidate_key).await;
                this.context
                    .import
                    .clear_explicit_identification(&candidate_key);
            },
            &self.runtime_handle,
        );
    }

    /// Start a run for a candidate a person just opened, unless its answer is
    /// already stored or something is already answering it.
    async fn enter_explicit_lookup(&self, candidate_key: &str) {
        let Some(candidate) = actionable_candidate(&self.context, candidate_key).await else {
            warn!(
                "cannot start Lookup for {candidate_key}: \
                 it is not a folder candidate"
            );
            return;
        };
        let has_stored_verdict = match has_stored_verdict(&self.context, candidate_key).await {
            Ok(stored) => stored,
            Err(error) => {
                warn!("cannot read the stored verdict for {candidate_key}: {error}");
                return;
            }
        };
        if has_stored_verdict || self.context.identification_in_flight(candidate_key) {
            return;
        }
        let Some(start) = run_start(&self.context, &candidate).await else {
            return;
        };
        self.start_explicit_lookup_run(candidate_key.to_string(), candidate, start);
    }

    /// Re-run an explicit Lookup without consulting its stored verdict.
    pub fn rerun_for_explicit_lookup(&self, candidate_key: String) {
        let this = self.clone();
        self.tasks.spawn_on(
            async move {
                this.context
                    .import
                    .queue_explicit_identification(&candidate_key);
                this.rerun_explicit_lookup(&candidate_key).await;
                this.context
                    .import
                    .clear_explicit_identification(&candidate_key);
            },
            &self.runtime_handle,
        );
    }

    async fn rerun_explicit_lookup(&self, candidate_key: &str) {
        let Some(candidate) = actionable_candidate(&self.context, candidate_key).await else {
            warn!(
                "cannot re-run Lookup for {candidate_key}: \
                 it is not a folder candidate"
            );
            return;
        };
        let Some(start) = run_start(&self.context, &candidate).await else {
            return;
        };
        self.start_explicit_lookup_run(candidate_key.to_string(), candidate, start);
    }

    fn start_explicit_lookup_run(
        &self,
        candidate_key: String,
        candidate: ReleaseCandidate,
        start: CandidateRunStart,
    ) {
        let run = self.context.identify.new_run();
        self.record_explicit_lookup(
            run,
            candidate_key.clone(),
            candidate.clone(),
            start.metadata_revision,
        );
        self.context.identify.start(
            run,
            candidate_key.clone(),
            CallPriority::Interactive,
            start.choices,
        );
        self.context.extraction.start(
            candidate_key,
            ExtractionSource::Candidate {
                candidate: candidate.clone(),
            },
            CallPriority::Interactive,
        );
    }

    /// Persist the verdict of an explicit Lookup after its lead documents have
    /// been stored.
    fn record_explicit_lookup(
        &self,
        run: IdentifyRunId,
        candidate_key: String,
        candidate: ReleaseCandidate,
        expected_metadata_revision: u64,
    ) {
        let context = self.context.clone();
        let token = self.token.child_token();
        if self.token.is_cancelled() {
            return;
        }
        self.tasks.spawn_on(
            async move {
                record_explicit_lookup_verdict(
                    &context,
                    run,
                    candidate_key,
                    candidate,
                    expected_metadata_revision,
                    &token,
                )
                .await;
            },
            &self.runtime_handle,
        );
    }
}

/// What a run of `candidate` starts from, or nothing when the stored row it
/// states cannot be read. A run started without it would ask about signals the
/// person took out and answer a metadata revision nobody checked, so it does
/// not start at all.
async fn run_start(
    context: &SweepContext,
    candidate: &ReleaseCandidate,
) -> Option<CandidateRunStart> {
    match candidate_run_start(context, candidate).await {
        Ok(start) => Some(start),
        Err(error) => {
            warn!(
                "cannot start Lookup for {}: its stored state does not read ({error})",
                candidate.key()
            );
            None
        }
    }
}
