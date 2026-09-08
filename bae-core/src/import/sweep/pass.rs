use super::*;

/// Everything one pass keeps: how much of the queue it counts as answered, and
/// the jobs it runs to answer the rest.
///
/// The two halves move together. A candidate that stops being the pass's —
/// removed, skipped, imported, or claimed by someone else — leaves the jobs and
/// the count in the same breath, which is what [`Pass::drop_candidate`] is, and
/// every count that changes is announced from here.
#[derive(Default)]
pub(super) struct Pass {
    /// The identity every counted candidate had when the pass last looked.
    known_identities: HashMap<String, CandidateIdentity>,
    /// Counted candidates that hold an answer.
    answered_keys: HashSet<String>,
    /// The identities those answers cover: a candidate hashing the same is
    /// answered by them without being identified again.
    answered_identities: HashSet<CandidateIdentity>,
    identified: u32,
    total: u32,
    /// Jobs waiting for a slot.
    pending: VecDeque<IdentifyJob>,
    /// Jobs holding a slot, by representative key.
    in_flight: HashMap<String, InFlight>,
    /// Members of a job that is settling, held until it reports so its answer
    /// covers them, by identity.
    finishing_members: HashMap<CandidateIdentity, Vec<ReleaseCandidate>>,
}

impl Pass {
    /// What the pass is responsible for, decided against the stored rows before
    /// any of it starts: every candidate counted, the ones that already hold
    /// applied metadata provenance or a usable verdict counted as answered, and
    /// the rest queued as jobs grouped by identity.
    pub(super) fn new(
        candidates: Vec<ReleaseCandidate>,
        stored: &HashMap<String, DbImportCandidateState>,
    ) -> Self {
        let mut pass = Self {
            total: candidates.len() as u32,
            ..Self::default()
        };
        for candidate in candidates {
            let key = candidate.key().into_owned();
            let identity = candidate_identity(&candidate);
            pass.known_identities.insert(key.clone(), identity.clone());
            if usable_stored_answer(stored, &candidate).is_some() {
                pass.answered_identities.insert(identity);
                pass.answered_keys.insert(key);
                pass.identified += 1;
            } else {
                pass.queue(candidate);
            }
        }
        pass
    }

    /// Announce how much of the queue has been answered.
    ///
    /// Both numbers are the sweep's, not the UI's: the total is how many
    /// candidates the sweep is responsible for, which is a domain fact about the
    /// queue and not something a view can infer from the rows it happens to be
    /// holding.
    pub(super) fn announce(&self, context: &SweepContext) {
        context
            .import
            .announce_queue_identify_progress(self.identified.min(self.total), self.total);
    }

    /// Tell the import runtime exactly which keys this pass has queued.
    pub(super) fn publish_queue(&self, context: &SweepContext) {
        context.import.replace_automatic_identification_queue(
            self.pending.iter().flat_map(IdentifyJob::candidate_keys),
        );
    }

    /// Nothing left to run and nothing left to start.
    pub(super) fn is_idle(&self) -> bool {
        self.pending.is_empty() && self.in_flight.is_empty()
    }

    pub(super) fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Whether the pass already counts `key` as exactly this shape.
    pub(super) fn counts(&self, key: &str, identity: &CandidateIdentity) -> bool {
        self.known_identities.get(key) == Some(identity)
    }

    /// Whether an answer the pass knows about already covers this shape.
    pub(super) fn answered(&self, identity: &CandidateIdentity) -> bool {
        self.answered_identities.contains(identity)
    }

    /// Count a candidate the pass was not responsible for when it planned.
    pub(super) fn count(&mut self, key: String, identity: CandidateIdentity) {
        if self.known_identities.insert(key, identity).is_none() {
            self.total = self.total.saturating_add(1);
        }
    }

    /// The candidate is a different shape than the pass counted. Drop the job
    /// answering the old shape and count it afresh; the caller announces once it
    /// has decided what the new shape needs.
    pub(super) fn recount(
        &mut self,
        context: &SweepContext,
        key: &str,
        identity: CandidateIdentity,
    ) {
        self.detach(context, key);
        self.forget(key);
        self.count(key.to_string(), identity);
    }

    /// The candidate is not the pass's any more: drop its job and stop counting
    /// it.
    pub(super) fn drop_candidate(&mut self, context: &SweepContext, key: &str) {
        self.detach(context, key);
        if self.forget(key) {
            self.announce(context);
        }
    }

    /// This candidate holds an answer for `identity` now.
    pub(super) fn mark_answered(&mut self, key: String, identity: CandidateIdentity) {
        self.answered_identities.insert(identity);
        if self.answered_keys.insert(key) {
            self.identified = self.identified.saturating_add(1).min(self.total);
        }
    }

    /// A job settled with a stored verdict: drop the jobs still queued for its
    /// identity, and count every candidate that shares it as answered.
    /// Announces, and reports whether that answered anything the pass was still
    /// waiting on.
    pub(super) fn answer_identity(
        &mut self,
        context: &SweepContext,
        identity: &CandidateIdentity,
    ) -> bool {
        self.pending.retain(|job| &job.identity != identity);
        self.answered_identities.insert(identity.clone());
        let covered = self
            .known_identities
            .iter()
            .filter(|(_, known)| *known == identity)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut newly_answered = 0;
        for key in covered {
            if self.answered_keys.insert(key) {
                newly_answered += 1;
            }
        }
        if newly_answered == 0 {
            return false;
        }
        self.identified = self
            .identified
            .saturating_add(newly_answered)
            .min(self.total);
        self.announce(context);
        true
    }

    /// Stop counting `key`, and report whether it was counted at all.
    fn forget(&mut self, key: &str) -> bool {
        let Some(identity) = self.known_identities.remove(key) else {
            return false;
        };
        self.total = self.total.saturating_sub(1);
        if self.answered_keys.remove(key) {
            self.identified = self.identified.saturating_sub(1);
        }
        if !self
            .known_identities
            .values()
            .any(|known_identity| known_identity == &identity)
        {
            self.answered_identities.remove(&identity);
        }
        true
    }

    /// The next job to start, in queue order.
    pub(super) fn next_job(&mut self) -> Option<IdentifyJob> {
        self.pending.pop_front()
    }

    /// This job holds a slot now, with `key` as its representative.
    pub(super) fn track(
        &mut self,
        key: String,
        job: IdentifyJob,
        run: IdentifyRunId,
        expected_metadata_revision: u64,
    ) {
        self.in_flight.insert(
            key,
            InFlight {
                job,
                run,
                signals: None,
                expected_metadata_revision,
            },
        );
    }

    /// Cancel every job holding a slot. For the paths that abandon the pass:
    /// what is left behind dies with it.
    pub(super) fn release_in_flight(&self, context: &SweepContext) {
        for key in self.in_flight.keys() {
            context.release(key);
        }
    }

    /// The candidate's latest extraction snapshot, kept for the commit that
    /// follows its verdict.
    pub(super) fn record_signals(&mut self, key: &str, signals: crate::signals::Signals) {
        if let Some(entry) = self.in_flight.get_mut(key) {
            entry.signals = Some(signals);
        }
    }

    /// Report a representative's state to the members waiting on it, and settle
    /// its job once that state is terminal: the job leaves the slot it holds,
    /// every member arriving while it settles is held for its answer, and the
    /// task that commits that answer starts.
    ///
    /// A state from another run of the same candidate — an earlier one still
    /// broadcasting — is not this pass's.
    ///
    /// `Idle` is this run ending with no answer, which happens when someone
    /// started a newer run for the same candidate: a person changing what its
    /// identification asks about. The job leaves its slot and is not queued
    /// again — the candidate belongs to that run now, and if it does not, the
    /// next pass plans it afresh from the stored row. Nothing is cancelled
    /// here: the key names the newer run.
    pub(super) fn settle(
        &mut self,
        context: &SweepContext,
        token: &CancellationToken,
        finishing: &mut JoinSet<Finished>,
        key: &str,
        run: IdentifyRunId,
        state: IdentifyState,
    ) {
        let Some(ours) = self.in_flight.get(key).filter(|entry| entry.run == run) else {
            return;
        };
        if matches!(state, IdentifyState::Idle) {
            let entry = self
                .in_flight
                .remove(key)
                .expect("the located in-flight job still exists");
            context.disown(key);
            for member_key in entry.job.candidate_keys() {
                context.import.clear_automatic_identification(&member_key);
            }
            return;
        }
        for member_key in ours.job.candidate_keys() {
            if member_key != key {
                context.import.report_identification(&member_key, &state);
            }
        }
        // Terminal means the machine stopped moving, including on an explicit
        // failure verdict. Either way the candidate's slot is free now.
        if !state.is_terminal() {
            return;
        }
        let Some(entry) = self.in_flight.remove(key) else {
            return;
        };
        let identity = entry.job.identity.clone();
        self.finishing_members.insert(identity.clone(), Vec::new());
        let representative_key = key.to_string();
        let context = context.clone();
        let child = token.child_token();
        finishing.spawn(async move {
            let candidate_keys = entry.job.candidate_keys().collect();
            let outcome = finish_candidate(&context, &entry, state, &child).await;
            let current_candidates = if matches!(&outcome, FinishCandidateOutcome::Stored) {
                Vec::new()
            } else {
                let mut current = Vec::new();
                for candidate in &entry.job.candidates {
                    let key = candidate.key();
                    if usable_current_candidate(&context, &key, &identity).await {
                        current.push(candidate.clone());
                    }
                }
                current
            };
            Finished {
                representative_key,
                identity,
                candidate_keys,
                current_candidates,
                outcome,
            }
        });
    }

    /// The members held while this identity settled.
    pub(super) fn take_finishing_members(
        &mut self,
        identity: &CandidateIdentity,
    ) -> Vec<ReleaseCandidate> {
        self.finishing_members
            .remove(identity)
            .expect("finishing identity is registered before its task starts")
    }

    /// Queue this candidate for identification, and mark it queued for the pane.
    pub(super) fn enqueue(&mut self, context: &SweepContext, candidate: ReleaseCandidate) {
        let key = candidate.key().into_owned();
        self.queue(candidate);
        context.import.requeue_automatic_identification(&key);
    }

    /// Hold the candidate when a job for its identity is settling — that
    /// answer covers this one too — and queue it otherwise.
    pub(super) fn defer_or_enqueue(
        &mut self,
        context: &SweepContext,
        identity: &CandidateIdentity,
        candidate: ReleaseCandidate,
    ) {
        if let Some(members) = self.finishing_members.get_mut(identity) {
            members.push(candidate);
        } else {
            self.enqueue(context, candidate);
        }
    }

    /// Give every running job back to the queue and run it again. Nothing
    /// durable was written, so replaying them whole cannot leave a wrong answer
    /// behind.
    pub(super) fn replay_in_flight(&mut self, context: &SweepContext) {
        for (key, entry) in self.in_flight.drain() {
            context.release(&key);
            for candidate_key in entry.job.candidate_keys() {
                context
                    .import
                    .requeue_automatic_identification(&candidate_key);
            }
            self.pending.push_back(entry.job);
        }
    }

    /// Stop running and stop queueing this candidate, without changing what the
    /// pass counts. A job that loses its representative goes back to the front
    /// of the queue; one that loses another member keeps running for the rest.
    pub(super) fn detach(&mut self, context: &SweepContext, key: &str) {
        let representative = self.in_flight.iter().find_map(|(representative, entry)| {
            entry
                .job
                .candidates
                .iter()
                .any(|member| member.key() == key)
                .then(|| representative.clone())
        });
        if let Some(representative) = representative {
            let mut entry = self
                .in_flight
                .remove(&representative)
                .expect("located in-flight job still exists");
            entry.job.candidates.retain(|member| member.key() != key);
            if representative == key {
                context.release(&representative);
                if !entry.job.candidates.is_empty() {
                    self.pending.push_front(entry.job);
                }
            } else if !entry.job.candidates.is_empty() {
                self.in_flight.insert(representative, entry);
            }
        }
        self.pending.retain_mut(|job| {
            job.candidates.retain(|candidate| candidate.key() != key);
            !job.candidates.is_empty()
        });
        for members in self.finishing_members.values_mut() {
            members.retain(|candidate| candidate.key() != key);
        }
        context.import.clear_automatic_identification(key);
    }

    /// The jobs this pass still has to run, in queue order.
    #[cfg(test)]
    pub(super) fn queued(&self) -> &VecDeque<IdentifyJob> {
        &self.pending
    }

    #[cfg(test)]
    pub(super) fn identified(&self) -> u32 {
        self.identified
    }

    /// Add the candidate to the job for its identity, or open one for it.
    fn queue(&mut self, candidate: ReleaseCandidate) {
        let identity = candidate_identity(&candidate);
        if let Some(job) = self.pending.iter_mut().find(|job| job.identity == identity) {
            job.candidates.push(candidate);
        } else {
            self.pending.push_back(IdentifyJob {
                identity,
                candidates: vec![candidate],
            });
        }
    }
}
