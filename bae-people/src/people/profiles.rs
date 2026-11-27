use super::*;
use crate::people::fragments::Profile;
use crate::people::{Avatar, HistorySharing, PeopleView, Person, PersonProfile, ProfileError};
use bae_store::{statement_optional, statement_row, statement_rows};

// A member with their profile, none until a device of theirs has written it.
sqlive_compose::fragment!(
    MemberWithProfile,
    r#"
    SELECT member.*, profile.*
    FROM @crate::membership::fragments::Member AS member
    LEFT JOIN @crate::people::fragments::Profile AS profile ON profile.id = member.member_id
"#
);

// The current member using this device, which coven marks as this device's own.
sqlive_compose::read!(
    writer OwnPerson,
    r#"
    SELECT person.*
    FROM @crate::people::profiles::MemberWithProfile AS person
    WHERE person.is_self AND NOT person.removed
"#
);

// The other current members. Those with a profile come first, by its name in
// the device's language; the read reads `name_sort`, so coven reruns it when
// the device's language changes. Those with no profile yet follow, and member
// id orders the rest.
sqlive_compose::read!(
    writer OtherPeople,
    r#"
    SELECT person.*
    FROM @crate::people::profiles::MemberWithProfile AS person
    JOIN @bae_records::records::name_sort::NameSortLanguage AS sort ON 1
    WHERE NOT person.is_self AND NOT person.removed
    ORDER BY person.id IS NULL, bae_name_key(person.name, sort.language), person.member_id
"#
);

// How many people the library has and how many active devices they have.
sqlive_compose::read!(
    writer PeopleCounts { people: u32, devices: u32 },
    r#"
    SELECT count(*) AS people, coalesce(sum(member.active_device_count), 0) AS devices
    FROM @crate::membership::fragments::Member AS member
    WHERE NOT member.removed
"#
);

// The member this device writes as: its own member, also after the library
// removed them, with their profile, none until a device of theirs has written
// it.
sqlive_compose::read!(
    writer WritingPerson { member_id: coven::MemberId },
    r#"
    SELECT own.member_id, profile.*
    FROM (SELECT m.member AS member_id FROM coven_members AS m WHERE m.is_self) AS own
    LEFT JOIN @crate::people::fragments::Profile AS profile ON profile.id = own.member_id
"#
);

// The profile of this device's person while the library holds them: the name
// an invite gives its inviter.
sqlive_compose::read!(
    writer InviterProfile,
    r#"
    SELECT profile.*
    FROM (SELECT m.member AS member_id FROM coven_members AS m
          WHERE m.is_self AND NOT m.removed) AS own
    JOIN @crate::people::fragments::Profile AS profile ON profile.id = own.member_id
"#
);

/// Who this device writes as, for plays, ratings, playlists and their
/// profile: its own member, who stays this device's person after the library
/// removes them, as the removed device keeps its copy and keeps writing to
/// it.
pub(super) struct Writer {
    pub(super) member: coven::MemberId,
    /// `None` until a device of the member's has written it.
    pub(super) profile: Option<PersonProfile>,
}

/// Who this device writes as; `None` until its registration has applied.
pub(super) fn writer_on(sql: &impl bae_store::WriterRead) -> Result<Option<Writer>, DbError> {
    Ok(statement_optional(sql, &WritingPersonParams {})?.map(|row| Writer {
        member: row.member_id,
        profile: row.profile.map(Profile::into_profile),
    }))
}

/// The profile this device writes as, or why it has none.
fn writing_profile_on(
    sql: &impl bae_store::WriterRead,
) -> Result<Result<PersonProfile, ProfileError>, DbError> {
    Ok(match writer_on(sql)? {
        None => Err(ProfileError::NotRegistered),
        Some(writer) => writer.profile.ok_or(ProfileError::Missing),
    })
}

// The current member `:member`.
sqlive_compose::read!(
    writer CurrentPerson(member: coven::MemberId),
    r#"
    SELECT person.*
    FROM @crate::people::profiles::MemberWithProfile AS person
    WHERE person.member_id = :member AND NOT person.removed
"#
);

// The profile of the person `:member`.
sqlive_compose::read!(
    writer ProfileOf(member: coven::MemberId),
    r#"
    SELECT profile.*
    FROM @crate::people::fragments::Profile AS profile
    WHERE profile.id = :member
"#
);

impl People {
    /// The person using this device, once their registration has applied:
    /// their membership, and their profile, which the same registration wrote.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn own_person(&self) -> Result<Option<Person>, DbError> {
        self.inner.store.read(|sql| own_person_on(&sql)).await
    }

    /// The person using this device, the other people, and how many people and
    /// devices the library has, live: a statement for each, over one snapshot
    /// of coven's members and devices and the profiles, so a screen draws its
    /// sections and its counts from the same moment.
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
                .map(|row| row.person.into_person())
                .transpose()
        })
    }

    pub async fn profile(&self, member: coven::MemberId) -> Result<Option<PersonProfile>, DbError> {
        self.inner
            .store
            .read(move |sql| {
                statement_optional(&sql, &ProfileOfParams { member })?
                    .map(|row| Ok(row.profile.into_profile()))
                    .transpose()
            })
            .await
    }

    /// Write as this device's person: `write` is handed their profile, read
    /// in the write's own transaction, which writes nothing when this device
    /// has no profile.
    pub(super) async fn write_as_self<T, E>(
        &self,
        write: impl for<'ctx> FnOnce(&CovenWrite<'ctx>, PersonProfile) -> Result<Result<T, E>, DbError>
            + Send
            + 'static,
    ) -> Result<T, E>
    where
        T: Send + 'static,
        E: From<ProfileError> + From<DbError> + Send + 'static,
    {
        self.inner
            .store
            .call(move |sql| match writing_profile_on(sql)? {
                Ok(person) => write(sql, person),
                Err(refused) => Ok(Err(refused.into())),
            })
            .await?
    }

    pub async fn rename_profile(&self, name: &str) -> Result<(), ProfileError> {
        let name = bae_store::EnteredName::parse(name)?.into_string();
        self.write_as_self(move |sql, person| {
            update_profile(
                sql,
                "UPDATE people SET name = ? WHERE id = ?",
                params![name, person.id.to_string()],
            )?;
            Ok(Ok(()))
        })
        .await
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
        self.write_as_self(move |sql, person| {
            update_profile(
                sql,
                "UPDATE people SET history_sharing = ? WHERE id = ?",
                params![sharing.as_str(), person.id.to_string()],
            )?;
            Ok(Ok(()))
        })
        .await
    }

    /// Set the person's avatar to `avatar`, a JPEG, staged as a new coven
    /// file the profile row references; `None` clears it. The file the row
    /// named before goes with its last reference.
    pub async fn set_avatar(&self, avatar: Option<Vec<u8>>) -> Result<(), ProfileError> {
        let own = self.inner.store.read(|sql| writing_profile_on(&sql)).await??;
        let person = own.id.to_string();
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
    sql: &impl bae_store::WriterRead,
) -> Result<Option<String>, DbError> {
    Ok(statement_optional(sql, &InviterProfileParams {})?
        .map(|row| row.profile.into_profile().name))
}

fn people_view_on(sql: &SqlReadContext<'_>) -> Result<PeopleView, DbError> {
    let own = own_person_on(sql)?;
    let others = statement_rows(sql, &OtherPeopleParams {})?
        .into_iter()
        .map(|row| row.person.into_person())
        .collect::<Result<_, _>>()?;
    let counts = statement_row(sql, &PeopleCountsParams {})?;
    Ok(PeopleView { own, others, people: counts.people.into(), devices: counts.devices.into() })
}

fn own_person_on(sql: &SqlReadContext<'_>) -> Result<Option<Person>, DbError> {
    statement_optional(sql, &OwnPersonParams {})?.map(|row| row.person.into_person()).transpose()
}

impl MemberWithProfile {
    fn into_person(self) -> Result<Person, DbError> {
        Ok(Person {
            profile: self.profile.map(Profile::into_profile),
            membership: self.member.into_membership()?,
        })
    }
}

impl Profile {
    pub(super) fn into_profile(self) -> PersonProfile {
        PersonProfile {
            id: self.id,
            name: self.name,
            private_circle: self.private_circle,
            history_sharing: self.history_sharing,
            avatar: self.avatar.map(|stored| {
                let (file, version) = stored.into_parts();
                Avatar { file, version }
            }),
        }
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
