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

    /// Run this candidate now, whatever is stored and whatever is running.
    ///
    /// The one way a person starts identification. A run takes its inputs
    /// once, at its start, so asking for one is asking for a fresh run: the
    /// candidate's run and the signal extraction feeding it are torn down
    /// before this one starts. Starting supersedes the run on its own; what
    /// the cancel adds is the extraction, which nothing else ends — without
    /// it the previous run's artwork OCR would keep going beside the new
    /// run's.
    ///
    /// The key is marked as queued for the whole time this is deciding, so a
    /// person who pressed sees their candidate waiting rather than nothing.
    /// A run that starts takes the mark off itself, in its first broadcast;
    /// the mark is cleared here only when no run came of it, so there is no
    /// instant in which the candidate is neither waiting nor running.
    pub fn rerun_for_explicit_lookup(&self, candidate_key: String) {
        if self.token.is_cancelled() {
            return;
        }
        self.context.import.cancel_identification(&candidate_key);
        let this = self.clone();
        self.tasks.spawn_on(
            async move {
                this.context
                    .import
                    .queue_explicit_identification(&candidate_key);
                if !this.rerun_explicit_lookup(&candidate_key).await {
                    this.context
                        .import
                        .clear_explicit_identification(&candidate_key);
                }
            },
            &self.runtime_handle,
        );
    }

    /// Whether a run started for the candidate.
    async fn rerun_explicit_lookup(&self, candidate_key: &str) -> bool {
        let Some(candidate) = actionable_candidate(&self.context, candidate_key).await else {
            warn!(
                "cannot re-run Lookup for {candidate_key}: \
                 it is not a folder candidate"
            );
            return false;
        };
        let Some(start) = run_start(&self.context, &candidate).await else {
            return false;
        };
        self.start_explicit_lookup_run(candidate_key.to_string(), candidate, start)
    }

    fn start_explicit_lookup_run(
        &self,
        candidate_key: String,
        candidate: ReleaseCandidate,
        start: CandidateRunStart,
    ) -> bool {
        let run = self.context.import.new_identification_run();
        let recording = self.token.child_token();
        self.record_explicit_lookup(
            run,
            candidate_key.clone(),
            candidate.clone(),
            start.metadata_revision,
            recording.clone(),
        );
        if self.context.import.start_identification(
            run,
            candidate_key,
            ExtractionSource::Candidate { candidate },
            CallPriority::Interactive,
            start.choices,
        ) {
            return true;
        }
        // No run, so nothing will ever reach the recorder waiting for one.
        recording.cancel();
        false
    }

    /// Persist the verdict of an explicit Lookup after its lead documents have
    /// been stored.
    fn record_explicit_lookup(
        &self,
        run: IdentifyRunId,
        candidate_key: String,
        candidate: ReleaseCandidate,
        expected_metadata_revision: u64,
        token: CancellationToken,
    ) {
        let context = self.context.clone();
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
