use super::*;
use crate::people::{PersonProfile, ProfileError, Sharing};

const PROFILE_COLUMNS: &str = "id, name, private_circle, history_sharing, avatar_id";

impl Database {
    pub(crate) async fn current_member(&self) -> Result<Option<coven::MemberId>, DbError> {
        Ok(self
            .inner
            .handle
            .members()
            .await?
            .into_iter()
            .find(|member| member.is_self)
            .map(|member| member.id))
    }

    pub async fn profiles(&self) -> Result<Vec<PersonProfile>, DbError> {
        self.read(|sql| Ok(profile_rows(&sql)?)).await
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
        let member = self
            .current_member()
            .await?
            .ok_or(ProfileError::NotRegistered)?;
        self.profile(member).await?.ok_or(ProfileError::Missing)
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
            return if profile.name == name {
                Ok(profile)
            } else {
                Err(ProfileError::AlreadyExists)
            };
        }
        let circle_name = format!("bae-person:{member}");
        let circles = self.inner.handle.circles();
        let matching: Vec<_> = circles
            .list()
            .await?
            .into_iter()
            .filter(|circle| circle.name == circle_name)
            .collect();
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
                Box::pin(circles.create(&circle_name)).await?
            }
            [circle] => {
                let members = circles.members(circle.id).await?;
                if members.len() != 1 || members[0].member != member {
                    return Err(ProfileError::InvalidPrivateCircle);
                }
                circle.id
            }
            _ => return Err(ProfileError::InvalidPrivateCircle),
        };
        let profile = PersonProfile {
            id: member,
            name,
            private_circle,
            history_sharing: Sharing::Private,
            avatar_id: None,
        };
        let row = profile.clone();
        self.call(move |sql| {
            sql.execute("INSERT INTO people (id, name, private_circle, history_sharing) VALUES (?1, ?2, ?3, ?4)",
                params![row.id.to_string(), row.name, row.private_circle.to_string(), row.history_sharing.as_str()])?;
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

    pub async fn avatar(&self, person: coven::MemberId) -> Result<Option<Vec<u8>>, ProfileError> {
        let profile = self.profile(person).await?.ok_or(ProfileError::Missing)?;
        if profile.avatar_id.is_none() {
            return Ok(None);
        }
        let file = self
            .inner
            .handle
            .file_ref("people", person.to_string().as_str())
            .await
            .map_err(DbError::from)?;
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
    fn id(row: &Row<'_>, index: usize) -> rusqlite::Result<uuid::Uuid> {
        let text: String = row.get(index)?;
        uuid::Uuid::parse_str(&text).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
    }
    Ok(PersonProfile {
        id: coven::MemberId(id(row, 0)?),
        name: row.get(1)?,
        private_circle: coven::CircleId(id(row, 2)?),
        history_sharing: Sharing::from_sql(&row.get::<_, String>(3)?)?,
        avatar_id: row.get(4)?,
    })
}

fn profile_rows(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<PersonProfile>> {
    sql.query(
        &format!("SELECT {PROFILE_COLUMNS} FROM people ORDER BY name, id"),
        [],
        profile_row,
    )
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
