//! The queue, and the one loop that runs it.
//!
//! The queue lives on that loop and is reached from nowhere else, so there is
//! no lock on it and no second copy of what it holds: the entry is the whole
//! record of what a candidate is doing, and what a surface draws is the
//! candidate runtime the queue publishes into.

use super::*;
use std::collections::VecDeque;
use tokio::task::JoinSet;

mod reactions;

use reactions::{finish, handle_event};

/// One candidate on the queue: the folder it names, and which admission put it
/// there.
struct Entry {
    candidate: FolderCandidate,
    admission: Admission,
}

/// Every entry that shares one identity. One member runs; the answer it stores
/// is keyed by the identity, so it answers all of them.
struct Job {
    identity: CandidateIdentity,
    /// At least one: a job is removed with its last member.
    members: Vec<Entry>,
    state: JobState,
}

/// Where a job stands. `representative` is the member whose run is answering
/// for the whole job — kept here rather than by position, because members come
/// and go under a run that carries on without them.
enum JobState {
    Waiting,
    Running {
        representative: String,
        run: IdentifyRunId,
        /// Editable metadata revision this run began from. A later edit makes
        /// the terminal result stale even when the files did not change.
        expected_metadata_revision: u64,
    },
    Settling {
        representative: String,
        run: IdentifyRunId,
        /// Tells this settle to give its answer up before it writes: what a
        /// person cancelling the job asks of it.
        abandon: CancellationToken,
    },
}

impl Job {
    /// A job is a request when any of its members is: one person asking puts
    /// the whole identity at the front, because one run answers all of it.
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

    fn in_flight(&self) -> bool {
        !matches!(self.state, JobState::Waiting)
    }

    /// The member whose run is answering this job, while one is running. A job
    /// being written has no run left to end.
    fn running_representative(&self) -> Option<String> {
        match &self.state {
            JobState::Running { representative, .. } => Some(representative.clone()),
            JobState::Waiting | JobState::Settling { .. } => None,
        }
    }

    /// Mark every member as waiting for this job's next run.
    ///
    /// Said whenever a job goes back to waiting, because the member whose run
    /// it was had its mark taken off by that run's first broadcast: without
    /// this the candidate whose run was superseded would read as neither
    /// waiting nor running while it waits for the run that replaced it.
    fn mark_waiting(&self, context: &Context) {
        context
            .import
            .admit_identification(self.keys(), self.admission());
    }
}

/// Every candidate identification that is wanted, running, or being written.
#[derive(Default)]
pub(super) struct Queue {
    /// The jobs in the order they run: the ones a person asked for at the
    /// front, each of them in the order it was admitted.
    jobs: VecDeque<Job>,
}

impl Queue {
    fn index_of_key(&self, key: &str) -> Option<usize> {
        self.jobs.iter().position(|job| job.holds(key))
    }

    fn index_of_identity(&self, identity: &CandidateIdentity) -> Option<usize> {
        self.jobs.iter().position(|job| &job.identity == identity)
    }

    /// Whether the queue holds `key` as exactly this shape — the question a
    /// re-announced scan item asks, because re-admitting a candidate already
    /// being answered would cancel the run answering it.
    fn holds(&self, key: &str, identity: &CandidateIdentity) -> bool {
        self.index_of_key(key)
            .is_some_and(|index| &self.jobs[index].identity == identity)
    }

    /// Whether `key` is on the queue because a person asked for it. Their run
    /// is theirs until it ends: a change elsewhere in the candidate does not
    /// take it from them.
    fn requested(&self, key: &str) -> bool {
        self.index_of_key(key).is_some_and(|index| {
            self.jobs[index].members.iter().any(|member| {
                member.candidate.key() == key && member.admission == Admission::Requested
            })
        })
    }

    /// Whether the automatic admission has anything left outstanding —
    /// waiting, running, or having its answer written.
    ///
    /// Nothing in the app asks: the queue is never done, only idle. A test
    /// waits on it for what a pass over the queue used to be.
    #[cfg(any(test, feature = "test-utils"))]
    fn automatic_is_drained(&self) -> bool {
        !self
            .jobs
            .iter()
            .any(|job| job.admission() == Admission::Automatic)
    }

    /// How many jobs hold a slot. A settle holds one too: it is the same
    /// candidate's local work finishing.
    fn in_flight_count(&self) -> usize {
        self.jobs.iter().filter(|job| job.in_flight()).count()
    }

    /// Put every candidate on the queue under `admission`, and mark the keys
    /// new to it in one change, so what a surface draws and what the
    /// identification count opens with is the admission entire rather than
    /// one key at a time. Reports how many keys were new.
    ///
    /// The one way onto the queue. The admission decides the rest:
    ///
    /// - where the job stands — an automatic job at the back; a requested one
    ///   ahead of every waiting automatic job and behind earlier requests, so
    ///   a batch of requests runs in the order it was made;
    /// - what a candidate the queue already holds as this shape gets — an
    ///   automatic admission leaves it where it is, and a request restarts
    ///   it: the run answering its identity took its inputs before the person
    ///   asked, and the person's candidate becomes the one whose files the
    ///   new run reads.
    ///
    /// A candidate held as a different shape than it has now is taken out
    /// first, whoever admits it: the run answering the old shape answers a
    /// question that is gone.
    ///
    /// A requested key's queue mark stands throughout — the handle put it
    /// there before this was reached, and a mark taken off and put back would
    /// read as one identification ending and another starting.
    ///
    /// Reached through [`admit`], which also opens each admitted candidate's
    /// pane on the page its run reports on.
    fn place(
        &mut self,
        context: &Context,
        candidates: Vec<FolderCandidate>,
        admission: Admission,
    ) -> Vec<String> {
        let mut marked = Vec::new();
        let mut content_hashes = Vec::new();
        for candidate in candidates {
            let key = candidate.key();
            let identity = candidate_identity(&candidate);
            if let Some(index) = self.index_of_key(&key) {
                if admission == Admission::Automatic && self.jobs[index].identity == identity {
                    continue;
                }
                self.take_out(context, &key);
            }
            content_hashes.push(identity.0.clone());
            let entry = Entry {
                candidate,
                admission,
            };
            match (self.index_of_identity(&identity), admission) {
                (Some(index), Admission::Automatic) => self.jobs[index].members.push(entry),
                (Some(index), Admission::Requested) => {
                    let job = &mut self.jobs[index];
                    // At the head of its own job as well: the person's
                    // candidate is the one whose files the run reads.
                    job.members.insert(0, entry);
                    // Whatever was answering this identity was answering the
                    // question just re-asked, from inputs taken before it was.
                    let running = job.running_representative();
                    job.state = JobState::Waiting;
                    if let Some(representative) = running {
                        context.import.cancel_identification(&representative);
                    }
                    let job = self
                        .jobs
                        .remove(index)
                        .expect("the located job still exists");
                    job.mark_waiting(context);
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
            marked.push(key);
        }
        context.import.admit_identification(marked, admission);
        content_hashes
    }

    /// Remove `key` from whatever job holds it, ending the run that was
    /// answering for it. Reports the admission it was on.
    ///
    /// The key's queue mark is untouched: the callers that want it gone say so.
    fn take_out(&mut self, context: &Context, key: &str) -> Option<Admission> {
        let index = self.index_of_key(key)?;
        let job = &mut self.jobs[index];
        let position = job
            .members
            .iter()
            .position(|member| member.candidate.key() == key)
            .expect("the located job holds the key");
        let admission = job.members.remove(position).admission;
        // A job that loses the member whose run answers it has nothing
        // answering it any more. One already settling keeps its answer: the
        // write is on its way and ends itself whichever way it goes.
        let lost_its_run = matches!(
            &job.state,
            JobState::Running { representative, .. } if representative == key
        );
        if lost_its_run {
            context.import.cancel_identification(key);
            job.state = JobState::Waiting;
        }
        if job.members.is_empty() {
            self.jobs.remove(index);
        } else if lost_its_run {
            // The rest of the group have been waiting on an answer that is not
            // coming, so they wait at the head rather than behind the queue.
            let job = self
                .jobs
                .remove(index)
                .expect("the located job still exists");
            job.mark_waiting(context);
            self.jobs.push_front(job);
        }
        Some(admission)
    }

    /// Take `key` off the queue and off the runtime's waiting mark.
    fn withdraw(&mut self, context: &Context, key: &str) {
        self.take_out(context, key);
        context.import.withdraw_identification(key);
    }

    /// A person cancelled `key`'s identification: its whole job leaves the
    /// queue — every member shares the one run, and it answers them all. The
    /// caller has already stored the decline that keeps the automatic
    /// admission from putting it back. A run in flight is ended, and an answer being written is told to
    /// give itself up before it writes, so the candidates are left as
    /// unidentified as they were: no verdict, no failure.
    ///
    /// A key the queue does not hold may still have a run of its own — a
    /// library release re-identified in its sheet, which the queue never
    /// starts — and that run is ended. One cancel, however the run began.
    fn cancel(&mut self, context: &Context, key: &str) {
        let Some(index) = self.index_of_key(key) else {
            context.import.cancel_identification(key);
            return;
        };
        let job = self
            .jobs
            .remove(index)
            .expect("the located job still exists");
        self.end_cancelled(context, job);
    }

    /// The identities of the jobs holding `keys`, each once — what a cancel
    /// of those keys declines.
    fn identities_of<'a>(&self, keys: impl IntoIterator<Item = &'a String>) -> Vec<CandidateIdentity> {
        let mut identities: Vec<CandidateIdentity> = Vec::new();
        for key in keys {
            if let Some(index) = self.index_of_key(key) {
                let identity = &self.jobs[index].identity;
                if !identities.contains(identity) {
                    identities.push(identity.clone());
                }
            }
        }
        identities
    }

    fn end_cancelled(&mut self, context: &Context, job: Job) {
        match &job.state {
            JobState::Waiting => {}
            JobState::Running { representative, .. } => {
                context.import.cancel_identification(representative);
            }
            JobState::Settling { abandon, .. } => abandon.cancel(),
        }
        for key in job.keys() {
            context.import.withdraw_identification(&key);
        }
        info!(
            "identification: cancelled {} candidate(s) of one job",
            job.members.len()
        );
    }

    /// Every entry with this identity leaves: the answer that just stored
    /// covers all of them.
    fn retire(&mut self, context: &Context, identity: &CandidateIdentity) {
        let Some(index) = self.index_of_identity(identity) else {
            return;
        };
        let job = self
            .jobs
            .remove(index)
            .expect("the located job still exists");
        for key in job.keys() {
            context.import.withdraw_identification(&key);
        }
    }

    /// The job to start next, while there is a slot for it: the one nearest
    /// the front that is waiting. Requests stand ahead of automatic jobs, so
    /// they take the slots first.
    fn next_waiting(&self) -> Option<usize> {
        if self.in_flight_count() >= MAX_IN_FLIGHT {
            return None;
        }
        self.jobs
            .iter()
            .position(|job| matches!(job.state, JobState::Waiting))
    }

    /// Where a request stands: behind every earlier request still waiting,
    /// ahead of every automatic job that is. A batch of requests runs in the
    /// order it was made.
    fn request_position(&self) -> usize {
        self.jobs
            .iter()
            .position(|job| {
                matches!(job.state, JobState::Waiting) && job.admission() == Admission::Automatic
            })
            .unwrap_or(self.jobs.len())
    }

    /// Stop every run the queue has going. For shutdown: what is left dies
    /// with the queue.
    fn cancel_every_run(&self, context: &Context) {
        for job in &self.jobs {
            if let JobState::Running { representative, .. } = &job.state {
                context.import.cancel_identification(representative);
            }
        }
    }

    /// Give every running job back to the queue and run it again. Nothing
    /// durable was written, so replaying them whole cannot leave a wrong answer
    /// behind.
    fn replay_running(&mut self, context: &Context) {
        for job in &mut self.jobs {
            let Some(representative) = job.running_representative() else {
                continue;
            };
            context.import.cancel_identification(&representative);
            job.state = JobState::Waiting;
            job.mark_waiting(context);
        }
    }

    /// The running job whose representative is `key` on `run`, if the queue
    /// still has one. A state from a run the queue has moved off is a run it
    /// superseded, and says nothing about the job standing now.
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
        identity: &CandidateIdentity,
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

/// Run the queue until the token is cancelled.
pub(super) async fn run(
    context: &Context,
    token: &CancellationToken,
    bus: &mut mpsc::UnboundedReceiver<Result<ImportEvent, broadcast::error::RecvError>>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    config: &mut watch::Receiver<crate::config::Config>,
) {
    let mut queue = Queue::default();
    // Nothing in the app waits for the queue to drain — it is never done, only
    // idle — so this is empty outside a test asking.
    #[cfg(any(test, feature = "test-utils"))]
    let mut waiting_for_drain: Vec<tokio::sync::oneshot::Sender<()>> = Vec::new();
    let mut settling = JoinSet::<Finished>::new();
    // Every settle task takes this token, a child of the queue's own. A queue
    // shutting down cancels it and waits; nothing is aborted, because a settle
    // task holds a durable write and ends its own pending answer whichever way
    // it goes.
    let settle_token = token.child_token();

    loop {
        fill_slots(context, &mut queue).await;
        #[cfg(any(test, feature = "test-utils"))]
        if queue.automatic_is_drained() {
            for drained in waiting_for_drain.drain(..) {
                let _ = drained.send(());
            }
        }

        tokio::select! {
            biased;
            _ = token.cancelled() => {
                info!("identification: the queue is shutting down");
                queue.cancel_every_run(context);
                while let Some(result) = settling.join_next().await {
                    if let Err(error) = result {
                        warn!("identification: a settle task failed: {error}");
                    }
                }
                return;
            }
            changed = config.changed() => {
                if changed.is_err() {
                    return;
                }
                // Off means no further automatic admissions. What the setting
                // already admitted runs to its answer: the queue is work the
                // person can see, and a preference is not a cancel.
                if automatic_is_on(config) {
                    admit_automatically(context, &mut queue).await;
                } else {
                    info!(
                        "identification: automatic identification was turned off; \
                         what is already on the queue finishes"
                    );
                }
            }
            Some(command) = commands.recv() => match command {
                Command::Request { candidate_key } => {
                    request(context, &mut queue, candidate_key).await;
                }
                Command::Cancel { candidate_keys, done } => {
                    let cancelled = cancel(context, &mut queue, &candidate_keys).await;
                    if done.send(cancelled).is_err() {
                        debug!("identification: the cancel's caller left before it ended");
                    }
                }
                Command::CancelAll { done } => {
                    let keys: Vec<String> = queue.jobs.iter().flat_map(Job::keys).collect();
                    let cancelled = cancel(context, &mut queue, &keys).await;
                    if done.send(cancelled).is_err() {
                        debug!("identification: the cancel's caller left before it ended");
                    }
                }
                #[cfg(any(test, feature = "test-utils"))]
                Command::AdmitAutomatic { drained } => {
                    if automatic_is_on(config) {
                        admit_automatically(context, &mut queue).await;
                    }
                    waiting_for_drain.push(drained);
                }
            },
            Some(result) = settling.join_next() => match result {
                Ok(done) => finish(context, &mut queue, config, done).await,
                Err(error) => warn!("identification: a settle task failed: {error}"),
            },
            event = bus.recv() => {
                if !handle_event(context, &mut queue, config, &settle_token, &mut settling, event).await {
                    return;
                }
            }
        }
    }
}

fn automatic_is_on(config: &watch::Receiver<crate::config::Config>) -> bool {
    config.borrow().prefs.identification.automatic
}

/// The one way onto the queue: place every candidate under `admission`, and
/// open each admitted candidate's pane on Find online — the page its run
/// reports on, whether a person opens the candidate while it is identified or
/// after. Reports how many were new to the queue.
///
/// The pane write follows the mark rather than preceding it so a row shows
/// waiting the instant it is admitted; the pane a person has open reads the
/// session live and follows within the same admission.
pub(super) async fn admit(
    context: &Context,
    queue: &mut Queue,
    candidates: Vec<FolderCandidate>,
    admission: Admission,
) -> usize {
    let content_hashes = queue.place(context, candidates, admission);
    let opened = content_hashes.len();
    if let Err(error) = context
        .import
        .open_find_online_for_admitted(content_hashes)
        .await
    {
        warn!("identification: could not open the admitted candidates' panes on Find online ({error})");
    }
    opened
}

/// A person cancelled these candidates' identification. The decline is
/// stored first, in one write, so the automatic admission cannot take them
/// back up — across launches too; only then do their jobs leave the queue. A
/// decline that does not store cancels nothing and is the caller's error.
async fn cancel(
    context: &Context,
    queue: &mut Queue,
    keys: &[String],
) -> Result<(), crate::library::LibraryError> {
    context
        .library_manager
        .decline_identification(queue.identities_of(keys))
        .await?;
    for key in keys {
        queue.cancel(context, key);
    }
    Ok(())
}

/// A person asked for this candidate to be identified now.
async fn request(context: &Context, queue: &mut Queue, candidate_key: String) {
    let Some(candidate) = answerable_candidate(context, &candidate_key).await else {
        warn!("identification: cannot identify {candidate_key}: no answer can be stored for it");
        // The handle marked it waiting before asking, so nothing is left saying
        // a run is coming.
        context.import.withdraw_identification(&candidate_key);
        return;
    };
    // Asking again is what lifts a cancel. A lift that does not store leaves
    // the candidate declined, so no run starts, and the row says why.
    if let Err(error) = context
        .library_manager
        .clear_declined_identification(&candidate.files.content_hash())
        .await
    {
        warn!("identification: cannot lift the cancel of {candidate_key} ({error})");
        context.import.withdraw_identification(&candidate_key);
        let run = context.import.new_identification_run();
        context.import.fail_identification(
            &candidate_key,
            run,
            format!("could not ask for identification again: {error}"),
        );
        return;
    }
    admit(context, queue, vec![candidate], Admission::Requested).await;
}

/// Start what the queue has room for. Reports nothing: what it did is the
/// queue's state and the runtime's marks.
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
                // A run started without it would ask about signals the person
                // took out and answer a metadata revision nobody checked.
                warn!(
                    "identification: cannot read what {key} runs from ({error}); \
                     leaving its job unanswered"
                );
                queue.retire(context, &identity);
                continue;
            }
        };
        let CandidateRunStart {
            metadata_revision: expected_metadata_revision,
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
            // No source to ask, so no run and no state to wait for. Held as in
            // flight the job would keep its slot and the queue behind it would
            // never move.
            warn!("identification: no source to ask about {key}; leaving its job unanswered");
            queue.retire(context, &identity);
            continue;
        }
        let index = queue
            .index_of_identity(&identity)
            .expect("the job that just started is the one that was selected");
        queue.jobs[index].state = JobState::Running {
            representative: key,
            run,
            expected_metadata_revision,
        };
    }
}
