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
        Ok(self
            .read(|sql| Ok(own_member_on(&sql)?))
            .await?
            .map(|(member, _)| member))
    }

    /// The member using this device and their profile, read together: what
    /// a play is recorded for.
    pub(crate) async fn own_member(
        &self,
    ) -> Result<Option<(coven::MemberId, Option<PersonProfile>)>, DbError> {
        self.read(|sql| Ok(own_member_on(&sql)?)).await
    }

    pub async fn profiles(&self) -> Result<Vec<PersonProfile>, DbError> {
        self.read(|sql| Ok(profile_rows(&sql)?)).await
    }

    /// Everyone with a profile or a membership, by member id: a removed
    /// person keeps their profile, and a new member may still be setting
    /// theirs up.
    pub async fn people(&self) -> Result<Vec<Person>, DbError> {
        self.read(|sql| {
            let self_is_admin: bool = sql.query_row(SELF_IS_ADMIN, [], |row| row.get(0))?;
            Ok(sql.query(PEOPLE_SQL, [], |row| {
                Ok(Person {
                    id: row.get(0)?,
                    profile: match row.get::<_, Option<String>>(1)? {
                        Some(_) => Some(profile_row_at(row, 1)?),
                        None => None,
                    },
                    membership: match row.get::<_, Option<String>>(6)? {
                        Some(_) => Some(member_row(row, 6, self_is_admin)?),
                        None => None,
                    },
                })
            })?)
        })
        .await
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
        let (_, profile) = self
            .own_member()
            .await?
            .ok_or(ProfileError::NotRegistered)?;
        profile.ok_or(ProfileError::Missing)
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

    pub async fn set_avatar(&self, avatar: Option<(String, Vec<u8>)>) -> Result<(), ProfileError> {
        let person = self.own_profile().await?.id.to_string();
        match avatar {
            Some((id, bytes)) => {
                // A new avatar replaces one still uploading; the person asked
                // for that, so the abandoned upload's record is dismissed.
                let abandoned = self
                    .own_abandonments(&[(crate::sync::PEOPLE_NAMESPACE, person.clone())])
                    .await?;
                let file_id = id.clone();
                self.write_with_files(
                    move |batch| {
                        batch.put_file(crate::sync::PEOPLE_NAMESPACE, file_id, bytes);
                        Ok(())
                    },
                    move |sql| {
                        update_profile(
                            &sql,
                            "UPDATE people SET avatar_id = ? WHERE id = ?",
                            params![id, person],
                        )?;
                        Ok(())
                    },
                )
                .await?;
                self.dismiss_own_abandonments(&abandoned).await;
            }
            None => {
                self.call_sql(move |sql| {
                    update_profile(
                        &sql,
                        "UPDATE people SET avatar_id = NULL, file_size = NULL WHERE id = ?",
                        [person],
                    )?;
                    Ok(())
                })
                .await?
            }
        }
        Ok(())
    }

    /// The person's avatar as this device sees it: this device's own avatar
    /// while its upload runs, otherwise the stored file their row names.
    /// `None` when there is none here, as while another device uploads it.
    pub async fn avatar(&self, person: coven::MemberId) -> Result<Option<Vec<u8>>, ProfileError> {
        self.profile(person).await?.ok_or(ProfileError::Missing)?;
        let Some(file) = self
            .current_file(crate::sync::PEOPLE_NAMESPACE, person.to_string().as_str())
            .await?
        else {
            return Ok(None);
        };
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
    profile_row_at(row, 0)
}

/// The profile whose [`profile_columns`] start at column `first`.
fn profile_row_at(row: &Row<'_>, first: usize) -> rusqlite::Result<PersonProfile> {
    Ok(PersonProfile {
        id: coven::MemberId(uuid_column(row, first)?),
        name: row.get(first + 1)?,
        private_circle: coven::CircleId(uuid_column(row, first + 2)?),
        history_sharing: Sharing::from_sql(&row.get::<_, String>(first + 3)?)?,
        avatar_id: row.get(first + 4)?,
    })
}

/// The circles named `?1` this device's member is in, each with whether
/// `?2` is its one member: what a person's private circle is.
const PRIVATE_CIRCLE_SQL: &str = "SELECT c.circle, \
         (SELECT count(*) = 1 AND max(m.member = ?2) FROM coven_circle_members AS m \
          WHERE m.circle = c.circle) \
     FROM coven_circles AS c WHERE c.name = ?1";

/// Everyone with a profile or a current membership, by member id, with
/// their profile's columns and then their member's.
const PEOPLE_SQL: &str = concat!(
    "WITH person (id) AS \
         (SELECT id FROM people UNION SELECT member FROM coven_members WHERE NOT removed) \
     SELECT person.id, ",
    profile_columns!(p),
    ", ",
    member_columns!(),
    " FROM person LEFT JOIN people AS p ON p.id = person.id \
     LEFT JOIN coven_members AS m ON m.member = person.id AND NOT m.removed \
     ORDER BY person.id"
);

/// The member using this device, with their profile when they made one. The
/// view's columns and the profile's share no name.
const OWN_MEMBER_SQL: &str = concat!(
    "SELECT ",
    profile_columns!(),
    ", member FROM coven_members LEFT JOIN people ON people.id = coven_members.member \
     WHERE is_self AND NOT removed"
);

fn own_member_on(
    sql: &SqlReadContext<'_>,
) -> rusqlite::Result<Option<(coven::MemberId, Option<PersonProfile>)>> {
    sql.query_row(OWN_MEMBER_SQL, [], |row| {
        let profile = match row.get::<_, Option<String>>(0)? {
            Some(_) => Some(profile_row(row)?),
            None => None,
        };
        Ok((row.get(5)?, profile))
    })
    .optional()
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
