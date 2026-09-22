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
            .admit_identification(&candidate_key, Admission::Requested);
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
