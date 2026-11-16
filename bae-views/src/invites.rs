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
}

/// This device's open invites, as rows of the shape [`INVITE_ROWS`] lists:
/// each with the pending record of its operation unless that record is the
/// invite's own waiting (the join request at its own path while no request
/// has arrived, and the admin's decision once one has), and the name its
/// request shows. A request shows the name a new person typed, or for a
/// member getting access again the name of their profile. The statements
/// that use it add their own WHERE and ORDER BY.
macro_rules! open_invite_rows {
    () => {
        "SELECT i.operation AS operation, 'open' AS kind, i.invite AS invite, \
                i.request_member IS NOT NULL AS has_request, \
                CASE WHEN i.request_member IS NULL THEN NULL \
                     ELSE COALESCE(i.joiner_name, person.name) END AS requester, \
                pending.record AS record, pending.cause AS cause, NULL AS outcome \
         FROM coven_invites AS i \
         LEFT JOIN coven_pending AS pending \
             ON pending.subject_kind = 'operation' AND pending.operation = i.operation \
            AND NOT (i.stage = 'waiting' \
                     AND ((i.request_member IS NULL AND pending.reason_kind = 'missing') \
                          OR (i.request_member IS NOT NULL AND pending.reason_kind = 'paused'))) \
         LEFT JOIN people AS person ON person.id = i.request_member "
    };
}

/// Every row of People's invites in the order coven started their operations:
/// this device's open invites, and each invite whose operation ended with a
/// sharing refusal nobody has dismissed.
const INVITE_ROWS: &str = concat!(
    open_invite_rows!(),
    "UNION ALL \
     SELECT o.operation, 'refused', NULL, NULL, NULL, NULL, NULL, o.outcome \
     FROM coven_operations AS o \
     WHERE o.kind = 'Invite' AND o.outcome_kind = 'sharing_refused' \
     ORDER BY operation"
);

/// The open invite whose id, as canonical UUID text, is `?1`, while it is
/// open.
const INVITE_BY_ID: &str = concat!(open_invite_rows!(), "WHERE i.invite_id = ?1");

/// The first open invite, in the order coven started their operations, whose
/// join request waits for the admin's decision, leaving out the invites whose
/// ids the JSON array `?1` holds.
const WAITING_REQUEST: &str = concat!(
    open_invite_rows!(),
    "WHERE i.stage = 'waiting' AND i.request_member IS NOT NULL \
       AND i.invite_id NOT IN (SELECT value FROM json_each(?1)) \
     ORDER BY i.operation LIMIT 1"
);

/// The error a row the statement read can't become an invite returns.
fn conversion(error: Box<dyn std::error::Error + Send + Sync>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, error)
}

/// Build the [`OpenInvite`] for one row of `open_invite_rows`.
fn open_invite(row: &rusqlite::Row<'_>) -> rusqlite::Result<OpenInvite> {
    let operation = coven::OperationId(row.get("operation")?);
    let invite = coven::Invite::from_columns(&row.get::<_, Vec<u8>>("invite")?, operation)
        .map_err(|error| conversion(Box::new(error)))?;
    let held = row
        .get::<_, Option<coven::PendingRecord>>("record")?
        .map(|mut record| {
            record.cause = row.get("cause")?;
            Ok::<_, rusqlite::Error>(bae_sync_status::PendingWait::of(&record, None))
        })
        .transpose()?;
    let requester = row.get::<_, Option<String>>("requester")?;
    // A recovering member's request carries no name; a member with no
    // profile is a row no writer here makes.
    if row.get::<_, bool>("has_request")? && requester.is_none() {
        return Err(conversion(
            format!("the member recovering with invite {operation:?} has no name in people").into(),
        ));
    }
    Ok(OpenInvite { invite, held, requester })
}

/// Build the [`InviteEntry`] for one row of [`INVITE_ROWS`].
fn invite_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<InviteEntry> {
    match row.get::<_, String>("kind")?.as_str() {
        "open" => Ok(InviteEntry::Open(open_invite(row)?)),
        _ => match row.get::<_, coven::OperationOutcome>("outcome")? {
            coven::OperationOutcome::SharingRefused { account, refusal } => {
                Ok(InviteEntry::Refused(RefusedInvite {
                    operation: coven::OperationId(row.get("operation")?),
                    address: account,
                    refusal,
                }))
            }
            other => Err(conversion(
                format!(
                    "a refused invite's operation ended with {other:?}, which is not a sharing \
                     refusal"
                )
                .into(),
            )),
        },
    }
}

impl LibraryViews {
    /// This device's open invites, each with the pending record of its
    /// operation when coven holds one that is a hold, and the invites a
    /// sharing refusal ended and nobody dismissed, live from coven's
    /// `coven_invites`, `coven_pending` and `coven_operations` in one
    /// statement, in the order coven started their operations.
    pub fn subscribe_invites(&self) -> coven::LiveQuery<Vec<InviteEntry>, bae_store::DbError> {
        self.store.subscribe(|sql| Ok(sql.query(INVITE_ROWS, [], invite_entry)?))
    }

    /// The open invite `invite`, live: none once it has ended, by approval,
    /// decline, cancel or expiry, or a sharing refusal.
    pub fn subscribe_invite(
        &self,
        invite: coven::InviteId,
    ) -> coven::LiveQuery<Option<OpenInvite>, bae_store::DbError> {
        let invite = invite.0.to_string();
        self.store.subscribe(move |sql| {
            Ok(sql.query(INVITE_BY_ID, [invite.as_str()], open_invite)?.pop())
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
            Ok(sql.query(WAITING_REQUEST, [closed.as_str()], open_invite)?.pop())
        })
    }
}

#[cfg(test)]
#[path = "invites_tests.rs"]
mod tests;
