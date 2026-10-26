//! The queue, and the one loop that owns it. Surfaces read what the queue
//! publishes into the candidate runtime.

use super::*;
use std::collections::VecDeque;
use tokio::task::JoinSet;

mod reactions;

use reactions::{finish, handle_events};

/// One candidate on the queue, and which admission put it there.
struct Entry {
    candidate: FolderCandidate,
    admission: Admission,
}

/// Every entry that shares one identity. One member runs, and its stored
/// answer covers them all.
struct Job {
    identity: CandidateFiles,
    /// At least one: a job is removed with its last member.
    members: Vec<Entry>,
    state: JobState,
}

/// Where a job stands. `representative` is the member whose run answers for
/// the job.
enum JobState {
    Waiting,
    Running {
        representative: String,
        run: IdentifyRunId,
    },
    Settling {
        representative: String,
        run: IdentifyRunId,
        /// Tells the settle to give its answer up before it writes.
        abandon: CancellationToken,
    },
}

impl Job {
    /// A job is a request when any of its members is.
    fn admission(&self) -> Admission {
        if self
            .members
            .iter()
            .any(|member| member.admission == Admission::Requested)
        {
            Admission::Requested
        } else {
            Admission::Automatic
        }
    }

    fn priority(&self) -> CallPriority {
        match self.admission() {
            Admission::Requested => CallPriority::Interactive,
            Admission::Automatic => CallPriority::Background,
        }
    }

    fn holds(&self, key: &str) -> bool {
        self.members
            .iter()
            .any(|member| member.candidate.key() == key)
    }

    fn keys(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|member| member.candidate.key())
            .collect()
    }

    /// Whether a run of this job holds one of the queue's slots. A settling
    /// job holds none: its run has answered, and reading the release the
    /// answer picks runs beside the queue rather than ahead of the next run.
    fn holds_a_slot(&self) -> bool {
        match self.state {
            JobState::Running { .. } => true,
            JobState::Waiting | JobState::Settling { .. } => false,
        }
    }

    /// The member whose run is answering this job, while one is running.
    fn running_representative(&self) -> Option<String> {
        match &self.state {
            JobState::Running { representative, .. } => Some(representative.clone()),
            JobState::Waiting | JobState::Settling { .. } => None,
        }
    }

    /// Every member with the files it is identified as.
    fn marks(&self) -> Vec<(String, CandidateFiles)> {
        self.members
            .iter()
            .map(|member| (member.candidate.key(), CandidateFiles::of(&member.candidate)))
            .collect()
    }

    /// Mark every member as waiting again; a run's first report took its
    /// representative's mark off.
    async fn mark_waiting(&self, context: &Context) {
        record(
            context,
            "mark a job's candidates waiting",
            context
                .import
                .admit_identification(self.marks(), self.admission())
                .await,
        );
    }
}

/// How the queue holds one key.
struct Held {
    admission: Admission,
    identity: CandidateFiles,
    /// Whether a run of this key's own is answering or being written.
    running: bool,
}

/// Every identification that is waiting, running, or being written.
#[derive(Default)]
pub(super) struct Queue {
    /// The jobs in the order they run: requests first, each in the order it
    /// was admitted.
    jobs: VecDeque<Job>,
}

impl Queue {
    fn index_of_key(&self, key: &str) -> Option<usize> {
        self.jobs.iter().position(|job| job.holds(key))
    }

    fn index_of_identity(&self, identity: &CandidateFiles) -> Option<usize> {
        self.jobs.iter().position(|job| &job.identity == identity)
    }

    /// How the queue holds `key`, if it does.
    fn held(&self, key: &str) -> Option<Held> {
        let job = &self.jobs[self.index_of_key(key)?];
        let member = job
            .members
            .iter()
            .find(|member| member.candidate.key() == key)
            .expect("the located job holds the key");
        let running = match &job.state {
            JobState::Waiting => false,
            JobState::Running { representative, .. }
            | JobState::Settling { representative, .. } => representative == key,
        };
        Some(Held {
            admission: member.admission,
            identity: job.identity.clone(),
            running,
        })
    }

    /// Whether no job `drain` names is left.
    #[cfg(any(test, feature = "test-utils"))]
    fn is_drained(&self, drain: super::Drain) -> bool {
        match drain {
            super::Drain::Automatic => !self
                .jobs
                .iter()
                .any(|job| job.admission() == Admission::Automatic),
            super::Drain::Every => self.jobs.is_empty(),
        }
    }

    /// How many jobs hold a slot: the ones whose runs are going.
    fn running_count(&self) -> usize {
        self.jobs.iter().filter(|job| job.holds_a_slot()).count()
    }

    /// Put every candidate on the queue under `admission` and mark them all in
    /// one change. Returns the content hashes placed.
    ///
    /// An automatic job goes to the back and a request goes ahead of every
    /// waiting automatic job. A candidate already held as this identity is
    /// left alone by an automatic admission and restarted by a request; one
    /// held as another identity is taken out first.
    async fn place(
        &mut self,
        context: &Context,
        candidates: Vec<FolderCandidate>,
        admission: Admission,
    ) -> Vec<String> {
        let mut marked = Vec::new();
        let mut content_hashes = Vec::new();
        for candidate in candidates {
            let key = candidate.key();
            let identity = CandidateFiles::of(&candidate);
            if let Some(index) = self.index_of_key(&key) {
                if admission == Admission::Automatic && self.jobs[index].identity == identity {
                    continue;
                }
                self.take_out(context, &key).await;
            }
            content_hashes.push(identity.content_hash.clone());
            let mark = (key, identity.clone());
            let entry = Entry {
                candidate,
                admission,
            };
            match (self.index_of_identity(&identity), admission) {
                (Some(index), Admission::Automatic) => self.jobs[index].members.push(entry),
                (Some(index), Admission::Requested) => {
                    let job = &mut self.jobs[index];
                    // The person's candidate is the one whose files the run reads.
                    job.members.insert(0, entry);
                    let running = job.running_representative();
                    job.state = JobState::Waiting;
                    if let Some(representative) = running {
                        cancel(context, &representative).await;
                    }
                    let job = self
                        .jobs
                        .remove(index)
                        .expect("the located job still exists");
                    job.mark_waiting(context).await;
                    let position = self.request_position();
                    self.jobs.insert(position, job);
                }
                (None, _) => {
                    let job = Job {
                        identity,
                        members: vec![entry],
                        state: JobState::Waiting,
                    };
                    match admission {
                        Admission::Automatic => self.jobs.push_back(job),
                        Admission::Requested => {
                            let position = self.request_position();
                            self.jobs.insert(position, job);
                        }
                    }
                }
            }
            marked.push(mark);
        }
        record(
            context,
            "mark the admitted candidates waiting",
            context.import.admit_identification(marked, admission).await,
        );
        content_hashes
    }

    /// Remove `key` from whatever job holds it, ending the run it was
    /// answering for, and leave its mark to the caller.
    async fn take_out(&mut self, context: &Context, key: &str) {
        let Some(index) = self.index_of_key(key) else {
            return;
        };
        let job = &mut self.jobs[index];
        let position = job
            .members
            .iter()
            .position(|member| member.candidate.key() == key)
            .expect("the located job holds the key");
        job.members.remove(position);
        // A settling job keeps its answer: the write ends itself either way.
        let lost_its_run = matches!(
            &job.state,
            JobState::Running { representative, .. } if representative == key
        );
        if lost_its_run {
            job.state = JobState::Waiting;
            cancel(context, key).await;
        }
        if job.members.is_empty() {
            self.jobs.remove(index);
        } else if lost_its_run {
            // The rest have waited for this answer, so they go to the head.
            let job = self
                .jobs
                .remove(index)
                .expect("the located job still exists");
            job.mark_waiting(context).await;
            self.jobs.push_front(job);
        }
    }

    /// Take `key` off the queue and off its waiting mark.
    async fn withdraw(&mut self, context: &Context, key: &str) {
        self.take_out(context, key).await;
        withdraw(context, key).await;
    }

    /// A person cancelled `key`'s identification: its whole job leaves the
    /// queue, its run ends and an answer being written is given up, and
    /// nothing else happens. A run the queue did not start, such as a library
    /// release's re-identify sheet, is ended too.
    ///
    /// Every ending is tried; the first that could not be written is the
    /// cancel's failure, for the person who asked.
    async fn cancel(
        &mut self,
        context: &Context,
        key: &str,
    ) -> Result<(), crate::library::LibraryError> {
        let Some(index) = self.index_of_key(key) else {
            return context.import.cancel_identification(key).await;
        };
        let job = self
            .jobs
            .remove(index)
            .expect("the located job still exists");
        self.end_cancelled(context, job).await
    }

    async fn end_cancelled(
        &mut self,
        context: &Context,
        job: Job,
    ) -> Result<(), crate::library::LibraryError> {
        let mut ended = Ok(());
        match &job.state {
            JobState::Waiting => {}
            JobState::Running { representative, .. } => {
                first_failure(
                    &mut ended,
                    context.import.cancel_identification(representative).await,
                );
            }
            JobState::Settling { abandon, .. } => abandon.cancel(),
        }
        for key in job.keys() {
            first_failure(&mut ended, context.import.withdraw_identification(&key).await);
        }
        info!(
            "identification: cancelled {} candidate(s) of one job",
            job.members.len()
        );
        ended
    }

    /// Take every entry with this identity off the queue.
    async fn retire(&mut self, context: &Context, identity: &CandidateFiles) {
        let Some(index) = self.index_of_identity(identity) else {
            return;
        };
        let job = self
            .jobs
            .remove(index)
            .expect("the located job still exists");
        for key in job.keys() {
            withdraw(context, &key).await;
        }
    }

    /// The waiting job nearest the front, while a slot is free.
    fn next_waiting(&self) -> Option<usize> {
        if self.running_count() >= MAX_IN_FLIGHT {
            return None;
        }
        self.jobs
            .iter()
            .position(|job| matches!(job.state, JobState::Waiting))
    }

    /// Where a request goes: behind earlier waiting requests, ahead of every
    /// waiting automatic job.
    fn request_position(&self) -> usize {
        self.jobs
            .iter()
            .position(|job| {
                matches!(job.state, JobState::Waiting) && job.admission() == Admission::Automatic
            })
            .unwrap_or(self.jobs.len())
    }

    /// Stop every run the queue has going, at shutdown.
    async fn cancel_every_run(&self, context: &Context) {
        for job in &self.jobs {
            if let JobState::Running { representative, .. } = &job.state {
                cancel(context, representative).await;
            }
        }
    }

    /// The running job whose representative is `key` on `run`; a superseded
    /// run matches none.
    fn running_job(&self, key: &str, run: IdentifyRunId) -> Option<usize> {
        self.jobs.iter().position(|job| {
            matches!(
                &job.state,
                JobState::Running { representative, run: running, .. }
                    if representative == key && *running == run
            )
        })
    }

    /// The settling job an answer belongs to, if the queue still has one.
    fn settling_job(
        &self,
        identity: &CandidateFiles,
        key: &str,
        run: IdentifyRunId,
    ) -> Option<usize> {
        self.jobs.iter().position(|job| {
            &job.identity == identity
                && matches!(
                    &job.state,
                    JobState::Settling { representative, run: settling, .. }
                        if representative == key && *settling == run
                )
        })
    }
}

/// What became of a standing write the queue made. The queue runs on its
/// own, so no caller waits on it to hear a failure: one is a bug, reported as
/// one, and the candidate reads as the table last held it.
pub(super) fn record(
    context: &Context,
    doing: &str,
    written: Result<(), crate::library::LibraryError>,
) {
    if let Err(error) = written {
        context
            .library_manager
            .report_bug(&format!("identification: {doing}"), &error);
    }
}

/// Keep the first failure of several writes tried in turn.
fn first_failure(
    kept: &mut Result<(), crate::library::LibraryError>,
    next: Result<(), crate::library::LibraryError>,
) {
    if kept.is_ok() {
        *kept = next;
    }
}

/// End `key`'s identification: its run stops, and its standing says so.
async fn cancel(context: &Context, key: &str) {
    record(
        context,
        &format!("end {key}'s identification"),
        context.import.cancel_identification(key).await,
    );
}

/// Take `key` off its waiting mark.
pub(super) async fn withdraw(context: &Context, key: &str) {
    record(
        context,
        &format!("take {key} off the identification queue"),
        context.import.withdraw_identification(key).await,
    );
}

/// Run the queue until the token is cancelled. A setting changing puts nothing
/// on the queue and takes nothing off it, so `config` is only read.
pub(super) async fn run(
    context: &Context,
    token: &CancellationToken,
    bus: &mut mpsc::UnboundedReceiver<ImportEvent>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    config: &watch::Receiver<crate::config::Config>,
) {
    let mut queue = Queue::default();
    #[cfg(any(test, feature = "test-utils"))]
    let mut waiting_for_drain: Vec<(super::Drain, tokio::sync::oneshot::Sender<()>)> = Vec::new();
    let mut settling = JoinSet::<Finished>::new();
    // Shutdown cancels the settles and waits for them rather than aborting a
    // durable write.
    let settle_token = token.child_token();

    loop {
        fill_slots(context, &mut queue).await;
        #[cfg(any(test, feature = "test-utils"))]
        {
            let (done, waiting) = std::mem::take(&mut waiting_for_drain)
                .into_iter()
                .partition(|(drain, _)| queue.is_drained(*drain));
            waiting_for_drain = waiting;
            for (_, drained) in done {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "a test that stopped waiting needs no answer"
                )]
                let _ = drained.send(());
            }
        }

        tokio::select! {
            biased;
            _ = token.cancelled() => {
                info!("identification: the queue is shutting down");
                queue.cancel_every_run(context).await;
                while let Some(result) = settling.join_next().await {
                    if let Err(error) = result {
                        warn!("identification: a settle task failed: {error}");
                    }
                }
                return;
            }
            Some(command) = commands.recv() => {
                // Every event sent before the command is on the bus by now, and
                // is heard first: a decision announced before a request must
                // not take off the queue the job the request puts on it.
                let events = pending(bus).collect();
                handle_events(context, &mut queue, &settle_token, &mut settling, events).await;
                match command {
                    Command::Request { candidate_key } => {
                        request(context, &mut queue, candidate_key).await;
                    }
                    Command::Cancel { candidate_keys, done } => {
                        let mut cancelled = Ok(());
                        for key in &candidate_keys {
                            first_failure(&mut cancelled, queue.cancel(context, key).await);
                        }
                        if done.send(cancelled).is_err() {
                            debug!("identification: the cancel's caller left before it ended");
                        }
                    }
                    Command::CancelAll { done } => {
                        let keys: Vec<String> = queue.jobs.iter().flat_map(Job::keys).collect();
                        let mut cancelled = Ok(());
                        for key in &keys {
                            first_failure(&mut cancelled, queue.cancel(context, key).await);
                        }
                        if done.send(cancelled).is_err() {
                            debug!("identification: the cancel's caller left before it ended");
                        }
                    }
                    #[cfg(any(test, feature = "test-utils"))]
                    Command::AwaitDrained { drain, drained } => {
                        waiting_for_drain.push((drain, drained));
                    }
                    #[cfg(any(test, feature = "test-utils"))]
                    Command::AwaitCommandsTaken { taken } => {
                        #[expect(
                            clippy::let_underscore_must_use,
                            reason = "a test that stopped waiting needs no answer"
                        )]
                        let _ = taken.send(());
                    }
                }
            }
            Some(result) = settling.join_next() => match result {
                Ok(done) => finish(context, &mut queue, config, done).await,
                Err(error) => warn!("identification: a settle task failed: {error}"),
            },
            event = bus.recv() => {
                let Some(event) = event else {
                    info!("identification: the import event stream closed");
                    return;
                };
                let events = std::iter::once(event).chain(pending(bus)).collect();
                handle_events(context, &mut queue, &settle_token, &mut settling, events).await;
            }
        }
    }
}

/// The events on the bus not yet taken off it.
fn pending(bus: &mut mpsc::UnboundedReceiver<ImportEvent>) -> impl Iterator<Item = ImportEvent> + '_ {
    std::iter::from_fn(|| bus.try_recv().ok())
}

/// The one way onto the queue: place the candidates under `admission`, then
/// open their panes on Find online, the page their runs report on.
pub(super) async fn admit(
    context: &Context,
    queue: &mut Queue,
    candidates: Vec<FolderCandidate>,
    admission: Admission,
) {
    let content_hashes = queue.place(context, candidates, admission).await;
    if let Err(error) = context
        .import
        .move_admitted_panes(content_hashes)
        .await
    {
        warn!("identification: could not open the admitted candidates' panes on Find online ({error})");
    }
}

/// A person asked for this candidate to be identified now.
async fn request(context: &Context, queue: &mut Queue, candidate_key: String) {
    let Some(candidate) = answerable_candidate(context, &candidate_key).await else {
        warn!("identification: cannot identify {candidate_key}: no answer can be stored for it");
        return;
    };
    admit(context, queue, vec![candidate], Admission::Requested).await;
}

/// Start what the queue has room for.
async fn fill_slots(context: &Context, queue: &mut Queue) {
    while let Some(index) = queue.next_waiting() {
        let job = &queue.jobs[index];
        let identity = job.identity.clone();
        let candidate = job.members[0].candidate.clone();
        let priority = job.priority();
        let key = candidate.key();
        let queued = queue.jobs.len();
        let start = match candidate_run_start(context, &candidate).await {
            Ok(start) => start,
            Err(error) => {
                warn!(
                    "identification: cannot read what {key} runs from ({error}); \
                     leaving its job unanswered"
                );
                queue.retire(context, &identity).await;
                continue;
            }
        };
        let CandidateRunStart {
            choices,
            title_search,
        } = start;
        info!(
            "identification: identifying {key} at {priority:?} ({} job(s) on the queue)",
            queued
        );
        let run = context.import.new_identification_run();
        if !context.import.start_identification(
            run,
            key.clone(),
            ExtractionSource::Candidate {
                candidate: candidate.clone(),
            },
            priority,
            choices,
            title_search,
        ) {
            // No source to ask: no run will report, so the job cannot hold a slot.
            warn!("identification: no source to ask about {key}; leaving its job unanswered");
            queue.retire(context, &identity).await;
            continue;
        }
        let index = queue
            .index_of_identity(&identity)
            .expect("the job that just started is the one that was selected");
        queue.jobs[index].state = JobState::Running {
            representative: key,
            run,
        };
    }
}
