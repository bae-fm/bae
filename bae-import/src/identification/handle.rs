//! The handle the app holds on the running queue.

use super::*;

/// The running identification queue.
#[derive(Clone)]
pub struct IdentificationHandle {
    token: CancellationToken,
    tasks: TaskTracker,
    executor_thread: Arc<Mutex<Option<bae_diagnostics::ReportedThread>>>,
    commands: mpsc::UnboundedSender<Command>,
    /// The import service the queue runs over, for the pane's identification
    /// commands.
    import: ImportServiceHandle,
}

impl IdentificationHandle {
    pub(super) fn new(
        token: CancellationToken,
        tasks: TaskTracker,
        executor_thread: bae_diagnostics::ReportedThread,
        commands: mpsc::UnboundedSender<Command>,
        import: ImportServiceHandle,
    ) -> Self {
        Self {
            token,
            tasks,
            executor_thread: Arc::new(Mutex::new(Some(executor_thread))),
            commands,
            import,
        }
    }

    /// Tell the queue to stop, and return without waiting.
    pub fn shut_down(&self) {
        self.token.cancel();
        self.tasks.close();
    }

    /// Stop the queue and wait for what it has in flight to end.
    pub fn stop(&self) {
        self.shut_down();
        let Some(executor_thread) =
            self.executor_thread.lock().expect("executor thread lock poisoned").take()
        else {
            return;
        };
        executor_thread.join();
    }

    /// Identify this candidate now with a fresh run, whatever is stored or
    /// running. The queue marks it waiting as it takes the request: the queue
    /// is the one writer of that mark.
    pub fn rerun_identify(&self, candidate_key: crate::CandidateKey) {
        if self.token.is_cancelled() {
            return;
        }
        let sent = self.commands.send(Command::Request { candidate_key: candidate_key.clone() });
        if sent.is_err() {
            warn!("identification: the queue has stopped; {candidate_key} was not identified");
        }
    }

    /// Make one change to what the candidate's identification asks about or
    /// counts, and run it again when what it looks up changed.
    pub async fn pane_edit_lookup_choices(
        &self,
        candidate_key: crate::CandidateKey,
        edit: bae_identify::LookupChoiceEdit,
    ) -> Result<crate::PaneOutcome, crate::ImportError> {
        use bae_identify::LookupChoiceEdit;
        let command = match &edit {
            LookupChoiceEdit::ToggleDiscId
            | LookupChoiceEdit::ToggleBarcode { .. }
            | LookupChoiceEdit::ToggleCatalog { .. } => crate::PaneCommand::ChangeLookups,
            LookupChoiceEdit::SearchBy { .. } => crate::PaneCommand::ChangeSearchWords,
            LookupChoiceEdit::ToggleDiscounted { .. } => crate::PaneCommand::ChangeAgreements,
        };
        let key = candidate_key.clone();
        self.import
            .run_pane_command(
                &key,
                command,
                self.edit_candidate_lookup_choices(candidate_key, edit),
            )
            .await
    }

    /// Show identification's results for the candidate: the stored verdict as
    /// it stood, picked row and all, when there is one, and a run started
    /// when there is none. A stored verdict is not asked again: the person
    /// looks at what it found, and picks from it.
    pub async fn pane_identify_automatically(
        &self,
        candidate_key: crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        if self.import.open_automatic(&candidate_key).await? {
            self.rerun_identify(candidate_key);
        }
        Ok(())
    }

    /// Stop identifying the candidate — take it off the identification queue,
    /// ending its run if one started — storing nothing, and go back to the
    /// draft. The pane stays where it is when the cancel fails.
    pub async fn pane_cancel_identification(
        &self,
        candidate_key: &crate::CandidateKey,
    ) -> Result<(), crate::ImportError> {
        self.cancel(vec![candidate_key.clone()]).await?;
        self.import.move_candidate_pane(candidate_key, crate::PaneMove::Back).await
    }

    /// Make one change to what a candidate's identification asks about, and
    /// run it again when what it looks up has changed: a run takes its choices
    /// at its start, so a person changing one is asking for a run that reads
    /// it. Any run already going for this candidate is superseded. Striking
    /// a catalog number out is such a change: a struck number is searched by
    /// nobody.
    async fn edit_candidate_lookup_choices(
        &self,
        candidate_key: crate::CandidateKey,
        edit: bae_identify::LookupChoiceEdit,
    ) -> Result<(), crate::ImportError> {
        let change = self.import.edit_candidate_lookup_choices(&candidate_key, edit).await?;
        if change == bae_identify::ChoiceChange::Lookups {
            self.rerun_identify(candidate_key);
        }
        Ok(())
    }

    /// Where one pane says which candidate it shows, for as long as the pane
    /// lives.
    pub fn shown_candidate(&self) -> ShownCandidate {
        ShownCandidate { commands: self.commands.clone(), shown: None }
    }

    /// Take these candidates' jobs off the queue, whatever they are doing, and
    /// store nothing. Returns once they are gone; fails when the queue has
    /// stopped, or with the first ending of a candidate's standing that could
    /// not be written.
    pub async fn cancel(
        &self,
        candidate_keys: Vec<crate::CandidateKey>,
    ) -> Result<(), crate::ImportError> {
        let (done, cancelled) = tokio::sync::oneshot::channel();
        self.send_cancel(Command::Cancel { candidate_keys, done })?;
        Self::await_cancel(cancelled).await
    }

    /// [`Self::cancel`] every job on the queue.
    pub async fn cancel_all(&self) -> Result<(), crate::ImportError> {
        let (done, cancelled) = tokio::sync::oneshot::channel();
        self.send_cancel(Command::CancelAll { done })?;
        Self::await_cancel(cancelled).await
    }

    fn send_cancel(&self, command: Command) -> Result<(), crate::ImportError> {
        self.commands.send(command).map_err(|error| crate::ImportError::Internal {
            detail: format!("the identification queue has stopped: {error}"),
        })
    }

    async fn await_cancel(
        cancelled: tokio::sync::oneshot::Receiver<Result<(), crate::ImportError>>,
    ) -> Result<(), crate::ImportError> {
        cancelled.await.map_err(|error| crate::ImportError::Internal {
            detail: format!("the identification queue stopped before the cancel ended: {error}"),
        })?
    }

    /// Wait until every release found so far has been admitted and every
    /// automatic job has ended.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn automatic_drained_for_test(&self) {
        self.drained(super::Drain::Automatic).await;
    }

    /// Wait until every release found so far has been admitted and every job,
    /// automatic or asked for, has ended.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drained_for_test(&self) {
        self.drained(super::Drain::Every).await;
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn drained(&self, drain: super::Drain) {
        let (drained, wait) = tokio::sync::oneshot::channel();
        if self.commands.send(Command::AwaitDrained { drain, drained }).is_err() {
            return;
        }
        // A stopped queue ends the wait too.
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a stopped queue ends the wait as surely as a drained one"
        )]
        let _ = wait.await;
    }

    /// Wait until the queue has handled every command sent before this call
    /// and started what it has room for.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn commands_taken_for_test(&self) {
        let (taken, wait) = tokio::sync::oneshot::channel();
        if self.commands.send(Command::AwaitCommandsTaken { taken }).is_err() {
            return;
        }
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a stopped queue ends the wait as surely as a taken command"
        )]
        let _ = wait.await;
    }
}

/// The candidate one pane shows. While a pane shows a candidate, the queue
/// starts that candidate's job ahead of every other waiting job, and its runs'
/// calls are admitted ahead of every other call, those already waiting
/// included. Dropping it says the pane shows nothing.
pub struct ShownCandidate {
    commands: mpsc::UnboundedSender<Command>,
    /// What the queue was last told this pane shows.
    shown: Option<crate::CandidateKey>,
}

impl ShownCandidate {
    /// The pane shows `key` from now on, or nothing.
    pub fn show(&mut self, key: Option<crate::CandidateKey>) {
        if self.shown == key {
            return;
        }
        let was = std::mem::replace(&mut self.shown, key.clone());
        self.tell(was, key);
    }

    fn tell(&self, was: Option<crate::CandidateKey>, now: Option<crate::CandidateKey>) {
        // A stopped queue identifies nothing, so there is nothing to move
        // ahead.
        if self.commands.send(Command::Shown { was, now }).is_err() {
            debug!("identification: the queue has stopped; the pane's candidate goes untold");
        }
    }
}

impl Drop for ShownCandidate {
    fn drop(&mut self) {
        if let Some(was) = self.shown.take() {
            self.tell(Some(was), None);
        }
    }
}
