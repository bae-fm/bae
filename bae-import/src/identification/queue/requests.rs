//! What a person's request does to the queue: ahead of every waiting
//! automatic job, its calls as requested.

use super::*;

/// A person asked for this candidate to be identified now.
pub(super) async fn request(
    context: &Context,
    queue: &mut Queue,
    candidate_key: crate::CandidateKey,
) {
    let candidate = match answerable_candidate(context, &candidate_key).await {
        Ok(Some(candidate)) => candidate,
        Ok(None) => {
            warn!(
                "identification: cannot identify {candidate_key}: no answer can be stored for it"
            );
            return;
        }
        Err(error) => {
            record(context, &format!("read {candidate_key} to identify it"), Err(error));
            return;
        }
    };
    admit(context, queue, vec![candidate], Admission::Requested).await;
}

impl Queue {
    /// Where a request goes: behind earlier waiting requests, ahead of every
    /// waiting automatic job.
    pub(super) fn request_position(&self) -> usize {
        self.jobs
            .iter()
            .position(|job| {
                matches!(job.state, JobState::Waiting) && job.admission() == Admission::Automatic
            })
            .unwrap_or(self.jobs.len())
    }
}

#[cfg(test)]
#[path = "requests_tests.rs"]
mod tests;
