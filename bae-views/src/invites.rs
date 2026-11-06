//! People's open invites, as People lists them.

use super::*;

/// One of this device's open invites, with the pending record of its
/// operation when coven holds one: a refused check or a failed grant, which
/// the person ends by cancelling the invite (§5.6).
#[derive(Clone, Debug, PartialEq)]
pub struct OpenInvite {
    pub invite: coven::Invite,
    pub held: Option<bae_sync_status::PendingWait>,
}

impl OpenInvite {
    /// Each of `invites` with the record among `records` whose subject is
    /// its operation.
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
                    .map(|record| bae_sync_status::PendingWait::of(record, None));
                Self { invite, held }
            })
            .collect()
    }
}

impl LibraryViews {
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one, live from coven's `coven_invites` and
    /// `coven_pending` in one read.
    pub fn subscribe_invites(&self) -> coven::LiveQuery<Vec<OpenInvite>> {
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
