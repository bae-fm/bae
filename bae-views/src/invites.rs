//! People's open invites, as People lists them.

use super::*;

/// One of this device's open invites, with the pending record of its
/// operation when coven holds one that is not the invite's own waiting: a
/// refused check, a failed grant, or a failure to read its request, which the
/// person ends by cancelling the invite (§5.6). The invite's own waiting is
/// not shown: the join request it hasn't found yet, and a request awaiting
/// the admin's decision.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenInvite {
    pub invite: coven::Invite,
    pub held: Option<bae_sync_status::PendingWait>,
    /// The name a request awaiting the admin's decision shows: the one the
    /// person typed when they joined, which their request carries, or, for a
    /// member getting access again (a recovery), the name of their profile.
    /// None while no request has arrived.
    pub requester: Option<String>,
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
                Self { invite, held, requester: None }
            })
            .collect()
    }
}

/// The name the request of `invite`, if one has arrived, shows. A recovery's
/// request carries none: the member already has a profile, whose name stands.
/// A member with no profile is a row no writer here makes.
fn requester_name(
    invite: &coven::Invite,
    profiles: &std::collections::HashMap<coven::MemberId, String>,
) -> Result<Option<String>, bae_store::DbError> {
    let request = match &invite.stage {
        coven::InviteStage::Waiting { request: Some(request), .. }
        | coven::InviteStage::Approving { request, .. } => request,
        coven::InviteStage::Waiting { request: None, .. } | coven::InviteStage::Granting => {
            return Ok(None)
        }
    };
    match &request.joiner {
        coven::Joiner::NewPerson { name } => Ok(Some(name.clone())),
        coven::Joiner::Recovery => match profiles.get(&request.member) {
            Some(name) => Ok(Some(name.clone())),
            None => Err(bae_store::DbError::unreadable(
                "people.name of a recovering member",
                request.member,
            )),
        },
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
    pub fn subscribe_invites(&self) -> coven::LiveQuery<Vec<OpenInvite>, bae_store::DbError> {
        self.store.subscribe(|sql| {
            let records = sql.query("SELECT record, cause FROM coven_pending", [], |row| {
                let mut record = row.get::<_, coven::PendingRecord>(0)?;
                record.cause = row.get(1)?;
                Ok(record)
            })?;
            let profiles: std::collections::HashMap<coven::MemberId, String> = sql
                .query("SELECT id, name FROM people", [], |row| {
                    Ok((row.get::<_, coven::MemberId>(0)?, row.get::<_, String>(1)?))
                })?
                .into_iter()
                .collect();
            let mut open = OpenInvite::held_by(coven::Invite::list(sql)?, &records);
            for invite in &mut open {
                invite.requester = requester_name(&invite.invite, &profiles)?;
            }
            Ok(open)
        })
    }
}

#[cfg(test)]
#[path = "invites_tests.rs"]
mod tests;
