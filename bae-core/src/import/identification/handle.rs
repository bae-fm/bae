//! The handle [`crate::library::AppServices`] holds on the running queue.

use super::*;

/// The running identification queue.
#[derive(Clone)]
pub struct IdentificationHandle {
    context: Context,
    token: CancellationToken,
    tasks: TaskTracker,
    executor_thread: Arc<Mutex<Option<std::thread::JoinHandle<()>>>>,
    commands: mpsc::UnboundedSender<Command>,
}

impl IdentificationHandle {
    pub(super) fn new(
        context: Context,
        token: CancellationToken,
        tasks: TaskTracker,
        executor_thread: std::thread::JoinHandle<()>,
        commands: mpsc::UnboundedSender<Command>,
    ) -> Self {
        Self {
            context,
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
    /// running. It is marked waiting right away, so the person sees it queued.
    pub fn rerun_identify(&self, candidate_key: String) {
        if self.token.is_cancelled() {
            return;
        }
        self.context
            .import
            .admit_identification(vec![candidate_key.clone()], Admission::Requested);
        let sent = self.commands.send(Command::Request {
            candidate_key: candidate_key.clone(),
        });
        if sent.is_err() {
            warn!("identification: the queue has stopped; {candidate_key} was not identified");
            self.context
                .import
                .withdraw_identification(&candidate_key);
        }
    }

    /// Take these candidates' jobs off the queue, whatever they are doing, and
    /// store nothing. Returns once they are gone; fails only when the queue has
    /// stopped.
    pub async fn cancel(
        &self,
        candidate_keys: Vec<String>,
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
        self.commands.send(command).map_err(|_| {
            crate::library::LibraryError::Internal(
                "the identification queue has stopped".to_string(),
            )
        })
    }

    async fn await_cancel(
        cancelled: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), crate::library::LibraryError> {
        cancelled.await.map_err(|_| {
            crate::library::LibraryError::Internal(
                "the identification queue stopped before the cancel ended".to_string(),
            )
        })
    }

    /// Wait until every release found so far has been admitted and every
    /// automatic job has ended.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn automatic_drained_for_test(&self) {
        let (drained, wait) = tokio::sync::oneshot::channel();
        if self
            .commands
            .send(Command::AwaitAutomaticDrained { drained })
            .is_err()
        {
            return;
        }
        // A stopped queue ends the wait too.
        let _ = wait.await;
    }
}
