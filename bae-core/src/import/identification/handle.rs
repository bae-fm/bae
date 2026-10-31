//! The handle [`crate::library::AppServices`] holds on the running queue.

use super::*;

/// The running identification queue.
#[derive(Clone)]
pub struct IdentificationHandle {
    token: CancellationToken,
    tasks: TaskTracker,
    executor_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    commands: mpsc::UnboundedSender<Command>,
}

impl IdentificationHandle {
    pub(super) fn new(
        token: CancellationToken,
        tasks: TaskTracker,
        executor_thread: std::thread::JoinHandle<()>,
        commands: mpsc::UnboundedSender<Command>,
    ) -> Self {
        Self {
            token,
            tasks,
            executor_thread: Arc::new(Mutex::new(Some(executor_thread))),
            commands,
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
        let Some(executor_thread) = self.executor_thread.lock().unwrap().take() else {
            return;
        };
        if executor_thread.join().is_err() {
            warn!("identification: the queue's executor thread panicked during shutdown");
        }
    }

    /// Identify this candidate now with a fresh run, whatever is stored or
    /// running. The queue marks it waiting as it takes the request: the queue
    /// is the one writer of that mark.
    pub fn rerun_identify(&self, candidate_key: crate::import::CandidateKey) {
        if self.token.is_cancelled() {
            return;
        }
        let sent = self.commands.send(Command::Request {
            candidate_key: candidate_key.clone(),
        });
        if sent.is_err() {
            warn!("identification: the queue has stopped; {candidate_key} was not identified");
        }
    }

    /// Take these candidates' jobs off the queue, whatever they are doing, and
    /// store nothing. Returns once they are gone; fails when the queue has
    /// stopped, or with the first ending of a candidate's standing that could
    /// not be written.
    pub async fn cancel(
        &self,
        candidate_keys: Vec<crate::import::CandidateKey>,
    ) -> Result<(), crate::library::LibraryError> {
        let (done, cancelled) = tokio::sync::oneshot::channel();
        self.send_cancel(Command::Cancel {
            candidate_keys,
            done,
        })?;
        Self::await_cancel(cancelled).await
    }

    /// [`Self::cancel`] every job on the queue.
    pub async fn cancel_all(&self) -> Result<(), crate::library::LibraryError> {
        let (done, cancelled) = tokio::sync::oneshot::channel();
        self.send_cancel(Command::CancelAll { done })?;
        Self::await_cancel(cancelled).await
    }

    fn send_cancel(&self, command: Command) -> Result<(), crate::library::LibraryError> {
        self.commands.send(command).map_err(|error| {
            crate::library::LibraryError::Internal(format!(
                "the identification queue has stopped: {error}"
            ))
        })
    }

    async fn await_cancel(
        cancelled: tokio::sync::oneshot::Receiver<Result<(), crate::library::LibraryError>>,
    ) -> Result<(), crate::library::LibraryError> {
        cancelled.await.map_err(|error| {
            crate::library::LibraryError::Internal(format!(
                "the identification queue stopped before the cancel ended: {error}"
            ))
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
        if self
            .commands
            .send(Command::AwaitDrained { drain, drained })
            .is_err()
        {
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
        if self
            .commands
            .send(Command::AwaitCommandsTaken { taken })
            .is_err()
        {
            return;
        }
        #[expect(
            clippy::let_underscore_must_use,
            reason = "a stopped queue ends the wait as surely as a taken command"
        )]
        let _ = wait.await;
    }
}
