use super::*;
#[cfg(any(test, feature = "test-utils"))]
use crate::collections::Rating;
use crate::collections::{
    AlbumRatings, CollectionError, OtherRating, Playlist, PlaylistEntry, RatingTarget, Stars,
};
use crate::people::Sharing;

impl People {
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
        self.inner.store.call(move |sql| {
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

    #[cfg(any(test, feature = "test-utils"))]
    pub async fn ratings(&self) -> Result<Vec<Rating>, DbError> {
        self.inner.store.read(|sql| Ok(ratings(&sql)?)).await
    }

    /// Writes the playlist and its `entries` (a position id and a track id
    /// each, at positions 0…n) in one write, so a playlist is never seen, or
    /// left, without the tracks it was made with. The caller obtains ids from
    /// its injected IdSource, including on retries.
    pub async fn create_playlist(
        &self,
        id: uuid::Uuid,
        name: &str,
        sharing: Sharing,
        entries: Vec<(uuid::Uuid, String)>,
    ) -> Result<Playlist, CollectionError> {
        let name = bae_store::EnteredName::parse(name)?.into_string();
        let person = self.own_profile().await?;
        let playlist = Playlist {
            id,
            person: person.id,
            name,
            audience: sharing.audience(person.private_circle),
        };
        let row = playlist.clone();
        self.inner
            .store
            .call(move |sql| {
                sql.execute(
                    "INSERT INTO playlists (id, person_id, name, audience) VALUES (?, ?, ?, ?)",
                    params![row.id.to_string(), row.person.to_string(), row.name, row.audience],
                )?;
                for (position, (position_id, track_id)) in entries.iter().enumerate() {
                    sql.execute(
                        "INSERT INTO playlist_entries (playlist_id, position_id, track_id, position) VALUES (?, ?, ?, ?)",
                        params![row.id.to_string(), position_id.to_string(), track_id, position as i64],
                    )?;
                }
                Ok(())
            })
            .await?;
        Ok(playlist)
    }

    pub async fn rename_playlist(&self, id: uuid::Uuid, name: &str) -> Result<(), CollectionError> {
        let name = bae_store::EnteredName::parse(name)?.into_string();
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
        self.inner.store.read(move |sql| Ok(entries(&sql, id)?)).await
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
            Ok(if changed == 1 { Ok(()) } else { Err(CollectionError::Missing) })
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
        change: impl FnOnce(&CovenWrite<'_>) -> Result<Result<(), CollectionError>, DbError>
            + Send
            + 'static,
    ) -> Result<(), CollectionError> {
        let person = self.own_profile().await?.id.to_string();
        self.inner
            .store
            .call(move |sql| {
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

#[cfg(any(test, feature = "test-utils"))]
fn ratings(sql: &SqlReadContext<'_>) -> rusqlite::Result<Vec<Rating>> {
    sql.query("SELECT person_id, album_id, track_id, stars FROM ratings ORDER BY id", [], |row| {
        let target = match (row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?) {
            (Some(album), None) => RatingTarget::Album(album),
            (None, Some(track)) => RatingTarget::Track(track),
            _ => return Err(rusqlite::Error::InvalidQuery),
        };
        Ok(Rating {
            person: coven::MemberId(uuid_column(row, 0)?),
            target,
            stars: Stars::from_sql(row.get(3)?)?,
        })
    })
}

/// One playlist's entries, in order; `playlist_entries_position` serves the
/// lookup and the order.
pub(super) const PLAYLIST_ENTRIES_SQL: &str = "SELECT position_id, track_id FROM playlist_entries \
     WHERE playlist_id = ? ORDER BY position, position_id";

fn entries(sql: &SqlReadContext<'_>, playlist: uuid::Uuid) -> rusqlite::Result<Vec<PlaylistEntry>> {
    sql.query(PLAYLIST_ENTRIES_SQL, [playlist.to_string()], |row| {
        Ok(PlaylistEntry { position_id: uuid_column(row, 0)?, track_id: row.get(1)? })
    })
}

#[cfg(test)]
#[path = "collections_tests.rs"]
mod tests;

/// The member using this device, and whether they have a profile; `None`
/// until their registration has applied. A profile is what ratings and
/// playlists belong to.
fn own_member_sql() -> String {
    format!(
        "SELECT m.member, EXISTS (SELECT 1 FROM people AS p WHERE p.id = m.member) \
         FROM coven_members AS m WHERE NOT m.removed AND {}",
        super::profiles::IS_OWN_MEMBER
    )
}

/// Every person's stars for the album `?1`, by name then id; `ratings_album`
/// finds the album's ratings.
pub(super) const ALBUM_RATINGS_SQL: &str = "SELECT r.person_id, p.name, r.stars \
     FROM ratings AS r JOIN people AS p ON p.id = r.person_id \
     WHERE r.album_id = ?1 ORDER BY p.name, p.id";

/// This person's stars for the tracks of the release `?2`, by the track,
/// found through `ratings`' key on the person and the track.
pub(super) const TRACK_STARS_SQL: &str = "SELECT r.track_id, r.stars FROM ratings AS r \
     JOIN tracks AS t ON t.id = r.track_id WHERE r.person_id = ?1 AND t.release_id = ?2";

/// The id of this person's profile, once they have one.
fn own_profile_id(sql: &SqlReadContext<'_>) -> rusqlite::Result<Option<(String, bool)>> {
    Ok(sql
        .query(&own_member_sql(), [], |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)))?
        .pop())
}

/// What the album `album_id`'s page reads of its ratings, for the person
/// using this device.
pub fn album_ratings_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> rusqlite::Result<AlbumRatings> {
    let own = own_profile_id(sql)?;
    let rows = sql.query(ALBUM_RATINGS_SQL, [album_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, Stars::from_sql(row.get(2)?)?))
    })?;
    let mut mine = None;
    let mut others = Vec::new();
    for (person, name, stars) in rows {
        match &own {
            Some((member, true)) if *member == person => mine = Some(stars),
            _ => others.push(OtherRating { name, stars }),
        }
    }
    Ok(AlbumRatings { can_rate: matches!(own, Some((_, true))), mine, others })
}

/// This person's stars for the tracks of the release `release_id` that they
/// rated, by track id; none without a profile.
pub fn track_stars_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> rusqlite::Result<std::collections::HashMap<String, Stars>> {
    let Some((member, true)) = own_profile_id(sql)? else {
        return Ok(std::collections::HashMap::new());
    };
    Ok(sql
        .query(TRACK_STARS_SQL, params![member, release_id], |row| {
            Ok((row.get::<_, String>(0)?, Stars::from_sql(row.get(1)?)?))
        })?
        .into_iter()
        .collect())
}
