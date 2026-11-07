use super::profile_request::{
    end_profile_request, insert_profile_request, profile_request_on, ProfileRequest,
    ProfileRequestState,
};
use super::*;
use crate::membership::{member_columns, member_row, SELF_IS_ADMIN};
use crate::people::{Avatar, Person, PersonProfile, ProfileError, Sharing};

/// The columns [`profile_row`] reads of the `people` row `p`: its own, and
/// its avatar's version; [`PROFILE_COLUMN_COUNT`] of them.
fn profile_columns() -> String {
    format!(
        "p.id, p.name, p.private_circle, p.history_sharing, p.avatar_id, {avatar_version}",
        avatar_version = bae_store::file_version_sql("p.avatar_id"),
    )
}

/// How many columns [`profile_columns`] names.
const PROFILE_COLUMN_COUNT: usize = 6;

/// The profile of the person `?`.
fn profile_sql() -> String {
    format!("SELECT {} FROM people AS p WHERE p.id = ?", profile_columns())
}

impl People {
    /// The member using this device, once its registration has applied.
    pub(crate) async fn current_member(&self) -> Result<Option<coven::MemberId>, DbError> {
        Ok(self.own_person().await?.map(|person| person.membership.id))
    }

    /// The person using this device, once their registration has applied:
    /// their membership, and their profile once they made one.
    pub async fn own_person(&self) -> Result<Option<Person>, DbError> {
        Ok(self.inner.store.read(|sql| Ok(people_on(&sql, &own_person_sql())?)).await?.pop())
    }

    pub async fn profiles(&self) -> Result<Vec<PersonProfile>, DbError> {
        self.inner.store.read(|sql| Ok(profile_rows(&sql)?)).await
    }

    /// Everyone in the library, in member-id order: each member, with
    /// their profile once they made one.
    pub async fn people(&self) -> Result<Vec<Person>, DbError> {
        self.inner.store.read(|sql| Ok(people_on(&sql, &everyone_sql())?)).await
    }

    pub fn subscribe_profiles(&self) -> coven::LiveQuery<Vec<PersonProfile>, bae_store::DbError> {
        self.inner.store.subscribe(|sql| Ok(profile_rows(&sql)?))
    }

    pub async fn profile(&self, member: coven::MemberId) -> Result<Option<PersonProfile>, DbError> {
        self.inner
            .store
            .read(move |sql| {
                Ok(sql.query_row(&profile_sql(), [member.to_string()], profile_row).optional()?)
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

    /// Create this device's person's profile, named `name`, in their own
    /// private circle (§5.3, §5.4). With that circle already made, one write
    /// creates the profile and moves `local_history` into the circle.
    /// Otherwise the write journals the circle with coven and records the
    /// request, offline too; [`Self::finish_profile_requests`] creates the
    /// profile once the circle's entry lands. A request the library refused,
    /// or one this device stopped before, is acknowledged and replaced by
    /// this one; one still running refuses another with `CirclePending`.
    pub async fn create_profile(&self, name: &str) -> Result<(), ProfileError> {
        let name = bae_store::EnteredName::parse(name)?.into_string();
        let _creation = self.inner.profile_creation.lock().await;
        let member = self.current_member().await?.ok_or(ProfileError::NotRegistered)?;
        if let Some(profile) = self.profile(member).await? {
            if profile.name != name {
                return Err(ProfileError::AlreadyExists);
            }
            let _registration = self.inner.history_registration.lock().await;
            self.inner
                .store
                .call(move |sql| {
                    history::adopt_local_history(sql, &profile)?;
                    Ok(())
                })
                .await?;
            return Ok(());
        }
        // History can keep recording locally while the circle is made.
        // Serialize only the transfer into the profile with in-flight
        // history writes.
        let _registration = self.inner.history_registration.lock().await;
        let made = self
            .inner
            .store
            .call(move |sql| {
                if let Some(request) = profile_request_on(sql)? {
                    match request.state()? {
                        ProfileRequestState::Making | ProfileRequestState::Made => {
                            return Ok(ProfileWrite::Pending(request.operation));
                        }
                        ProfileRequestState::Refused(_) | ProfileRequestState::Stopped(_) => {
                            end_profile_request(sql, &request)?
                        }
                    }
                }
                let circle_name = format!("bae-person:{member}");
                let matching =
                    sql.query(PRIVATE_CIRCLE_SQL, params![circle_name, member], |row| {
                        Ok((row.get::<_, coven::CircleId>(0)?, row.get::<_, bool>(1)?))
                    })?;
                match matching.as_slice() {
                    [] => {
                        let (circle, operation) = sql.create_circle(&circle_name)?;
                        insert_profile_request(sql, operation, circle, &name)?;
                    }
                    [(circle, true)] => {
                        insert_profile(sql, &new_profile(member, name, *circle))?;
                    }
                    _ => return Ok(ProfileWrite::InvalidPrivateCircle),
                }
                Ok(ProfileWrite::Written)
            })
            .await
            .map_err(profile_write_error)?;
        match made {
            ProfileWrite::Written => Ok(()),
            ProfileWrite::Pending(operation) => Err(ProfileError::CirclePending(operation)),
            ProfileWrite::InvalidPrivateCircle => Err(ProfileError::InvalidPrivateCircle),
        }
    }

    /// Create the profile this device's person asked for once coven has
    /// made its private circle: follows the request while the library is
    /// open, and when the circle's operation lands, one write creates the
    /// profile, moves `local_history` into the circle and acknowledges the
    /// outcome. A circle refused, or stopped by this device's stop for
    /// good, waits for the person, who acknowledges it by asking again
    /// (§5.4).
    pub async fn finish_profile_requests(&self) -> Result<(), ProfileRequestError> {
        let mut requests = self.subscribe_profile_request();
        while let Some(request) = requests.next().await {
            if let Some(request) = request? {
                if request.state()? == ProfileRequestState::Made {
                    self.finish_profile(request).await?;
                }
            }
        }
        Err(DbError::from(coven::CovenError::Database(coven::DbError::StoreClosed)).into())
    }

    /// The write that creates the profile `request` asked for, from its
    /// landed circle. A profile the person made on another of their devices
    /// meanwhile takes this device's plays instead; the circle made here
    /// stays theirs alone.
    async fn finish_profile(&self, request: ProfileRequest) -> Result<(), ProfileRequestError> {
        let _creation = self.inner.profile_creation.lock().await;
        let _registration = self.inner.history_registration.lock().await;
        let operation = request.operation;
        let written = self
            .inner
            .store
            .call(move |sql| {
                // Only this write ends a request whose circle has landed.
                if profile_request_on(sql)?.as_ref() != Some(&request) {
                    return Ok(false);
                }
                let member: coven::MemberId = sql.query_row(
                    "SELECT member FROM coven_members WHERE is_self AND NOT removed",
                    [],
                    |row| row.get(0),
                )?;
                end_profile_request(sql, &request)?;
                let existing =
                    sql.query_row(&profile_sql(), [member.to_string()], profile_row).optional()?;
                match existing {
                    Some(profile) => history::adopt_local_history(sql, &profile)?,
                    None => {
                        insert_profile(sql, &new_profile(member, request.name, request.circle))?
                    }
                }
                Ok(true)
            })
            .await?;
        match written {
            true => Ok(()),
            false => Err(ProfileRequestError::Changed(operation)),
        }
    }

    pub async fn rename_profile(&self, name: &str) -> Result<(), ProfileError> {
        let name = bae_store::EnteredName::parse(name)?.into_string();
        let person = self.own_profile().await?.id;
        self.inner
            .store
            .call(move |sql| {
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

    /// Rename `person`'s profile whoever's it is, as no surface asks: coven
    /// refuses the write on a profile this device's person doesn't own.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn rename_profile_of_for_test(
        &self,
        person: coven::MemberId,
        name: &str,
    ) -> Result<(), coven::CovenError> {
        let name = name.to_string();
        self.inner
            .store
            .write(move |sql| {
                update_profile(
                    &sql,
                    "UPDATE people SET name = ? WHERE id = ?",
                    params![name, person.to_string()],
                )?;
                Ok(())
            })
            .await
    }

    pub async fn set_history_sharing(&self, sharing: Sharing) -> Result<(), ProfileError> {
        let person = self.own_profile().await?.id;
        self.inner
            .store
            .call(move |sql| {
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
                self.inner
                    .store
                    .write_staged(
                        move |batch| {
                            Ok::<_, DbError>(batch.put_file(
                                crate::synced::AVATARS_NAMESPACE,
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
                self.inner
                    .store
                    .call_sql(move |sql| {
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
        let Some(avatar) = profile.avatar else {
            return Ok(None);
        };
        let file = self.inner.store.file_ref(avatar.file).await?;
        Ok(Some(self.inner.store.read_file(&file).await.map_err(DbError::from)?))
    }
}

/// What a profile's write did, for the call to answer.
enum ProfileWrite {
    Written,
    Pending(coven::OperationId),
    InvalidPrivateCircle,
}

/// A new profile, private until the person shares their history.
fn new_profile(member: coven::MemberId, name: String, circle: coven::CircleId) -> PersonProfile {
    PersonProfile {
        id: member,
        name,
        private_circle: circle,
        history_sharing: Sharing::Private,
        avatar: None,
    }
}

/// Write `profile`'s row and move the plays waiting in `local_history` into
/// its private circle, in one write (§5.3).
fn insert_profile(sql: &CovenWrite<'_>, profile: &PersonProfile) -> rusqlite::Result<()> {
    sql.execute(
        "INSERT INTO people (id, name, private_circle, history_sharing) VALUES (?1, ?2, ?3, ?4)",
        params![
            profile.id.to_string(),
            profile.name,
            profile.private_circle.to_string(),
            profile.history_sharing.as_str()
        ],
    )?;
    history::adopt_local_history(sql, profile)
}

/// A profile write's failure: a refusal of coven's circle step as the sync
/// error it is, anything else as the database's.
fn profile_write_error(error: DbError) -> ProfileError {
    match error {
        DbError::Coven(error) => match *error {
            coven::CovenError::Sync(error) => ProfileError::Sync(error),
            error => DbError::Coven(Box::new(error)).into(),
        },
        error => error.into(),
    }
}

fn profile_row(row: &Row<'_>) -> rusqlite::Result<PersonProfile> {
    Ok(PersonProfile {
        id: coven::MemberId(uuid_column(row, 0)?),
        name: row.get(1)?,
        private_circle: coven::CircleId(uuid_column(row, 2)?),
        history_sharing: Sharing::from_sql(&row.get::<_, String>(3)?)?,
        avatar: avatar_columns(row, 4)?,
    })
}

/// The avatar the columns from `at` read: its file, then its version. The
/// avatar's reference is bound, so a row naming a file finds it in
/// `coven_files` and so has a version; one without is a broken store.
fn avatar_columns(row: &Row<'_>, at: usize) -> rusqlite::Result<Option<Avatar>> {
    match (row.get::<_, Option<coven::FileId>>(at)?, row.get::<_, Option<String>>(at + 1)?) {
        (Some(file), Some(version)) => Ok(Some(Avatar { file, version })),
        (None, None) => Ok(None),
        (file, _) => Err(rusqlite::Error::FromSqlConversionFailure(
            at,
            rusqlite::types::Type::Null,
            format!("the avatar file {file:?} has no record in coven_files").into(),
        )),
    }
}

/// The circles named `?1` this device's member is in, each with whether
/// `?2` is its one member: what a person's private circle is.
const PRIVATE_CIRCLE_SQL: &str = "SELECT c.circle, \
         (SELECT count(*) = 1 AND max(m.member = ?2) FROM coven_circle_members AS m \
          WHERE m.circle = c.circle) \
     FROM coven_circles AS c WHERE c.name = ?1";

/// The library's current members in member-id order, `narrowing` narrowing
/// them, each with their profile's columns and then their member's.
fn people_sql(narrowing: &str) -> String {
    format!(
        "SELECT {profile}, {member} \
         FROM coven_members AS m LEFT JOIN people AS p ON p.id = m.member \
         WHERE NOT m.removed{narrowing} ORDER BY m.member",
        profile = profile_columns(),
        member = member_columns!(),
    )
}

/// Everyone in the library.
fn everyone_sql() -> String {
    people_sql("")
}

/// The person using this device.
fn own_person_sql() -> String {
    people_sql(" AND m.is_self")
}

/// The people `statement` reads, from [`people_sql`].
fn people_on(sql: &SqlReadContext<'_>, statement: &str) -> rusqlite::Result<Vec<Person>> {
    let self_is_admin: bool = sql.query_row(SELF_IS_ADMIN, [], |row| row.get(0))?;
    sql.query(statement, [], |row| {
        Ok(Person {
            profile: match row.get::<_, Option<String>>(0)? {
                Some(_) => Some(profile_row(row)?),
                None => None,
            },
            membership: member_row(row, PROFILE_COLUMN_COUNT, self_is_admin)?,
        })
    })
}

/// Every profile, by name; `people_name` serves the order.
pub(super) fn profiles_sql() -> String {
    format!("SELECT {} FROM people AS p ORDER BY p.name, p.id", profile_columns())
}

fn profile_rows(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<PersonProfile>> {
    sql.query(&profiles_sql(), [], profile_row)
}

fn update_profile(
    sql: &CovenWrite<'_>,
    statement: &str,
    params: impl Params,
) -> rusqlite::Result<()> {
    let changed = sql.execute(statement, params)?;
    if changed != 1 {
        return Err(rusqlite::Error::StatementChangedRows(changed));
    }
    Ok(())
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
