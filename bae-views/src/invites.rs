//! People's open invites, as People lists them, and the invites a provider's
//! refusal to share ended.

use super::*;
use bae_store::{statement_optional, statement_rows};
use coven::types::{FromSql, ValueRef};

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
}

// This device's open invites: each with the pending record of its operation
// unless that record is the invite's own waiting (the join request at its own
// path while no request has arrived, and the admin's decision once one has),
// and the name its request shows. A request shows the name a new person typed,
// or for a member getting access again the name of their profile.
sqlive_compose::fragment!(
    OpenInviteRows {
        invite_id: String,
        stage: String,
        has_request: bool,
        requester: Option<String>,
        record: Option<Vec<u8>>,
        cause: Option<String>
    },
    r#"
    SELECT i.operation, i.invite, i.invite_id, i.stage,
           i.request_member IS NOT NULL AS has_request,
           CASE WHEN i.request_member IS NULL THEN NULL
                ELSE COALESCE(i.joiner_name, person.name) END AS requester,
           pending.record AS record, pending.cause AS cause
    FROM coven_invites AS i
    LEFT JOIN coven_pending AS pending
        ON pending.subject_kind = 'operation' AND pending.operation = i.operation
       AND NOT (i.stage = 'waiting'
                AND ((i.request_member IS NULL AND pending.reason_kind = 'missing')
                     OR (i.request_member IS NOT NULL AND pending.reason_kind = 'paused')))
    LEFT JOIN @bae_people::people::fragments::Profile AS person
        ON person.id = i.request_member
"#
);

// Every row of People's invites in the order coven started their operations:
// this device's open invites, each with its columns, and each invite whose
// operation ended with a sharing refusal nobody has dismissed, with how it
// ended: `coven_operations` lists an invite only once a sharing refusal ends
// it.
sqlive_compose::read!(
    writer ListedInvites { listed_operation: i64, outcome: Option<coven::InviteOutcome> },
    r#"
    SELECT listed.operation AS listed_operation, listed.outcome, open.*
    FROM (
        SELECT i.operation, NULL AS outcome
        FROM coven_invites AS i
        UNION ALL
        SELECT ended.operation, ended.outcome
        FROM coven_operations AS ended
        WHERE ended.kind = 'Invite'
    ) AS listed
    LEFT JOIN @crate::invites::OpenInviteRows AS open ON open.operation = listed.operation
    ORDER BY listed.operation
"#
);

// The open invite whose id, as canonical UUID text, is `:invite`, while it is
// open.
sqlive_compose::read!(
    writer InviteOfId(invite: String),
    r#"
    SELECT open.*
    FROM @crate::invites::OpenInviteRows AS open
    WHERE open.invite_id = :invite
"#
);

// The first open invite, in the order coven started their operations, whose
// join request waits for the admin's decision, leaving out the invites whose
// ids the JSON array `:closed` holds.
sqlive_compose::read!(
    writer FirstWaitingRequest(closed: String),
    r#"
    SELECT open.*
    FROM @crate::invites::OpenInviteRows AS open
    WHERE open.stage = 'waiting' AND open.has_request
      AND open.invite_id NOT IN (SELECT closed.value FROM json_each(:closed) AS closed)
    ORDER BY open.operation
    LIMIT 1
"#
);

impl OpenInviteRows {
    /// The invite these columns hold.
    fn open_invite(self) -> Result<OpenInvite, DbError> {
        let operation = coven::OperationId(self.operation);
        let unreadable = |cause: String| {
            DbError::unreadable(format!("the invite of operation {}", operation.0), cause)
        };
        let invite = coven::Invite::from_columns(&self.invite, operation)
            .map_err(|error| unreadable(error.to_string()))?;
        let held = self
            .record
            .map(|record| {
                let mut record = coven::PendingRecord::column_result(ValueRef::Blob(&record))
                    .map_err(|error| unreadable(error.to_string()))?;
                record.cause = self.cause;
                Ok::<_, DbError>(bae_sync_status::PendingWait::of(&record))
            })
            .transpose()?;
        // A recovering member's request carries no name; a member with no
        // profile is a row no writer here makes.
        if self.has_request && self.requester.is_none() {
            return Err(unreadable(
                "the member recovering with it has no name in people".to_string(),
            ));
        }
        Ok(OpenInvite { invite, held, requester: self.requester })
    }
}

/// The entry one row of [`ListedInvites`] lists.
fn invite_entry(row: ListedInvitesRow) -> Result<InviteEntry, DbError> {
    match (row.open, row.outcome) {
        (Some(open), None) => Ok(InviteEntry::Open(open.open_invite()?)),
        (None, Some(coven::InviteOutcome::SharingRefused { account, refusal })) => {
            Ok(InviteEntry::Refused(RefusedInvite {
                operation: coven::OperationId(row.listed_operation),
                address: account,
                refusal,
            }))
        }
        (open, outcome) => Err(DbError::unreadable(
            format!("the invite of operation {}", row.listed_operation),
            (open.is_some(), outcome),
        )),
    }
}

impl LibraryViews {
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one that is a hold, and the invites a
    /// sharing refusal ended and nobody dismissed, live from coven's
    /// `coven_invites`, `coven_pending` and `coven_operations` in one
    /// statement, in the order coven started their operations.
    pub fn subscribe_invites(&self) -> coven::LiveQuery<Vec<InviteEntry>, bae_store::DbError> {
        self.store.subscribe(|sql| {
            statement_rows(&sql, &ListedInvitesParams {})?.into_iter().map(invite_entry).collect()
        })
    }

    /// The open invite `invite`, live: none once it has ended, by approval,
    /// decline, cancel or expiry, or a sharing refusal.
    pub fn subscribe_invite(
        &self,
        invite: coven::InviteId,
    ) -> coven::LiveQuery<Option<OpenInvite>, bae_store::DbError> {
        let invite = invite.0.to_string();
        self.store.subscribe(move |sql| {
            statement_optional(&sql, &InviteOfIdParams { invite: invite.clone() })?
                .map(|row| row.open.open_invite())
                .transpose()
        })
    }

    /// The first of this device's open invites, in the order coven started
    /// their operations, whose join request waits for the admin's decision,
    /// leaving out the invites in `closed`: the ones whose request the person
    /// closed. None while no other request waits.
    pub fn subscribe_waiting_request(
        &self,
        closed: Vec<coven::InviteId>,
    ) -> coven::LiveQuery<Option<OpenInvite>, bae_store::DbError> {
        let closed = serde_json::Value::from(
            closed.iter().map(|invite| invite.0.to_string()).collect::<Vec<_>>(),
        )
        .to_string();
        self.store.subscribe(move |sql| {
            statement_optional(&sql, &FirstWaitingRequestParams { closed: closed.clone() })?
                .map(|row| row.open.open_invite())
                .transpose()
        })
    }
}

#[cfg(test)]
#[path = "invites_tests.rs"]
mod tests;
