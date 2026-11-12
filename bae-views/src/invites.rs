//! People's open invites, as People lists them, and the invites a provider's
//! refusal to share ended.

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

/// An invite the provider refused to share the library's folder for, for a
/// reason only a person can fix (§5.6): it left coven's open invites, and
/// coven keeps its operation's outcome until the person dismisses it.
#[derive(Clone, Debug, PartialEq)]
pub struct RefusedInvite {
    /// The invite's operation, which dismissing acknowledges.
    pub operation: coven::OperationId,
    /// The address the invite was to share with.
    pub address: String,
    pub refusal: coven::SharingRefusal,
}

/// A row of People's invites: an open invite, or one a refusal ended.
#[derive(Clone, Debug, PartialEq)]
pub enum InviteEntry {
    Open(OpenInvite),
    Refused(RefusedInvite),
}

impl InviteEntry {
    /// The open invite, if this row is one.
    pub fn open(&self) -> Option<&OpenInvite> {
        match self {
            Self::Open(open) => Some(open),
            Self::Refused(_) => None,
        }
    }

    /// The operation the row stands for, which orders the rows.
    fn operation(&self) -> coven::OperationId {
        match self {
            Self::Open(open) => open.invite.operation,
            Self::Refused(refused) => refused.operation,
        }
    }

    /// `open` and `refused` as rows, in the order their operations were made.
    pub fn in_operation_order(open: Vec<OpenInvite>, refused: Vec<RefusedInvite>) -> Vec<Self> {
        let mut entries: Vec<Self> = open
            .into_iter()
            .map(Self::Open)
            .chain(refused.into_iter().map(Self::Refused))
            .collect();
        entries.sort_by_key(|entry| entry.operation().0);
        entries
    }
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
    /// operation when coven holds one that is a hold, and the invites a
    /// sharing refusal ended and nobody dismissed, live from coven's
    /// `coven_invites`, `coven_pending` and `coven_operations` in one read.
    pub fn subscribe_invites(&self) -> coven::LiveQuery<Vec<InviteEntry>, bae_store::DbError> {
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
            // Only an invite's operation ends with a refused sharing.
            let refused = sql
                .query(
                    "SELECT operation, outcome FROM coven_operations \
                     WHERE kind = 'Invite' AND outcome IS NOT NULL ORDER BY operation",
                    [],
                    |row| {
                        Ok((
                            coven::OperationId(row.get(0)?),
                            row.get::<_, coven::OperationOutcome>(1)?,
                        ))
                    },
                )?
                .into_iter()
                .filter_map(|(operation, outcome)| match outcome {
                    coven::OperationOutcome::SharingRefused { account, refusal } => {
                        Some(RefusedInvite { operation, address: account, refusal })
                    }
                    _ => None,
                })
                .collect();
            let mut open = OpenInvite::held_by(coven::Invite::list(sql)?, &records);
            for invite in &mut open {
                invite.requester = requester_name(&invite.invite, &profiles)?;
            }
            Ok(InviteEntry::in_operation_order(open, refused))
        })
    }
}

#[cfg(test)]
#[path = "invites_tests.rs"]
mod tests;
