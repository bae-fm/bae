use super::*;
use crate::collections::{CollectionError, Playlist, PlaylistEntry, Rating, RatingTarget, Stars};
use crate::people::Sharing;

impl Database {
    pub async fn set_rating(
        &self,
        target: RatingTarget,
        stars: Option<Stars>,
    ) -> Result<(), CollectionError> {
        let person = self.own_profile().await?.id;
        let (kind, target_id, album, track) = match target {
            RatingTarget::Album(id) => ("album", id.clone(), Some(id), None),
            RatingTarget::Track(id) => ("track", id.clone(), None, Some(id)),
        };
        let id = format!("{person}:{kind}:{target_id}");
        self.call(move |sql| {
            match stars {
                Some(stars) => {
                    sql.execute("INSERT INTO ratings (id, person_id, album_id, track_id, stars) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET stars = excluded.stars", params![id, person.to_string(), album, track, stars as i64])?;
                }
                None => { sql.execute("DELETE FROM ratings WHERE id = ?", [id])?; }
            }
            Ok(())
        }).await?;
        Ok(())
    }

    pub async fn ratings(&self) -> Result<Vec<Rating>, DbError> {
        self.read(|sql| Ok(ratings(&sql)?)).await
    }

    pub fn subscribe_ratings(&self) -> coven::LiveQuery<Vec<Rating>> {
        self.inner.handle.subscribe(|sql| Ok(ratings(&sql)?))
    }

    /// The caller obtains ids from its injected IdSource, including on retries.
    pub async fn create_playlist(
        &self,
        id: uuid::Uuid,
        name: &str,
        sharing: Sharing,
    ) -> Result<Playlist, CollectionError> {
        let name = playlist_name(name)?;
        let person = self.own_profile().await?;
        let playlist = Playlist {
            id,
            person: person.id,
            name,
            audience: sharing.audience(person.private_circle),
        };
        let row = playlist.clone();
        self.call(move |sql| {
            sql.execute(
                "INSERT INTO playlists (id, person_id, name, audience) VALUES (?, ?, ?, ?)",
                params![
                    row.id.to_string(),
                    row.person.to_string(),
                    row.name,
                    row.audience
                ],
            )?;
            Ok(())
        })
        .await?;
        Ok(playlist)
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>, DbError> {
        self.read(|sql| Ok(playlists(&sql)?)).await
    }

    pub fn subscribe_playlists(&self) -> coven::LiveQuery<Vec<Playlist>> {
        self.inner.handle.subscribe(|sql| Ok(playlists(&sql)?))
    }

    pub async fn rename_playlist(&self, id: uuid::Uuid, name: &str) -> Result<(), CollectionError> {
        let name = playlist_name(name)?;
        self.change_playlist(id, move |sql| {
            sql.execute(
                "UPDATE playlists SET name = ? WHERE id = ?",
                params![name, id.to_string()],
            )?;
            Ok(Ok(()))
        })
        .await
    }

    pub async fn set_playlist_sharing(
        &self,
        id: uuid::Uuid,
        sharing: Sharing,
    ) -> Result<(), CollectionError> {
        let audience = sharing.audience(self.own_profile().await?.private_circle);
        self.change_playlist(id, move |sql| {
            sql.execute(
                "UPDATE playlists SET audience = ? WHERE id = ?",
                params![audience, id.to_string()],
            )?;
            Ok(Ok(()))
        })
        .await
    }

    pub async fn delete_playlist(&self, id: uuid::Uuid) -> Result<(), CollectionError> {
        self.change_playlist(id, move |sql| {
            sql.execute("DELETE FROM playlists WHERE id = ?", [id.to_string()])?;
            Ok(Ok(()))
        })
        .await
    }

    pub async fn playlist_entries(&self, id: uuid::Uuid) -> Result<Vec<PlaylistEntry>, DbError> {
        self.read(move |sql| Ok(entries(&sql, id)?)).await
    }

    pub fn subscribe_playlist_entries(
        &self,
        id: uuid::Uuid,
    ) -> coven::LiveQuery<Vec<PlaylistEntry>> {
        self.inner
            .handle
            .subscribe(move |sql| Ok(entries(&sql, id)?))
    }

    pub async fn append_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        position: uuid::Uuid,
        track: String,
    ) -> Result<(), CollectionError> {
        self.change_playlist(playlist, move |sql| {
            sql.execute("INSERT INTO playlist_entries (playlist_id, position_id, track_id, position) SELECT ?1, ?2, ?3, COALESCE(MAX(position) + 1, 0) FROM playlist_entries WHERE playlist_id = ?1", params![playlist.to_string(), position.to_string(), track])?;
            Ok(Ok(()))
        }).await
    }

    pub async fn remove_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        position: uuid::Uuid,
    ) -> Result<(), CollectionError> {
        self.change_playlist(playlist, move |sql| {
            let changed = sql.execute(
                "DELETE FROM playlist_entries WHERE playlist_id = ? AND position_id = ?",
                params![playlist.to_string(), position.to_string()],
            )?;
            Ok(if changed == 1 {
                Ok(())
            } else {
                Err(CollectionError::Missing)
            })
        })
        .await
    }

    /// A stale reorder cannot silently drop entries received from another device.
    pub async fn reorder_playlist(
        &self,
        playlist: uuid::Uuid,
        order: Vec<uuid::Uuid>,
    ) -> Result<(), CollectionError> {
        self.change_playlist(playlist, move |sql| {
            let current: HashSet<String> = sql.query("SELECT position_id FROM playlist_entries WHERE playlist_id = ?", [playlist.to_string()], |row| row.get(0))?.into_iter().collect();
            let wanted: HashSet<String> = order.iter().map(ToString::to_string).collect();
            if order.len() != wanted.len() || wanted != current { return Ok(Err(CollectionError::OrderChanged)); }
            for (index, position) in order.iter().enumerate() {
                sql.execute("UPDATE playlist_entries SET position = ? WHERE playlist_id = ? AND position_id = ?", params![index as i64, playlist.to_string(), position.to_string()])?;
            }
            Ok(Ok(()))
        }).await
    }

    async fn change_playlist(
        &self,
        id: uuid::Uuid,
        change: impl FnOnce(&SqlContext<'_, '_>) -> Result<Result<(), CollectionError>, DbError>
            + Send
            + 'static,
    ) -> Result<(), CollectionError> {
        let person = self.own_profile().await?.id.to_string();
        self.call(move |sql| {
            let owner: Option<String> = sql
                .query_row(
                    "SELECT person_id FROM playlists WHERE id = ?",
                    [id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            match owner {
                None => Ok(Err(CollectionError::Missing)),
                Some(owner) if owner != person => Ok(Err(CollectionError::NotOwner)),
                Some(_) => change(sql),
            }
        })
        .await?
    }
}

fn playlist_name(name: &str) -> Result<String, CollectionError> {
    let name = name.trim();
    if name.is_empty() {
        Err(CollectionError::EmptyName)
    } else {
        Ok(name.to_owned())
    }
}

fn ratings(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<Rating>> {
    sql.query(
        "SELECT person_id, album_id, track_id, stars FROM ratings ORDER BY id",
        [],
        |row| {
            let target = match (
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ) {
                (Some(album), None) => RatingTarget::Album(album),
                (None, Some(track)) => RatingTarget::Track(track),
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            Ok(Rating {
                person: coven::MemberId(uuid_column(row, 0)?),
                target,
                stars: Stars::from_sql(row.get(3)?)?,
            })
        },
    )
}

fn playlists(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<Playlist>> {
    sql.query(
        "SELECT id, person_id, name, audience FROM playlists ORDER BY name, id",
        [],
        |row| {
            Ok(Playlist {
                id: uuid_column(row, 0)?,
                person: coven::MemberId(uuid_column(row, 1)?),
                name: row.get(2)?,
                audience: row.get(3)?,
            })
        },
    )
}

fn entries(sql: &SqlReadContext<'_>, playlist: uuid::Uuid) -> rusqlite::Result<Vec<PlaylistEntry>> {
    sql.query("SELECT position_id, track_id FROM playlist_entries WHERE playlist_id = ? ORDER BY position, position_id", [playlist.to_string()], |row| {
        Ok(PlaylistEntry { position_id: uuid_column(row, 0)?, track_id: row.get(1)? })
    })
}
