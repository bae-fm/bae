//! The queue, and the one loop that runs it.
//!
//! The queue lives on that loop and is reached from nowhere else, so there is
//! no lock on it and no second copy of what it holds: the entry is the whole
//! record of what a candidate is doing, and what a surface draws is the
//! candidate runtime the queue publishes into.

use super::*;
use std::collections::VecDeque;
use tokio::task::JoinSet;

/// One candidate on the queue: the folder it names, and which admission put it
/// there.
struct Entry {
    candidate: ReleaseCandidate,
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
        self.members.iter().any(|member| member.candidate.key() == key)
    }

    fn keys(&self) -> Vec<String> {
        self.members
            .iter()
            .map(|member| member.candidate.key().into_owned())
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
        let admission = self.admission();
        for key in self.keys() {
            context.import.admit_identification(&key, admission);
        }
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
            self.jobs[index]
                .members
                .iter()
                .any(|member| member.candidate.key() == key && member.admission == Admission::Requested)
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

    /// How many automatic jobs hold a slot. A settle holds one too: it is the
    /// same candidate's local work finishing.
    fn automatic_in_flight(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| job.admission() == Admission::Automatic && job.in_flight())
            .count()
    }

    /// Put `candidate` in the job for its identity, or open one for it.
    /// Reports whether the key was new to the queue.
    fn place(
        &mut self,
        context: &Context,
        candidate: ReleaseCandidate,
        admission: Admission,
    ) -> bool {
        let key = candidate.key().into_owned();
        let identity = candidate_identity(&candidate);
        if let Some(index) = self.index_of_key(&key) {
            if self.jobs[index].identity == identity {
                return false;
            }
            // A different shape than the queue holds: the run answering the old
            // one is answering a question that is gone.
            self.take_out(context, &key);
        }
        match self.index_of_identity(&identity) {
            Some(index) => self.jobs[index]
                .members
                .push(Entry { candidate, admission }),
            None => self.jobs.push_back(Job {
                identity,
                members: vec![Entry { candidate, admission }],
                state: JobState::Waiting,
            }),
        }
        true
    }

    /// Admit one candidate and mark it waiting.
    pub(super) fn admit(
        &mut self,
        context: &Context,
        candidate: ReleaseCandidate,
        admission: Admission,
    ) {
        let key = candidate.key().into_owned();
        if self.place(context, candidate, admission) {
            context.import.admit_identification(&key, admission);
        }
    }

    /// Admit a whole set, marking every key new to the queue in one change, so
    /// what a surface draws and what the identification count opens with is the
    /// admission entire rather than one key at a time.
    pub(super) fn admit_all(
        &mut self,
        context: &Context,
        candidates: Vec<ReleaseCandidate>,
        admission: Admission,
    ) -> usize {
        let mut admitted = Vec::new();
        for candidate in candidates {
            let key = candidate.key().into_owned();
            if self.place(context, candidate, admission) {
                admitted.push(key);
            }
        }
        let opened = admitted.len();
        context.import.admit_identifications(admitted, admission);
        opened
    }

    /// Admit `candidate` because a person asked for it: at the head of the
    /// queue, at interactive priority, and superseding whatever was answering
    /// its identity.
    ///
    /// Its queue mark stays throughout — the handle put it there before this
    /// was reached, and a mark taken off and put back would read as one
    /// identification ending and another starting.
    pub(super) fn request(&mut self, context: &Context, candidate: ReleaseCandidate) {
        let key = candidate.key().into_owned();
        let identity = candidate_identity(&candidate);
        self.take_out(context, &key);
        let index = match self.index_of_identity(&identity) {
            Some(index) => index,
            None => {
                self.jobs.push_front(Job {
                    identity,
                    members: Vec::new(),
                    state: JobState::Waiting,
                });
                0
            }
        };
        let job = &mut self.jobs[index];
        // At the head of its own job as well: the person's candidate is the one
        // whose files the run reads.
        job.members.insert(
            0,
            Entry {
                candidate,
                admission: Admission::Requested,
            },
        );
        // Whatever was answering this identity was answering the question just
        // re-asked, from inputs taken before it was.
        let running = job.running_representative();
        job.state = JobState::Waiting;
        if let Some(representative) = running {
            context.import.cancel_identification(&representative);
        }
        let job = self.jobs.remove(index).expect("the located job still exists");
        job.mark_waiting(context);
        self.jobs.push_front(job);
        // Last, so the request's own mark is the one that stands.
        context
            .import
            .admit_identification(&key, Admission::Requested);
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
            let job = self.jobs.remove(index).expect("the located job still exists");
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

    /// Every entry with this identity leaves: the answer that just stored
    /// covers all of them.
    fn retire(&mut self, context: &Context, identity: &CandidateIdentity) {
        let Some(index) = self.index_of_identity(identity) else {
            return;
        };
        let job = self.jobs.remove(index).expect("the located job still exists");
        for key in job.keys() {
            context.import.withdraw_identification(&key);
        }
    }

    /// The job to start next: the one nearest the front that is waiting. A
    /// request runs whatever the cap says — the person is waiting on it.
    fn next_waiting(&self, automatic_has_room: bool) -> Option<usize> {
        self.jobs.iter().position(|job| {
            matches!(job.state, JobState::Waiting)
                && (job.admission() == Admission::Requested || automatic_has_room)
        })
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

    /// Withdraw everything the automatic admission put on the queue, and stop
    /// the runs it has going. A job whose answer is already being written keeps
    /// it: the write lands and the row states its result.
    fn withdraw_automatic(&mut self, context: &Context) {
        let mut kept = VecDeque::with_capacity(self.jobs.len());
        while let Some(job) = self.jobs.pop_front() {
            if job.admission() == Admission::Requested
                || matches!(job.state, JobState::Settling { .. })
            {
                kept.push_back(job);
                continue;
            }
            if let JobState::Running { representative, .. } = &job.state {
                context.import.cancel_identification(representative);
            }
            for key in job.keys() {
                context.import.withdraw_identification(&key);
            }
        }
        self.jobs = kept;
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
                    JobState::Settling { representative, run: settling }
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
                if automatic_is_on(config) {
                    admit_automatically(context, &mut queue).await;
                } else {
                    info!("identification: automatic identification was turned off");
                    queue.withdraw_automatic(context);
                }
            }
            Some(command) = commands.recv() => match command {
                Command::Request { candidate_key } => {
                    request(context, &mut queue, candidate_key).await;
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
    config.borrow().prefs.identify_automatically
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
    queue.request(context, candidate);
}

/// Start what the queue has room for. Reports nothing: what it did is the
/// queue's state and the runtime's marks.
async fn fill_slots(context: &Context, queue: &mut Queue) {
    while let Some(index) = queue.next_waiting(queue.automatic_in_flight() < MAX_IN_FLIGHT) {
        let job = &queue.jobs[index];
        let identity = job.identity.clone();
        let candidate = job.members[0].candidate.clone();
        let priority = job.priority();
        let key = candidate.key().into_owned();
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

/// Apply one import event. Reports whether the loop carries on.
async fn handle_event(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    event: Option<Result<ImportEvent, broadcast::error::RecvError>>,
) -> bool {
    match event {
        Some(Ok(ImportEvent::IdentifyStateChanged {
            candidate_key,
            run,
            state,
            ..
        })) => {
            advance(
                context,
                queue,
                config,
                settle_token,
                settling,
                &candidate_key,
                run,
                state,
            )
            .await;
        }
        // A scan finished: whatever it found, and whatever it took away, the
        // automatic admission reads the queue afresh.
        Some(Ok(ImportEvent::Scan(ScanEvent::Finished))) => {
            if automatic_is_on(config) {
                admit_automatically(context, queue).await;
            }
        }
        // A scan announces every candidate it walks, including ones the queue
        // is not responsible for — skipped, already in the library, or claimed
        // by an import that started since. Whether this is one of ours is
        // `answerable_candidate`'s question and nobody else's: the event's own
        // flags answer a narrower one, and re-deriving the answer here is what
        // let a re-scan count an importing candidate back into a total the
        // import had just taken it out of.
        Some(Ok(ImportEvent::Scan(ScanEvent::FolderCandidate { candidate, .. }))) => {
            let key = candidate.path.to_string_lossy().into_owned();
            observe(context, queue, config, &key).await;
        }
        // The folder is a different shape now, or a person decided the
        // candidate: they picked a release, said File Tags, or cleared what it
        // had. The command that decided ended its run as part of its own write.
        // What the decision left is what the queue takes the candidate as: a
        // pick answers it, and a clear puts it back on the queue rather than
        // leaving it for the next scan.
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateBindingChanged { candidate }))) => {
            let key = candidate.path.to_string_lossy().into_owned();
            reconsider(context, queue, config, &key).await;
        }
        Some(Ok(ImportEvent::Scan(
            ScanEvent::CandidateMetadataChanged { candidate_key }
            | ScanEvent::CandidateSkipChanged { candidate_key, .. },
        ))) => {
            reconsider(context, queue, config, &candidate_key).await;
        }
        // The folder was removed, renamed or unmounted, or an import has taken
        // the candidate. Extraction is cancelled by the signal service's own
        // listener, so no further `Signals` will ever arrive and a driver left
        // running would sit in `Triangulating` forever, holding a slot that
        // never frees.
        Some(Ok(ImportEvent::Scan(ScanEvent::CandidateRemoved { candidate_key })))
        | Some(Ok(ImportEvent::ImportProgress { candidate_key, .. })) => {
            queue.withdraw(context, &candidate_key);
        }
        Some(Ok(_)) => {}
        Some(Err(broadcast::error::RecvError::Lagged(n))) => {
            // A dropped `IdentifyStateChanged` would leave its job running with
            // nothing left to wake it, so the queue would stall on a slot that
            // never frees. Give the affected candidates back to the queue and
            // run them again: nothing durable was written, so replaying them
            // whole is the only shape that cannot leave a wrong answer behind.
            warn!("identification: import bus lagged by {n} events; replaying what was running");
            context
                .library_manager
                .record_telemetry(crate::diagnostics::TelemetryEvent::Anomaly {
                    kind: crate::diagnostics::AnomalyKind::EventBusLagged,
                });
            queue.replay_running(context);
            if automatic_is_on(config) {
                admit_automatically(context, queue).await;
            }
        }
        Some(Err(broadcast::error::RecvError::Closed)) | None => {
            info!("identification: the import event stream closed");
            return false;
        }
    }
    true
}

/// One state of the run answering a job.
#[allow(clippy::too_many_arguments)]
async fn advance(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    settle_token: &CancellationToken,
    settling: &mut JoinSet<Finished>,
    key: &str,
    run: IdentifyRunId,
    state: IdentifyState,
) {
    let Some(index) = queue.running_job(key, run) else {
        return;
    };
    // The run ended with no answer: something decided the candidate, or took
    // it away. The queue takes every member as it stands now rather than
    // dropping the group — what ended the run says nothing about the others.
    if matches!(state, IdentifyState::Idle) {
        let job = queue.jobs.remove(index).expect("the located job exists");
        for member_key in job.keys() {
            context.import.withdraw_identification(&member_key);
            readmit(context, queue, config, &member_key).await;
        }
        return;
    }
    // Terminal means the machine stopped moving, including on an explicit
    // failure verdict. Either way the candidate's slot is free now.
    if !state.is_terminal() {
        return;
    }
    let job = &mut queue.jobs[index];
    let identity = job.identity.clone();
    let priority = job.priority();
    let JobState::Running {
        expected_metadata_revision,
        ..
    } = job.state
    else {
        unreachable!("the job this run was located by is the running one");
    };
    // A member leaving takes its job's run with it, so a job still running for
    // this key still holds it.
    let candidate = job
        .members
        .iter()
        .find(|member| member.candidate.key() == key)
        .map(|member| member.candidate.clone())
        .expect("a job's running representative is one of its members");
    // The run ended on its own, so there is no driver left to cancel — only
    // the extraction that fed it, whose artwork pass would otherwise keep
    // going beside the settle.
    context.import.cancel_candidate_extraction(key);
    job.state = JobState::Settling {
        representative: key.to_string(),
        run,
    };
    let context = context.clone();
    let settle_token = settle_token.clone();
    settling.spawn(async move {
        settle_answer(
            context,
            identity,
            candidate,
            run,
            expected_metadata_revision,
            state,
            priority,
            settle_token,
        )
        .await
    });
}

/// What one settled answer does to the job it answered.
async fn finish(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    done: Finished,
) {
    let Some(index) = queue.settling_job(&done.identity, &done.representative_key, done.run) else {
        debug!(
            "identification: {} settled for a job the queue has moved off",
            done.representative_key
        );
        return;
    };
    let job = queue.jobs.remove(index).expect("the located job exists");
    let keys = job.keys();
    match done.settled {
        Settled::Stored => {
            info!(
                "identification: stored the verdict for {}",
                done.representative_key
            );
            for key in &keys {
                context.import.withdraw_identification(key);
            }
        }
        // Nothing was stored and nothing failed: the candidate changed while
        // its answer was being written, or the answer was given up. Every
        // member is taken as it stands now.
        Settled::Refused | Settled::Abandoned => {
            info!(
                "identification: {} stored no answer; re-reading {} candidate(s) for it",
                done.representative_key,
                keys.len()
            );
            for key in &keys {
                context.import.withdraw_identification(key);
                readmit(context, queue, config, key).await;
            }
        }
        Settled::WriteFailed { error } | Settled::Unwritable { error } => {
            warn!(
                "identification: could not store the answer for {} ({error})",
                done.representative_key
            );
            for key in &keys {
                context.import.withdraw_identification(key);
                // The representative's own failure is already on its row — the
                // write's, or the settle's when no write ran — and saying it
                // again says the same thing. Its group learns it here.
                context
                    .import
                    .fail_identification(key, done.run, error.clone());
            }
        }
    }
}

/// Take the candidate at `key` as it stands right now: off the queue, read
/// afresh, and back on it if it still wants an answer.
///
/// What an event that changes what a candidate *is* resolves to, and the same
/// resolution for each of them: a decision made about it, a skip lifted or
/// applied, a binding changed.
async fn reconsider(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    let Some(candidate) = answerable_candidate(context, key).await else {
        // Not the queue's any more: set aside, already in the library, claimed
        // by an import, or gone.
        queue.withdraw(context, key);
        return;
    };
    // A candidate a person asked for is theirs until their run ends: what
    // changed here is not what they asked about, and the write of their answer
    // checks for itself that the candidate still stands where it read it.
    if queue.requested(key) {
        return;
    }
    queue.withdraw(context, key);
    admit_as_it_stands(context, queue, config, candidate).await;
}

/// The same, for a scan re-announcing a candidate: one already being answered
/// as exactly this shape is left alone, because taking it back would cancel the
/// run answering it.
async fn observe(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    let Some(candidate) = answerable_candidate(context, key).await else {
        queue.withdraw(context, key);
        return;
    };
    if queue.holds(key, &candidate_identity(&candidate)) {
        return;
    }
    queue.withdraw(context, key);
    admit_as_it_stands(context, queue, config, candidate).await;
}

/// Read `key` afresh and put it back on the queue if it still wants an answer.
async fn readmit(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    key: &str,
) {
    let Some(candidate) = answerable_candidate(context, key).await else {
        return;
    };
    admit_as_it_stands(context, queue, config, candidate).await;
}

/// Put `candidate` on the queue if its row states no result for the files it
/// has right now.
///
/// Always as an automatic admission: a request is one act, and once the run it
/// asked for is over the person's latest word is whatever decided the candidate
/// since.
async fn admit_as_it_stands(
    context: &Context,
    queue: &mut Queue,
    config: &watch::Receiver<crate::config::Config>,
    candidate: ReleaseCandidate,
) {
    if !automatic_is_on(config) {
        return;
    }
    if wants_an_answer(context, &candidate).await {
        queue.admit(context, candidate, Admission::Automatic);
    }
}
