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
    pub fn identify_for_explicit_lookup(&self, candidate_key: String) {
        if self.token.is_cancelled() {
            return;
        }
        let this = self.clone();
        self.tasks.spawn_on(
            async move {
                let Some(candidate) = actionable_candidate(&this.context, &candidate_key).await
                else {
                    warn!(
                        "cannot start Lookup for {candidate_key}: \
                         it is not a folder candidate"
                    );
                    return;
                };
                let has_stored_verdict =
                    match has_stored_verdict(&this.context, &candidate_key).await {
                        Ok(stored) => stored,
                        Err(error) => {
                            warn!("cannot read the stored verdict for {candidate_key}: {error}");
                            return;
                        }
                    };
                if has_stored_verdict || this.context.identify.is_running(&candidate_key) {
                    return;
                }
                this.start_explicit_lookup_run(candidate_key, candidate);
            },
            &self.runtime_handle,
        );
    }

    /// Re-run an explicit Lookup without consulting its stored verdict.
    pub fn rerun_for_explicit_lookup(&self, candidate_key: String) {
        let this = self.clone();
        self.tasks.spawn_on(
            async move {
                let Some(candidate) = actionable_candidate(&this.context, &candidate_key).await
                else {
                    warn!(
                        "cannot re-run Lookup for {candidate_key}: \
                         it is not a folder candidate"
                    );
                    return;
                };
                this.start_explicit_lookup_run(candidate_key, candidate);
            },
            &self.runtime_handle,
        );
    }

    fn start_explicit_lookup_run(&self, candidate_key: String, candidate: FolderCandidate) {
        self.context
            .import
            .queue_explicit_identification(&candidate_key);
        let run = self.context.identify.new_run();
        self.record_explicit_lookup(run, candidate_key.clone(), candidate.clone());
        self.context
            .identify
            .start(run, candidate_key.clone(), CallPriority::Interactive);
        self.context.extraction.start(
            candidate_key,
            ExtractionSource::Folder {
                path: candidate.path,
                files: candidate.files,
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
        candidate: FolderCandidate,
    ) {
        let context = self.context.clone();
        let token = self.token.child_token();
        if self.token.is_cancelled() {
            return;
        }
        self.tasks.spawn_on(
            async move {
                record_explicit_lookup_verdict(&context, run, candidate_key, candidate, &token)
                    .await;
            },
            &self.runtime_handle,
        );
    }
}
