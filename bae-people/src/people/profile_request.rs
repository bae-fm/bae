//! The profile this device's person asked for while coven makes their
//! private circle (`profile_request`), and how the circle's operation stands
//! in `coven_operations`. The profile is written once the circle's entry
//! lands, in the write that acknowledges that outcome (§5.4).

use super::*;

/// The profile this device's person asked for, and the operation making its
/// private circle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProfileRequest {
    pub(crate) operation: coven::OperationId,
    pub(crate) circle: coven::CircleId,
    pub(crate) name: String,
    /// `None` while coven makes the circle; then how its operation ended,
    /// until a write acknowledges it.
    pub(crate) outcome: Option<coven::OperationOutcome>,
}

/// Where a profile request stands, from its circle's operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProfileRequestState {
    /// coven is making the circle.
    Making,
    /// The circle's entry landed: the profile can be written.
    Made,
    /// The library refused the circle; the request waits for the person.
    Refused(coven::OperationRejection),
    /// This device stopped for good before the circle was made; the request
    /// waits for the person.
    Stopped(coven::DeviceStop),
}

impl ProfileRequest {
    /// Where the request stands. Making a circle claims a slot, so its
    /// operation is never satisfied, and only storage setup is cancelled or
    /// undone: an outcome of those is one no writer stores for it.
    pub(crate) fn state(&self) -> Result<ProfileRequestState, DbError> {
        match &self.outcome {
            None => Ok(ProfileRequestState::Making),
            Some(coven::OperationOutcome::Landed(_)) => Ok(ProfileRequestState::Made),
            Some(coven::OperationOutcome::Rejected(reason)) => {
                Ok(ProfileRequestState::Refused(reason.clone()))
            }
            Some(coven::OperationOutcome::Stopped(stop)) => Ok(ProfileRequestState::Stopped(*stop)),
            Some(
                outcome @ (coven::OperationOutcome::Satisfied
                | coven::OperationOutcome::Cancelled
                | coven::OperationOutcome::Undone(_)),
            ) => Err(DbError::unreadable(
                "coven_operations.outcome of the profile request's circle",
                (self.operation, outcome),
            )),
        }
    }
}

/// The request with its operation's outcome, and whether `coven_operations`
/// lists the operation at all.
const PROFILE_REQUEST_SQL: &str = "SELECT r.operation, r.circle, r.name, \
         o.operation IS NOT NULL, o.outcome \
     FROM profile_request AS r \
     LEFT JOIN coven_operations AS o ON o.operation = r.operation";

/// The profile request, if this device's person has one.
pub(super) fn profile_request_on(
    sql: &impl coven::SqlRead,
) -> rusqlite::Result<Option<ProfileRequest>> {
    sql.query(PROFILE_REQUEST_SQL, [], |row| {
        let operation = coven::OperationId(row.get(0)?);
        // The write that ends the request acknowledges the operation, and
        // nothing else does: a request whose operation coven no longer
        // lists has lost what it waits for.
        if !row.get::<_, bool>(3)? {
            return Err(bae_store::read::column_conversion_error(
                row,
                0,
                format!("the profile request's operation {operation:?} isn't listed"),
            ));
        }
        Ok(ProfileRequest {
            operation,
            circle: coven::CircleId(uuid_column(row, 1)?),
            name: row.get(2)?,
            outcome: row.get(4)?,
        })
    })
    .map(|mut found| found.pop())
}

/// Record the request in the write that journals its circle.
pub(super) fn insert_profile_request(
    sql: &CovenWrite<'_>,
    operation: coven::OperationId,
    circle: coven::CircleId,
    name: &str,
) -> rusqlite::Result<()> {
    sql.execute(
        "INSERT INTO profile_request (singleton, operation, circle, name) VALUES (1, ?1, ?2, ?3)",
        params![operation.0, circle.to_string(), name],
    )?;
    Ok(())
}

/// End the request: acknowledge its operation's outcome and delete it, in
/// the write that acts on that outcome.
pub(super) fn end_profile_request(
    sql: &CovenWrite<'_>,
    request: &ProfileRequest,
) -> Result<(), DbError> {
    sql.acknowledge_operation(request.operation)?;
    sql.execute("DELETE FROM profile_request", [])?;
    Ok(())
}

/// The request as this device's person sees it: a circle that landed is
/// still being made into their profile.
fn requested_profile(request: ProfileRequest) -> Result<crate::people::RequestedProfile, DbError> {
    use crate::people::RequestedProfileState;
    let state = match request.state()? {
        ProfileRequestState::Making | ProfileRequestState::Made => RequestedProfileState::Making,
        ProfileRequestState::Refused(reason) => RequestedProfileState::Refused(reason),
        ProfileRequestState::Stopped(stop) => RequestedProfileState::Stopped(stop),
    };
    Ok(crate::people::RequestedProfile { name: request.name, state })
}

impl People {
    /// The profile this device's person asked for, until it is written,
    /// live.
    pub fn subscribe_requested_profile(
        &self,
    ) -> coven::LiveQuery<Option<crate::people::RequestedProfile>, bae_store::DbError> {
        self.inner
            .store
            .subscribe(|sql| profile_request_on(&sql)?.map(requested_profile).transpose())
    }

    /// The profile request, live: it reruns as the request is made or ends
    /// and as its circle's operation ends.
    pub(crate) fn subscribe_profile_request(
        &self,
    ) -> coven::LiveQuery<Option<ProfileRequest>, bae_store::DbError> {
        self.inner.store.subscribe(|sql| Ok(profile_request_on(&sql)?))
    }
}
