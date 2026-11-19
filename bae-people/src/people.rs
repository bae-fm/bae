//! Profiles belong to coven members; private data belongs to their
//! self-only circle.

use bae_store::{DbError, Store};
use coven::{CovenWrite, SqlReadContext};
use rusqlite::{params, OptionalExtension, Params, Row};
use std::sync::Arc;
use time::UtcDateTime;

mod collections;
pub use collections::{album_ratings_on, own_track_stars_sql};
mod history;
mod initials;
pub use initials::initials;
mod joining;
pub use joining::private_circle_name;
mod profiles;
pub(crate) use profiles::name_this_person_goes_by;

/// The SQL functions the people's reads name: the key lists of names order by.
const SQL_FUNCTIONS: &[&bae_store::SchemaFunction] = &[&bae_store::NAME_KEY];

/// People's part of the schema: `people.sql`.
pub const SCHEMA: bae_store::SchemaPart = bae_store::SchemaPart::new(&[include_str!("people.sql")])
    .functions(SQL_FUNCTIONS)
    .synced(crate::synced::synced_tables, crate::synced::namespaces)
    .joining_write(joining::write_profile);

struct PeopleInner {
    /// The store every read and write of the people runs through.
    store: Store,
    /// The installation's budget of open files, which a read of an avatar
    /// takes its place from.
    files: coven::FileBudget,
}

/// The library's profiles, ratings, playlists and plays over the store.
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
    pub fn new(store: Store, files: coven::FileBudget) -> Self {
        People { inner: Arc::new(PeopleInner { store, files }) }
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
    /// Who a stored audience lets see a playlist: the store is the household,
    /// a circle is the person alone.
    pub fn of_audience(audience: coven::Audience) -> Self {
        match audience {
            coven::Audience::Store => Self::Household,
            coven::Audience::Circle(_) => Self::Private,
        }
    }

    pub fn audience(self, circle: coven::CircleId) -> coven::Audience {
        match self {
            Self::Private => coven::Audience::Circle(circle),
            Self::Household => coven::Audience::Store,
        }
    }
}

/// Who sees a person's listening history, or that none is kept (§5.3).
/// Playlists take only [`Sharing`]: a playlist has no "off".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistorySharing {
    /// New plays are not recorded at all.
    Off,
    Private,
    Household,
}

impl HistorySharing {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Private => "private",
            Self::Household => "household",
        }
    }

    pub(crate) fn from_sql(value: &str) -> rusqlite::Result<Self> {
        match value {
            "off" => Ok(Self::Off),
            "private" => Ok(Self::Private),
            "household" => Ok(Self::Household),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }

    /// Where a play begun under this setting goes; `None` when none is kept.
    pub fn audience(self, circle: coven::CircleId) -> Option<coven::Audience> {
        match self {
            Self::Off => None,
            Self::Private => Some(Sharing::Private.audience(circle)),
            Self::Household => Some(Sharing::Household.audience(circle)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonProfile {
    pub id: coven::MemberId,
    pub name: String,
    pub private_circle: coven::CircleId,
    pub history_sharing: HistorySharing,
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

/// Someone in the library: their membership, and their profile, which a
/// device of theirs writes as it registers: another device may read the
/// membership before that write has synced. A removed person's profile
/// stays, read-only, but they are no one in the library.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub profile: Option<PersonProfile>,
    pub membership: crate::membership::MembershipMember,
}

/// What the library's people screens show: the person using this device, the
/// other people in member-id order, and how many people and devices the
/// library has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeopleView {
    pub own: Option<Person>,
    pub others: Vec<Person>,
    /// Everyone in the library, this device's person included.
    pub people: u64,
    /// The active devices of everyone in the library.
    pub devices: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("this device is not registered in the library yet")]
    NotRegistered,
    #[error("this person has no profile")]
    Missing,
    #[error("a person's name cannot be empty")]
    EmptyName,
    #[error(transparent)]
    Database(#[from] bae_store::DbError),
}

/// A bug the store carried back, which the store reported as it returned;
/// a refusal of the profile holds none.
impl bae_util::task_failure::ReportedBug for ProfileError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            Self::Database(error) => error.reported_bug(),
            Self::NotRegistered | Self::Missing | Self::EmptyName => None,
        }
    }
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
            Self::NotRegistered | Self::Missing | Self::EmptyName => C::Config,
            Self::Database(error) => bae_diagnostics::database_category(error),
        }
    }
}
