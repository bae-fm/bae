//! People's open invites, as People lists them.

use super::*;

/// One of this device's open invites, with the pending record of its
/// operation when coven holds one that is not the invite's own waiting: a
/// refused check, a failed grant, or a failure to read its request, which the
/// person ends by cancelling the invite (§5.6). The invite's own waiting is
/// not shown: the join request it hasn't found yet, and a sent invite's
/// request awaiting the admin's decision.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenInvite {
    pub invite: coven::Invite,
    pub held: Option<bae_sync_status::PendingWait>,
}

impl OpenInvite {
    /// Each of `invites` with the record among `records` whose subject is its
    /// operation, unless the record is the invite's own waiting.
    pub fn held_by(invites: Vec<coven::Invite>, records: &[coven::PendingRecord]) -> Vec<Self> {
        invites
            .into_iter()
            .map(|invite| {
                let held = records
                    .iter()
                    .find(|record| {
                        matches!(record.subject,
                            coven::PendingSubject::Operation { id, .. } if id == invite.operation)
                    })
                    .filter(|record| !waits_for_its_person(&invite, record))
                    .map(|record| bae_sync_status::PendingWait::of(record, None));
                Self { invite, held }
            })
            .collect()
    }
}

/// Whether `record` is `invite` waiting as it should once its code shows:
/// for the join request at its own path, or, with the request read, for the
/// admin's decision.
fn waits_for_its_person(invite: &coven::Invite, record: &coven::PendingRecord) -> bool {
    matches!(
        (&invite.stage, &record.reason),
        (coven::InviteStage::Waiting { request: None, .. }, coven::PendingReason::Missing { .. })
            | (coven::InviteStage::Waiting { request: Some(_), .. }, coven::PendingReason::Paused)
    )
}

impl LibraryViews {
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one that is a hold, live from coven's
    /// `coven_invites` and `coven_pending` in one read.
    pub fn subscribe_invites(&self) -> coven::LiveQuery<Vec<OpenInvite>, coven::CovenError> {
        self.store.subscribe(|sql| {
            let records = sql.query("SELECT record, cause FROM coven_pending", [], |row| {
                let mut record = row.get::<_, coven::PendingRecord>(0)?;
                record.cause = row.get(1)?;
                Ok(record)
            })?;
            Ok(OpenInvite::held_by(coven::Invite::list(sql)?, &records))
        })
    }
}

#[cfg(test)]
#[path = "invites_tests.rs"]
mod tests;
