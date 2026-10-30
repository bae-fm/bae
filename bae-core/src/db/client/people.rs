use super::membership::{member_columns, member_row, SELF_IS_ADMIN};
use super::*;
use crate::people::{Person, PersonProfile, ProfileError, Sharing};

/// The columns [`profile_row`] reads, of `people` or of its row `p`.
macro_rules! profile_columns {
    () => {
        "id, name, private_circle, history_sharing, avatar_id"
    };
    (p) => {
        "p.id, p.name, p.private_circle, p.history_sharing, p.avatar_id"
    };
}

const PROFILE_COLUMNS: &str = profile_columns!();

impl Database {
    /// The member using this device, once its registration has applied.
    pub(crate) async fn current_member(&self) -> Result<Option<coven::MemberId>, DbError> {
        Ok(self.own_person().await?.map(|person| person.membership.id))
    }

    /// The person using this device, once their registration has applied:
    /// their membership, and their profile once they made one.
    pub async fn own_person(&self) -> Result<Option<Person>, DbError> {
        Ok(self
            .read(|sql| Ok(people_on(&sql, OWN_PERSON_SQL)?))
            .await?
            .pop())
    }

    pub async fn profiles(&self) -> Result<Vec<PersonProfile>, DbError> {
        self.read(|sql| Ok(profile_rows(&sql)?)).await
    }

    /// Everyone in the library, in member-id order: each member, with
    /// their profile once they made one.
    pub async fn people(&self) -> Result<Vec<Person>, DbError> {
        self.read(|sql| Ok(people_on(&sql, PEOPLE_SQL)?)).await
    }

    pub fn subscribe_profiles(&self) -> coven::LiveQuery<Vec<PersonProfile>> {
        self.inner.handle.subscribe(|sql| Ok(profile_rows(&sql)?))
    }

    pub async fn profile(&self, member: coven::MemberId) -> Result<Option<PersonProfile>, DbError> {
        self.read(move |sql| {
            Ok(sql
                .query_row(
                    &format!("SELECT {PROFILE_COLUMNS} FROM people WHERE id = ?"),
                    [member.to_string()],
                    profile_row,
                )
                .optional()?)
        })
        .await
    }

    pub(crate) async fn own_profile(&self) -> Result<PersonProfile, ProfileError> {
        self.own_person()
            .await?
            .ok_or(ProfileError::NotRegistered)?
            .profile
            .ok_or(ProfileError::Missing)
    }

    /// A failed profile write can retry using the same named, self-only circle.
    /// No background worker guesses a profile name or finishes this operation.
    pub async fn create_profile(&self, name: &str) -> Result<PersonProfile, ProfileError> {
        let name = profile_name(name)?;
        let _creation = self.inner.profile_creation.lock().await;
        let member = self
            .current_member()
            .await?
            .ok_or(ProfileError::NotRegistered)?;
        if let Some(profile) = self.profile(member).await? {
            if profile.name != name {
                return Err(ProfileError::AlreadyExists);
            }
            let _registration = self.inner.history_registration.lock().await;
            let existing = profile.clone();
            self.call(move |sql| {
                history::adopt_local_history(sql, &existing)?;
                Ok(())
            })
            .await?;
            return Ok(profile);
        }
        let circle_name = format!("bae-person:{member}");
        let named = circle_name.clone();
        let matching = self
            .read(move |sql| {
                Ok(
                    sql.query(PRIVATE_CIRCLE_SQL, params![named, member], |row| {
                        Ok((row.get::<_, coven::CircleId>(0)?, row.get::<_, bool>(1)?))
                    })?,
                )
            })
            .await?;
        let private_circle = match matching.as_slice() {
            [] => {
                for record in self.inner.handle.pending().await? {
                    if let coven::PendingSubject::Operation {
                        id,
                        kind: coven::OperationKind::CreateCircle,
                    } = record.subject
                    {
                        return Err(ProfileError::CirclePending(id));
                    }
                }
                Box::pin(self.inner.handle.circles().create(&circle_name)).await?
            }
            [(circle, true)] => *circle,
            _ => return Err(ProfileError::InvalidPrivateCircle),
        };
        let profile = PersonProfile {
            id: member,
            name,
            private_circle,
            history_sharing: Sharing::Private,
            avatar_id: None,
        };
        // History can keep recording locally while circle creation awaits storage.
        // Serialize only the transfer into the profile with in-flight history writes.
        let _registration = self.inner.history_registration.lock().await;
        let row = profile.clone();
        self.call(move |sql| {
            sql.execute("INSERT INTO people (id, name, private_circle, history_sharing) VALUES (?1, ?2, ?3, ?4)",
                params![row.id.to_string(), row.name, row.private_circle.to_string(), row.history_sharing.as_str()])?;
            history::adopt_local_history(sql, &row)?;
            Ok(())
        }).await?;
        Ok(profile)
    }

    pub async fn rename_profile(&self, name: &str) -> Result<(), ProfileError> {
        let name = profile_name(name)?;
        let person = self.own_profile().await?.id;
        self.call(move |sql| {
            update_profile(
                sql,
                "UPDATE people SET name = ? WHERE id = ?",
                params![name, person.to_string()],
            )?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    pub async fn set_history_sharing(&self, sharing: Sharing) -> Result<(), ProfileError> {
        let person = self.own_profile().await?.id;
        self.call(move |sql| {
            update_profile(
                sql,
                "UPDATE people SET history_sharing = ? WHERE id = ?",
                params![sharing.as_str(), person.to_string()],
            )?;
            Ok(())
        })
        .await?;
        Ok(())
    }

    /// Set the person's avatar to `avatar`, a JPEG, staged as a new coven
    /// file the profile row references; `None` clears it. The file the row
    /// named before goes with its last reference.
    pub async fn set_avatar(&self, avatar: Option<Vec<u8>>) -> Result<(), ProfileError> {
        let person = self.own_profile().await?.id.to_string();
        match avatar {
            Some(bytes) => {
                self.write_with_files(
                    move |batch| {
                        Ok(batch.put_file(
                            crate::sync::AVATARS_NAMESPACE,
                            bytes,
                            coven::Place::Cloud,
                        ))
                    },
                    move |sql, file| {
                        update_profile(
                            &sql,
                            "UPDATE people SET avatar_id = ? WHERE id = ?",
                            params![file, person],
                        )?;
                        Ok(())
                    },
                )
                .await?;
            }
            None => {
                self.call_sql(move |sql| {
                    update_profile(
                        &sql,
                        "UPDATE people SET avatar_id = NULL WHERE id = ?",
                        [person],
                    )?;
                    Ok(())
                })
                .await?
            }
        }
        Ok(())
    }

    /// The person's avatar, read through coven from wherever it has the
    /// file their row references. `None` when the row references none.
    pub async fn avatar(&self, person: coven::MemberId) -> Result<Option<Vec<u8>>, ProfileError> {
        let profile = self.profile(person).await?.ok_or(ProfileError::Missing)?;
        let Some(avatar) = profile.avatar_id else {
            return Ok(None);
        };
        let file = self.coven_file(avatar).await?;
        Ok(Some(
            self.inner
                .handle
                .read_file(&file)
                .await
                .map_err(DbError::from)?,
        ))
    }
}

fn profile_name(name: &str) -> Result<String, ProfileError> {
    let name = name.trim();
    if name.is_empty() {
        Err(ProfileError::EmptyName)
    } else {
        Ok(name.to_string())
    }
}

fn profile_row(row: &Row<'_>) -> rusqlite::Result<PersonProfile> {
    Ok(PersonProfile {
        id: coven::MemberId(uuid_column(row, 0)?),
        name: row.get(1)?,
        private_circle: coven::CircleId(uuid_column(row, 2)?),
        history_sharing: Sharing::from_sql(&row.get::<_, String>(3)?)?,
        avatar_id: row.get(4)?,
    })
}

/// The circles named `?1` this device's member is in, each with whether
/// `?2` is its one member: what a person's private circle is.
const PRIVATE_CIRCLE_SQL: &str = "SELECT c.circle, \
         (SELECT count(*) = 1 AND max(m.member = ?2) FROM coven_circle_members AS m \
          WHERE m.circle = c.circle) \
     FROM coven_circles AS c WHERE c.name = ?1";

/// The library's current members in member-id order, `$where` narrowing
/// them, each with their profile's columns and then their member's.
macro_rules! people_sql {
    ($where:literal) => {
        concat!(
            "SELECT ",
            profile_columns!(p),
            ", ",
            member_columns!(),
            " FROM coven_members AS m LEFT JOIN people AS p ON p.id = m.member \
             WHERE NOT m.removed",
            $where,
            " ORDER BY m.member"
        )
    };
}

/// Everyone in the library.
const PEOPLE_SQL: &str = people_sql!("");

/// The person using this device.
const OWN_PERSON_SQL: &str = people_sql!(" AND m.is_self");

/// The people `statement` reads, from [`people_sql`].
fn people_on(sql: &SqlReadContext<'_>, statement: &str) -> rusqlite::Result<Vec<Person>> {
    let self_is_admin: bool = sql.query_row(SELF_IS_ADMIN, [], |row| row.get(0))?;
    sql.query(statement, [], |row| {
        Ok(Person {
            profile: match row.get::<_, Option<String>>(0)? {
                Some(_) => Some(profile_row(row)?),
                None => None,
            },
            membership: member_row(row, 5, self_is_admin)?,
        })
    })
}

/// Every profile, by name; `people_name` serves the order.
pub(super) const PROFILES_SQL: &str = concat!(
    "SELECT ",
    profile_columns!(),
    " FROM people ORDER BY name, id"
);

fn profile_rows(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<PersonProfile>> {
    sql.query(PROFILES_SQL, [], profile_row)
}

fn update_profile(
    sql: &SqlContext<'_, '_>,
    statement: &str,
    params: impl Params,
) -> rusqlite::Result<()> {
    let changed = sql.execute(statement, params)?;
    if changed != 1 {
        return Err(rusqlite::Error::StatementChangedRows(changed));
    }
    Ok(())
}
