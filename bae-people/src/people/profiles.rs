use super::*;
use crate::membership::{member_columns, member_row};
use crate::people::{Avatar, HistorySharing, PeopleView, Person, PersonProfile, ProfileError};
use bae_records::records::name_sort::sort_key_sql;

/// The columns [`profile_row`] reads of the `people` row `p` and the
/// `coven_files` row `avatar` of its avatar ([`PROFILES`]): its own, then
/// the avatar's file and version; [`PROFILE_COLUMN_COUNT`] of them.
fn profile_columns() -> String {
    format!(
        "p.id, p.name, p.private_circle, p.history_sharing, avatar.id, {avatar_version}",
        avatar_version = bae_store::file_version_sql("avatar.id"),
    )
}

/// Each `people` row `p` with the `coven_files` row `avatar` of the avatar
/// file it references, none for a profile with no avatar. An avatar's
/// reference always names a record in `coven_files`, which coven deletes only
/// with the last reference (coven 17.2).
const PROFILES: &str = "people AS p LEFT JOIN coven_files AS avatar ON avatar.id = p.avatar_id";

/// How many columns [`profile_columns`] names.
const PROFILE_COLUMN_COUNT: usize = 6;

/// The profile of the person `?`.
fn profile_sql() -> String {
    format!("SELECT {} FROM {PROFILES} WHERE p.id = ?", profile_columns())
}

impl People {
    /// The person using this device, once their registration has applied:
    /// their membership, and their profile, which the same registration wrote.
    pub async fn own_person(&self) -> Result<Option<Person>, DbError> {
        Ok(self.inner.store.read(|sql| Ok(people_on(&sql, &own_person_sql(), [])?)).await?.pop())
    }

    /// The person using this device, the other people, and how many people and
    /// devices the library has, live: one read of coven's members and devices
    /// and the profiles, so a screen draws its sections and its counts from
    /// the same moment.
    pub fn subscribe_people(&self) -> coven::LiveQuery<PeopleView, bae_store::DbError> {
        self.inner.store.subscribe(|sql| {
            let own = people_on(&sql, &own_person_sql(), [])?.pop();
            let others = people_on(&sql, &others_sql(), [])?;
            let (people, devices) = sql.query_row(PEOPLE_COUNTS, [], |row| {
                Ok((
                    bae_store::read::count_column(row, "people")?,
                    bae_store::read::count_column(row, "devices")?,
                ))
            })?;
            Ok(PeopleView { own, others, people, devices })
        })
    }

    /// The person with member id `member`, live: none once they have left the
    /// library, or before their registration has applied.
    pub fn subscribe_person(
        &self,
        member: coven::MemberId,
    ) -> coven::LiveQuery<Option<Person>, bae_store::DbError> {
        self.inner
            .store
            .subscribe(move |sql| Ok(people_on(&sql, &person_sql(), params![member])?.pop()))
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

    pub async fn set_history_sharing(&self, sharing: HistorySharing) -> Result<(), ProfileError> {
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
        let lease = bae_store::lease_one(&self.inner.files).await;
        Ok(Some(self.inner.store.read_file(&file, &lease).await.map_err(DbError::from)?))
    }
}

/// The name this device's person goes by: their profile's. None means a
/// person with no profile, which no registered device has: a bug of bae's,
/// for the caller to report. The one place that decides it, so an invite
/// names its inviter as the person is named everywhere else.
pub(crate) fn name_this_person_goes_by(
    sql: &impl coven::SqlRead,
) -> rusqlite::Result<Option<String>> {
    sql.query(
        "SELECT p.name FROM coven_members AS m JOIN people AS p ON p.id = m.member \
         WHERE m.is_self AND NOT m.removed",
        [],
        |row| row.get::<_, String>(0),
    )
    .map(|mut found| found.pop())
}

fn profile_row(row: &Row<'_>) -> rusqlite::Result<PersonProfile> {
    Ok(PersonProfile {
        id: coven::MemberId(uuid_column(row, 0)?),
        name: row.get(1)?,
        private_circle: coven::CircleId(uuid_column(row, 2)?),
        history_sharing: HistorySharing::from_sql(&row.get::<_, String>(3)?)?,
        avatar: avatar_columns(row, 4)?,
    })
}

/// The avatar the columns from `at` read: the file of its `coven_files` row,
/// then that file's version, both NULL for a profile with no avatar.
fn avatar_columns(row: &Row<'_>, at: usize) -> rusqlite::Result<Option<Avatar>> {
    row.get::<_, Option<coven::FileId>>(at)?
        .map(|file| Ok(Avatar { file, version: row.get(at + 1)? }))
        .transpose()
}

/// The library's current members, `narrowing` narrowing them, each with
/// their profile's columns and then their member's. The members with a
/// profile come first, by its name in the device's language; the query reads
/// `name_sort`, so coven reruns it when the device's language changes. The
/// members with no profile yet follow, and member id orders the rest.
fn people_sql(narrowing: &str) -> String {
    format!(
        "SELECT {profile}, {member} \
         FROM coven_members AS m LEFT JOIN ({PROFILES}) ON p.id = m.member \
         WHERE NOT m.removed{narrowing} \
         ORDER BY p.id IS NULL, {name_key}, m.member",
        profile = profile_columns(),
        member = member_columns!(),
        name_key = sort_key_sql("p.name"),
    )
}

/// What names the member using this device, over `coven_members AS m`: the
/// person using this device is the member coven marks as this device's own.
pub(super) const IS_OWN_MEMBER: &str = "m.is_self";

/// The person using this device.
fn own_person_sql() -> String {
    people_sql(&format!(" AND {IS_OWN_MEMBER}"))
}

/// The member bound to `?1`.
fn person_sql() -> String {
    people_sql(" AND m.member = ?1")
}

/// Everyone but the person using this device.
fn others_sql() -> String {
    people_sql(&format!(" AND NOT {IS_OWN_MEMBER}"))
}

/// How many people the library has and how many active devices they have.
const PEOPLE_COUNTS: &str = "\
    SELECT (SELECT COUNT(*) FROM coven_members AS m WHERE NOT m.removed) AS people, \
           (SELECT COUNT(*) FROM coven_devices AS d \
            JOIN coven_members AS m ON m.member = d.member \
            WHERE NOT m.removed AND d.state = 'active') AS devices";

/// The people `statement` reads with `params`, from [`people_sql`].
fn people_on(
    sql: &SqlReadContext<'_>,
    statement: &str,
    params: impl Params,
) -> rusqlite::Result<Vec<Person>> {
    sql.query(statement, params, |row| {
        Ok(Person {
            profile: match row.get::<_, Option<String>>(0)? {
                Some(_) => Some(profile_row(row)?),
                None => None,
            },
            membership: member_row(row, PROFILE_COLUMN_COUNT)?,
        })
    })
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
