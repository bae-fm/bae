//! Profiles belong to coven members; private data belongs to their
//! self-only circle.

use bae_store::{DbError, Store};
use chrono::{DateTime, Utc};
use coven::{CovenWrite, SqlReadContext};
use rusqlite::{params, OptionalExtension, Params, Row};
use std::collections::HashSet;
use std::sync::Arc;

mod collections;
mod history;
mod profile_request;
mod profiles;

/// People's part of the schema: `people.sql`.
pub const SCHEMA: bae_store::SchemaPart = bae_store::SchemaPart::new(&[include_str!("people.sql")])
    .synced(crate::synced::synced_tables, crate::synced::namespaces);

struct PeopleInner {
    /// The store every read and write of the people runs through.
    store: Store,
    /// Held while a profile is created, so two creations never overlap.
    profile_creation: tokio::sync::Mutex<()>,
    /// Held while a play is registered or local history moves into a profile.
    history_registration: tokio::sync::Mutex<()>,
}

/// The library's profiles, ratings, playlists and plays over the store. Its
/// clones share the locks that serialize profile creation and history
/// registration.
#[derive(Clone)]
pub struct People {
    inner: Arc<PeopleInner>,
}

impl std::fmt::Debug for People {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("People").finish_non_exhaustive()
    }
}

impl People {
    pub fn new(store: Store) -> Self {
        People {
            inner: Arc::new(PeopleInner {
                store,
                profile_creation: tokio::sync::Mutex::new(()),
                history_registration: tokio::sync::Mutex::new(()),
            }),
        }
    }
}

fn uuid_column(row: &Row<'_>, index: usize) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(index)?;
    uuid::Uuid::parse_str(&value)
        .map_err(|error| bae_store::read::column_conversion_error(row, index, error))
}

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
    /// The avatar the row references, if any.
    pub avatar: Option<Avatar>,
}

/// A person's avatar: the coven file the row references, and its version,
/// which the apps cache the image by as they do covers
/// ([`bae_store::file_version_sql`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Avatar {
    pub file: coven::FileId,
    pub version: String,
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
    Database(#[from] bae_store::DbError),
    #[error(transparent)]
    Sync(#[from] coven::SyncError),
}

impl From<coven::CovenError> for ProfileError {
    fn from(error: coven::CovenError) -> Self {
        Self::Database(error.into())
    }
}

impl From<bae_store::BlankName> for ProfileError {
    fn from(_: bae_store::BlankName) -> Self {
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
            Self::Database(error) => bae_diagnostics::database_category(error),
        }
    }
}
