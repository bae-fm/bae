//! What [`crate::library::AppServices`] holds: the running queue, and the one
//! way a person asks for a candidate to be identified.

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

    /// Tell the queue to stop, and return. Every run it has going is cancelled
    /// on its own loop, and the answers already being written are told to
    /// abandon themselves before they write.
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

    /// Identify this candidate now, whatever is stored and whatever is running.
    ///
    /// The one way a person starts identification. A run takes its inputs once,
    /// at its start, so asking for one is asking for a fresh run: the
    /// candidate's run and the signal extraction feeding it are torn down
    /// before this one starts.
    ///
    /// The key is marked as waiting for the whole time the queue is deciding,
    /// so a person who pressed sees their candidate waiting rather than
    /// nothing. A run that starts takes the mark off itself, in its first
    /// broadcast; the mark is cleared only when no run came of it, so there is
    /// no instant in which the candidate is neither waiting nor running.
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

    /// Stop identifying these candidates, whether they are waiting, running,
    /// or having their answer written, or a run the queue did not start. Each
    /// one's whole job goes — candidates with the same files share one run —
    /// and it is left unidentified: no verdict and no failure are stored. The
    /// cancel is stored with the candidate, so the automatic admission does
    /// not take it back up, this launch or the next; a person asking for it
    /// again, or a file decision that makes it a different question, lifts
    /// it. Returns once the cancel is stored and the jobs are gone.
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

    /// Stop every identification on the queue, as [`Self::cancel`] does for
    /// one.
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
        cancelled: tokio::sync::oneshot::Receiver<Result<(), crate::library::LibraryError>>,
    ) -> Result<(), crate::library::LibraryError> {
        cancelled.await.map_err(|_| {
            crate::library::LibraryError::Internal(
                "the identification queue stopped before the cancel ended".to_string(),
            )
        })?
    }

    /// Run the automatic admission now, and wait until every job it is
    /// responsible for has ended — a verdict stored, refused, failed, or
    /// withdrawn. The whole of what one pass over the queue was.
    ///
    /// A candidate a person asked for does not hold this up: the automatic
    /// admission never had it.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn identify_the_queue_for_test(&self) {
        let (drained, wait) = tokio::sync::oneshot::channel();
        if self
            .commands
            .send(Command::AdmitAutomatic { drained })
            .is_err()
        {
            return;
        }
        // An error is the queue stopping, which is also the end of waiting for
        // anything it was doing.
        let _ = wait.await;
    }
}
