//! Profiles belong to coven members; private data belongs to their
//! self-only circle.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sharing {
    Private,
    Household,
}

impl Sharing {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Household => "household",
        }
    }

    pub(crate) fn from_sql(value: &str) -> rusqlite::Result<Self> {
        match value {
            "private" => Ok(Self::Private),
            "household" => Ok(Self::Household),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }

    pub fn audience(self, circle: coven::CircleId) -> coven::Audience {
        match self {
            Self::Private => coven::Audience::Circle(circle),
            Self::Household => coven::Audience::Store,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonProfile {
    pub id: coven::MemberId,
    pub name: String,
    pub private_circle: coven::CircleId,
    pub history_sharing: Sharing,
    /// The coven file of the avatar the row references.
    pub avatar_id: Option<coven::FileId>,
}

/// The profile this device's person asked for, until it is written: coven
/// is making their private circle, the library refused it, or this device
/// stopped for good before it was made (§5.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestedProfile {
    pub name: String,
    pub state: RequestedProfileState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestedProfileState {
    /// The circle is being made; the profile is written once its entry
    /// lands, with sync running.
    Making,
    /// The library refused the circle. Asking again acknowledges this and
    /// asks anew.
    Refused(coven::OperationRejection),
    /// This device stopped for good before the circle was made. Asking
    /// again acknowledges this and asks anew.
    Stopped(coven::DeviceStop),
}

/// Someone in the library: their membership, and their profile once they
/// made one; a new member may still be setting theirs up. A removed
/// person's profile stays, read-only, but they are no one in the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub profile: Option<PersonProfile>,
    pub membership: crate::membership::MembershipMember,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("set up storage before creating this person's profile")]
    NotRegistered,
    #[error("this person has no profile")]
    Missing,
    #[error("a person's name cannot be empty")]
    EmptyName,
    #[error("this person already has a profile; rename it to change the name")]
    AlreadyExists,
    #[error("this person's private circle has other members or more than one matching circle")]
    InvalidPrivateCircle,
    #[error("private circle creation is waiting for operation {0:?}")]
    CirclePending(coven::OperationId),
    #[error(transparent)]
    Database(#[from] crate::DbError),
    #[error(transparent)]
    Sync(#[from] coven::SyncError),
}

impl From<coven::CovenError> for ProfileError {
    fn from(error: coven::CovenError) -> Self {
        Self::Database(error.into())
    }
}

impl From<crate::BlankName> for ProfileError {
    fn from(_: crate::BlankName) -> Self {
        Self::EmptyName
    }
}

impl ProfileError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::NotRegistered | Self::Missing | Self::EmptyName | Self::AlreadyExists => {
                C::Config
            }
            Self::InvalidPrivateCircle | Self::CirclePending(_) | Self::Sync(_) => C::Membership,
            Self::Database(_) => C::Database,
        }
    }
}

/// One of this device's open invites, with the pending record of its
/// operation when coven holds one: a refused check or a failed grant, which
/// the person ends by cancelling the invite (§5.6).
#[derive(Clone, Debug, PartialEq)]
pub struct OpenInvite {
    pub invite: coven::Invite,
    pub held: Option<crate::PendingWait>,
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
                    .map(|record| crate::PendingWait::of(record, None));
                Self { invite, held }
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;
