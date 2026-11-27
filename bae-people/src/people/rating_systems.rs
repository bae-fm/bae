//! How the library shows ratings: the one `rating_systems` row, which the
//! library's first write inserts with stars for albums and for tracks, and
//! which only an admin changes. coven refuses anyone else's write to it on
//! every device (an admin-only table).

use super::*;
use crate::collections::Stars;
use bae_store::{statement_row, WriteStatements};

/// How the library shows the ratings of albums, or of tracks. Each person's
/// rating stays one to five stars, or none, under every system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatingSystem {
    /// One to five stars.
    Stars,
    /// A heart on what the person rated three stars or more.
    Heart,
    /// No ratings shown, which Library settings calls *None*. The ratings
    /// stay stored.
    Hidden,
}

impl RatingSystem {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Stars => "stars",
            Self::Heart => "heart",
            Self::Hidden => "hidden",
        }
    }
}

impl std::str::FromStr for RatingSystem {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "stars" => Ok(Self::Stars),
            "heart" => Ok(Self::Heart),
            "hidden" => Ok(Self::Hidden),
            _ => Err(format!("{value:?} is not a rating system")),
        }
    }
}

/// What a rating system applies to: the library's albums or its tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RatedItems {
    Albums,
    Tracks,
}

/// How the library shows the ratings of its albums and of its tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RatingSystems {
    pub albums: RatingSystem,
    pub tracks: RatingSystem,
}

/// The rating systems as Library settings shows them to this member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RatingSystemsSettings {
    pub systems: RatingSystems,
    pub can_change: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RatingSystemError {
    /// The person using this device is not an admin of the library, so coven
    /// refused the write.
    #[error("only an admin can change how the library shows ratings")]
    NotAdmin,
    #[error(transparent)]
    Database(bae_store::DbError),
}

impl From<DbError> for RatingSystemError {
    fn from(error: DbError) -> Self {
        match &error {
            DbError::Coven(coven)
                if matches!(
                    **coven,
                    coven::CovenError::Database(coven::DbError::NotAdmin { .. })
                ) =>
            {
                Self::NotAdmin
            }
            _ => Self::Database(error),
        }
    }
}

impl RatingSystemError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::NotAdmin => C::Membership,
            Self::Database(error) => bae_diagnostics::database_category(error),
        }
    }
}

/// The library's rating systems as its first write sets them: stars for
/// albums and for tracks.
pub(crate) fn write_first_rating_systems(sql: &coven::MemberWrite<'_>) -> rusqlite::Result<()> {
    sql.run(&InsertRatingSystemsParams { system: RatingSystem::Stars.as_str() })?;
    Ok(())
}

sqlive_compose::write!(
    InsertRatingSystems(system: &'static str),
    r#"INSERT INTO rating_systems (id, albums, tracks) VALUES ('library', :system, :system)"#
);

sqlive_compose::write!(
    SetAlbumRatingSystem(system: &'static str),
    r#"UPDATE rating_systems SET albums = :system WHERE id = 'library'"#
);

sqlive_compose::write!(
    SetTrackRatingSystem(system: &'static str),
    r#"UPDATE rating_systems SET tracks = :system WHERE id = 'library'"#
);

// The library's one `rating_systems` row.
sqlive_compose::fragment!(
    LibraryRatingSystems,
    r#"
    SELECT systems.albums, systems.tracks
    FROM rating_systems AS systems
    WHERE systems.id = 'library'
"#
);

// The library's rating systems, and whether the member using this device may
// change them.
sqlive_compose::read!(
    writer RatingSystemsToChange,
    r#"
    SELECT library.*, viewer.*
    FROM @crate::people::rating_systems::LibraryRatingSystems AS library
    JOIN @crate::membership::fragments::SelfIsAdmin AS viewer ON 1
"#
);

const MIN_HEART_STARS: Stars = Stars::Three;

/// Whether *Heart* shows a rating of `stars` as a heart: three stars or
/// more.
pub fn is_heart(stars: Stars) -> bool {
    stars as i64 >= MIN_HEART_STARS as i64
}

impl LibraryRatingSystems {
    /// How the library shows its tracks' ratings.
    pub fn track_system(&self) -> Result<RatingSystem, DbError> {
        Ok(self.systems()?.tracks)
    }

    fn systems(&self) -> Result<RatingSystems, DbError> {
        let system = |stored: &str| {
            stored.parse().map_err(|refusal| DbError::unreadable("a rating system", refusal))
        };
        Ok(RatingSystems { albums: system(&self.albums)?, tracks: system(&self.tracks)? })
    }
}

impl People {
    /// How the library shows ratings, live: the first value at once, then
    /// one after each change, with the permission to change either system.
    pub fn subscribe_rating_systems(&self) -> coven::LiveQuery<RatingSystemsSettings, DbError> {
        self.inner.store.subscribe(|sql| {
            let row = statement_row(&sql, &RatingSystemsToChangeParams {})?;
            Ok(RatingSystemsSettings {
                systems: row.library.systems()?,
                can_change: row.viewer.self_is_admin,
            })
        })
    }

    /// Show the ratings of `items` with `system`, for everyone in the
    /// library. The library's other rating system stays as it is. No rating
    /// changes: each shows in the new system from the stars it holds.
    pub async fn set_rating_system(
        &self,
        items: RatedItems,
        system: RatingSystem,
    ) -> Result<(), RatingSystemError> {
        let system = system.as_str();
        self.inner
            .store
            .call(move |sql| {
                let changed = match items {
                    RatedItems::Albums => sql.run(&SetAlbumRatingSystemParams { system })?,
                    RatedItems::Tracks => sql.run(&SetTrackRatingSystemParams { system })?,
                };
                // The library's first write inserted the row, and nothing
                // deletes it.
                match changed {
                    0 => Err(rusqlite::Error::QueryReturnedNoRows.into()),
                    _ => Ok(()),
                }
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "rating_systems_tests.rs"]
mod tests;
