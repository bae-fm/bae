//! Profiles belong to coven members; private data belongs to their self-only circle.

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
    /// The id of the avatar the row names, as every device sees it: a new
    /// avatar's id shows here only once its upload completes.
    pub avatar_id: Option<String>,
}

/// The request of one of this device's open invites that waits for the
/// admin: a sent invite's, approved only after comparing the confirmation
/// number. An in-person invite's request is approved without the admin
/// (§5.6).
pub fn request_to_decide(invite: &coven::Invite) -> Option<&coven::JoinRequest> {
    match invite.delivery {
        coven::InviteDeliveryKind::Sent => invite.request.as_ref(),
        coven::InviteDeliveryKind::InPerson => None,
    }
}

/// Someone in the library: their membership, and their profile once they
/// made one; a new member may still be setting theirs up. A removed
/// person's profile stays, read-only, but they are no one in the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub profile: Option<PersonProfile>,
    pub membership: crate::sync::membership::MembershipMember,
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
    Database(#[from] crate::db::DbError),
    #[error(transparent)]
    Sync(#[from] coven::SyncError),
}

impl From<coven::CovenError> for ProfileError {
    fn from(error: coven::CovenError) -> Self {
        Self::Database(error.into())
    }
}

impl ProfileError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        use crate::ui::UiErrorCategory as C;
        match self {
            Self::NotRegistered | Self::Missing | Self::EmptyName | Self::AlreadyExists => {
                C::Config
            }
            Self::InvalidPrivateCircle | Self::CirclePending(_) | Self::Sync(_) => C::Membership,
            Self::Database(_) => C::Database,
        }
    }
}
