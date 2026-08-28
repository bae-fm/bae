use super::*;

pub(super) fn enqueue_candidate(pending: &mut VecDeque<IdentifyJob>, candidate: FolderCandidate) {
    let identity = candidate_identity(&candidate);
    if let Some(job) = pending.iter_mut().find(|job| job.identity == identity) {
        job.candidates.push(candidate);
    } else {
        pending.push_back(IdentifyJob {
            identity,
            candidates: vec![candidate],
        });
    }
}

pub(super) fn detach_candidate(
    context: &SweepContext,
    candidate_key: &str,
    in_flight: &mut HashMap<String, InFlight>,
    pending: &mut VecDeque<IdentifyJob>,
) {
    let representative = in_flight.iter().find_map(|(representative, entry)| {
        entry
            .job
            .candidates
            .iter()
            .any(|member| member.path.to_string_lossy() == candidate_key)
            .then(|| representative.clone())
    });
    if let Some(representative) = representative {
        let mut entry = in_flight
            .remove(&representative)
            .expect("located in-flight job still exists");
        entry
            .job
            .candidates
            .retain(|member| member.path.to_string_lossy() != candidate_key);
        if representative == candidate_key {
            context.release(&representative);
            if !entry.job.candidates.is_empty() {
                pending.push_front(entry.job);
            }
        } else if !entry.job.candidates.is_empty() {
            in_flight.insert(representative, entry);
        }
    }
    pending.retain_mut(|job| {
        job.candidates
            .retain(|candidate| candidate.path.to_string_lossy() != candidate_key);
        !job.candidates.is_empty()
    });
}

pub(super) fn remove_finishing_member(
    finishing_members: &mut HashMap<CandidateIdentity, Vec<FolderCandidate>>,
    candidate_key: &str,
) {
    for members in finishing_members.values_mut() {
        members.retain(|candidate| candidate.path.to_string_lossy() != candidate_key);
    }
}

pub(super) fn forget_candidate(
    candidate_key: &str,
    known_identities: &mut HashMap<String, CandidateIdentity>,
    answered_keys: &mut HashSet<String>,
    answered_identities: &mut HashSet<CandidateIdentity>,
    identified: &mut u32,
    total: &mut u32,
) -> bool {
    let Some(identity) = known_identities.remove(candidate_key) else {
        return false;
    };
    *total = total.saturating_sub(1);
    if answered_keys.remove(candidate_key) {
        *identified = identified.saturating_sub(1);
    }
    if !known_identities
        .values()
        .any(|known_identity| known_identity == &identity)
    {
        answered_identities.remove(&identity);
    }
    true
}

pub(super) fn candidate_identity(candidate: &FolderCandidate) -> CandidateIdentity {
    (candidate.files.content_hash(), candidate.file_edit_revision)
}

pub(super) fn usable_stored_answer<'a>(
    stored: &'a HashMap<String, DbImportCandidateState>,
    candidate: &FolderCandidate,
) -> Option<&'a DbImportCandidateState> {
    stored
        .get(&candidate.files.content_hash())
        .filter(|row| row.file_edits.revision == candidate.file_edit_revision)
        .filter(|row| row.metadata_provenance.is_some() || row.identify.is_some())
}

pub(super) async fn usable_current_candidate(
    context: &SweepContext,
    key: &str,
    identity: &CandidateIdentity,
) -> bool {
    sweepable_candidate(context, key)
        .await
        .is_some_and(|candidate| candidate_identity(&candidate) == *identity)
}

/// Whether this candidate, as it is on disk right now, already has metadata or
/// an identification answer for that shape.
pub(super) async fn current_stored_answer(
    context: &SweepContext,
    candidate: &FolderCandidate,
) -> Result<bool, String> {
    let Some(row) = context
        .library_manager
        .load_import_candidate_state(&candidate.files.content_hash())
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(false);
    };
    if row.file_edits.revision != candidate.file_edit_revision {
        return Ok(false);
    }
    Ok(row.metadata_provenance.is_some() || row.identify.is_some())
}

/// Split the queue against what is already stored.
pub(super) fn plan(
    candidates: Vec<FolderCandidate>,
    stored: &HashMap<String, DbImportCandidateState>,
    total: u32,
) -> Plan {
    let mut identify = VecDeque::new();
    let mut identified = 0;
    let mut grouped = Vec::<IdentifyJob>::new();
    for candidate in candidates {
        let identity = candidate_identity(&candidate);
        if let Some(job) = grouped.iter_mut().find(|job| job.identity == identity) {
            job.candidates.push(candidate);
        } else {
            grouped.push(IdentifyJob {
                identity,
                candidates: vec![candidate],
            });
        }
    }
    for job in grouped {
        let candidate = job.representative();
        // Only a candidate with neither applied metadata provenance nor an identify
        // result belongs to automatic Lookup.
        if usable_stored_answer(stored, candidate).is_none() {
            identify.push_back(job);
            continue;
        }
        identified += job.candidates.len() as u32;
    }
    Plan {
        identify,
        identified,
        total,
    }
}
