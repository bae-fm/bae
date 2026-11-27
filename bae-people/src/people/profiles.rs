use super::*;
use crate::membership::fragments::Member;
use crate::people::fragments::Profile;
use crate::people::{Avatar, HistorySharing, PeopleView, Person, PersonProfile, ProfileError};
use bae_store::{statement_optional, statement_rows};

// The library's current members, each with their profile and how many people
// and active devices the library has. The members with a profile come first, by
// its name in the device's language; the read reads `name_sort`, so coven
// reruns it when the device's language changes. The members with no profile yet
// follow, and member id orders the rest.
sqlive_compose::read!(
    CurrentPeople { people: u32, devices: u32 },
    r#"
    SELECT member.*, profile.*, count(*) OVER () AS people,
           sum(member.active_device_count) OVER () AS devices
    FROM @crate::membership::fragments::Member AS member
    JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
    LEFT JOIN @crate::people::fragments::Profile AS profile ON profile.id = member.member_id
    WHERE NOT member.removed
    ORDER BY profile.id IS NULL, bae_name_key(profile.name, sort.language), member.member_id
"#
);

// The current member `:member`, with their profile.
sqlive_compose::read!(
    CurrentPerson(member: coven::MemberId),
    r#"
    SELECT member.*, profile.*
    FROM @crate::membership::fragments::Member AS member
    LEFT JOIN @crate::people::fragments::Profile AS profile ON profile.id = member.member_id
    WHERE member.member_id = :member AND NOT member.removed
"#
);

// The member using this device, which coven marks as this device's own, with
// their profile: while they are in the library, and with `:former` also after
// it removed them.
sqlive_compose::read!(
    ThisDevicesPerson(former: bool),
    r#"
    SELECT member.*, profile.*
    FROM @crate::membership::fragments::Member AS member
    LEFT JOIN @crate::people::fragments::Profile AS profile ON profile.id = member.member_id
    WHERE member.is_self AND (:former OR NOT member.removed)
"#
);

// The profile of the person `:member`.
sqlive_compose::read!(
    ProfileOf(member: coven::MemberId),
    r#"
    SELECT profile.*
    FROM @crate::people::fragments::Profile AS profile
    WHERE profile.id = :member
"#
);

impl People {
    /// The person this device writes as, for plays, ratings, playlists and
    /// their profile: [`Self::own_person`], and still this device's person
    /// after the library removed it, as the removed device keeps its copy and
    /// keeps writing to it.
    pub(super) async fn writing_person(&self) -> Result<Option<Person>, DbError> {
        self.inner.store.read(|sql| this_devices_person_on(&sql, true)).await
    }

    /// The person using this device, once their registration has applied:
    /// their membership, and their profile, which the same registration wrote.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn own_person(&self) -> Result<Option<Person>, DbError> {
        self.inner.store.read(|sql| this_devices_person_on(&sql, false)).await
    }

    /// The person using this device, the other people, and how many people and
    /// devices the library has, live: one statement over coven's members and
    /// devices and the profiles, so a screen draws its sections and its counts
    /// from the same moment.
    pub fn subscribe_people(&self) -> coven::LiveQuery<PeopleView, bae_store::DbError> {
        self.inner.store.subscribe(|sql| people_view_on(&sql))
    }

    /// The person with member id `member`, live: none once they have left the
    /// library, or before their registration has applied.
    pub fn subscribe_person(
        &self,
        member: coven::MemberId,
    ) -> coven::LiveQuery<Option<Person>, bae_store::DbError> {
        self.inner.store.subscribe(move |sql| {
            statement_optional(&sql, &CurrentPersonParams { member })?
                .map(|row| person(row.member, row.profile))
                .transpose()
        })
    }

    pub async fn profile(&self, member: coven::MemberId) -> Result<Option<PersonProfile>, DbError> {
        self.inner
            .store
            .read(move |sql| {
                statement_optional(&sql, &ProfileOfParams { member })?
                    .map(|row| row.profile.into_profile())
                    .transpose()
            })
            .await
    }

    pub(crate) async fn own_profile(&self) -> Result<PersonProfile, ProfileError> {
        self.writing_person()
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

fn people_view_on(sql: &SqlReadContext<'_>) -> Result<PeopleView, DbError> {
    let rows = statement_rows(sql, &CurrentPeopleParams {})?;
    // Each row says both counts. A read that returns no row has no member to
    // count.
    let (people, devices) = rows.first().map_or((0, 0), |row| (row.people, row.devices));
    let (mut own, others): (Vec<Person>, Vec<Person>) = rows
        .into_iter()
        .map(|row| person(row.member, row.profile))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .partition(|person| person.membership.is_self);
    Ok(PeopleView { own: own.pop(), others, people: people.into(), devices: devices.into() })
}

fn this_devices_person_on(
    sql: &SqlReadContext<'_>,
    former: bool,
) -> Result<Option<Person>, DbError> {
    statement_optional(sql, &ThisDevicesPersonParams { former })?
        .map(|row| person(row.member, row.profile))
        .transpose()
}

fn person(member: Member, profile: Option<Profile>) -> Result<Person, DbError> {
    Ok(Person {
        profile: profile.map(Profile::into_profile).transpose()?,
        membership: member.into_membership()?,
    })
}

impl Profile {
    fn into_profile(self) -> Result<PersonProfile, DbError> {
        let avatar = self
            .avatar_id
            .map(|file| {
                let version = self.avatar_version.ok_or_else(|| {
                    DbError::unreadable(format!("the avatar of {}", self.id), "no stored file")
                })?;
                Ok::<_, DbError>(Avatar { file, version })
            })
            .transpose()?;
        Ok(PersonProfile {
            id: self.id,
            name: self.name,
            private_circle: self.private_circle,
            history_sharing: self.history_sharing,
            avatar,
        })
    }
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
