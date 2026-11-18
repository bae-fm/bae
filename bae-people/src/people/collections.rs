use super::*;
#[cfg(any(test, feature = "test-utils"))]
use crate::collections::Rating;
use crate::collections::{
    AlbumRatings, CollectionError, EntryPlace, OtherRating, Playlist, PlaylistEntry, RatingTarget,
    Stars,
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

    /// Writes the playlist and an entry for each of `tracks`, in the list's
    /// order at positions 0…n, with the position ids `positions` (one per
    /// track the list names), in one write, so a playlist is never seen, or
    /// left, without the tracks it was made with. A given track the library
    /// lacks fails the write; a picked list makes entries only for the tracks
    /// the library holds.
    /// The caller obtains ids from its injected IdSource, including on
    /// retries.
    pub async fn create_playlist(
        &self,
        id: uuid::Uuid,
        name: &str,
        sharing: Sharing,
        positions: Vec<uuid::Uuid>,
        tracks: bae_records::TrackList,
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
                let reading = match tracks {
                    bae_records::TrackList::Given(_) => bae_records::TrackReading::Named,
                    bae_records::TrackList::Picked(_) => bae_records::TrackReading::Held,
                };
                let track_ids = tracks.ordered_on(sql, reading)?;
                assert!(positions.len() >= track_ids.len(), "a position id for each entry");
                for (position, (position_id, track_id)) in positions.iter().zip(&track_ids).enumerate() {
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

    /// Sets a playlist's name and who sees it in one write, so no reader ever
    /// sees the playlist with only one of the two changed. edit_playlist
    /// refuses a blank name before it writes anything.
    pub async fn edit_playlist(
        &self,
        id: uuid::Uuid,
        name: &str,
        sharing: Sharing,
    ) -> Result<(), CollectionError> {
        let name = bae_store::EnteredName::parse(name)?.into_string();
        let audience = sharing.audience(self.own_profile().await?.private_circle);
        self.change_playlist(id, move |sql| {
            sql.execute(
                "UPDATE playlists SET name = ?, audience = ? WHERE id = ?",
                params![name, audience, id.to_string()],
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
        self.append_playlist_entries(playlist, vec![position], vec![track].into()).await
    }

    /// Adds an entry for each of `tracks` after the playlist's last entry, in
    /// the list's order, with the fresh position ids `positions` (one per
    /// track), in one write. A track the library lacks fails the write.
    pub async fn append_playlist_entries(
        &self,
        playlist: uuid::Uuid,
        positions: Vec<uuid::Uuid>,
        tracks: bae_records::TrackList,
    ) -> Result<(), CollectionError> {
        self.change_playlist(playlist, move |sql| {
            let track_ids = tracks.ordered_on(sql, bae_records::TrackReading::Named)?;
            assert_eq!(positions.len(), track_ids.len(), "a playlist entry for each track");
            for (position, track) in positions.iter().zip(&track_ids) {
                sql.execute("INSERT INTO playlist_entries (playlist_id, position_id, track_id, position) SELECT ?1, ?2, ?3, COALESCE(MAX(position) + 1, 0) FROM playlist_entries WHERE playlist_id = ?1", params![playlist.to_string(), position.to_string(), track])?;
            }
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

    /// Moves the entry `position` to `place` in the playlist, and renumbers
    /// every entry from 0 in the resulting order. Fails as missing when the
    /// playlist holds no such entry, or no entry `place` names.
    pub async fn move_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        position: uuid::Uuid,
        place: EntryPlace,
    ) -> Result<(), CollectionError> {
        self.change_playlist(playlist, move |sql| {
            let Some(to) = place_among_others_on(sql, playlist, position, place)? else {
                return Ok(Err(CollectionError::Missing));
            };
            sql.execute(
                "WITH others AS ( \
                     SELECT position_id, ROW_NUMBER() OVER (ORDER BY position) - 1 AS place \
                     FROM playlist_entries WHERE playlist_id = ?1 AND position_id <> ?2), \
                 placed AS ( \
                     SELECT position_id, CASE WHEN place >= ?3 THEN place + 1 ELSE place END \
                            AS place FROM others \
                     UNION ALL \
                     SELECT ?2, MIN(?3, (SELECT COUNT(*) FROM others))) \
                 UPDATE playlist_entries SET position = placed.place FROM placed \
                 WHERE playlist_entries.playlist_id = ?1 \
                   AND playlist_entries.position_id = placed.position_id",
                params![playlist.to_string(), position.to_string(), to],
            )?;
            Ok(Ok(()))
        })
        .await
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
            stars: crate::collections::stars_from_sql(row.get(3)?)?,
        })
    })
}

/// One playlist's entries, in order; `playlist_entries_position` serves the
/// lookup and the order.
pub(super) const PLAYLIST_ENTRIES_SQL: &str = "SELECT position_id, track_id FROM playlist_entries \
     WHERE playlist_id = ? ORDER BY position";

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
        "SELECT m.member AS member, \
                EXISTS (SELECT 1 FROM people AS p WHERE p.id = m.member) AS has_profile \
         FROM coven_members AS m WHERE NOT m.removed AND {}",
        super::profiles::IS_OWN_MEMBER
    )
}

/// What the album `?1`'s page reads of its ratings, in one row: whether this
/// device's person has a profile to rate with, whether the page shows a rating
/// row (they can rate, or someone rated the album), their own stars, and every
/// other person's, by name then id, as a JSON array; `ratings_album` finds
/// the album's ratings. A person with no registration yet is none of the
/// ratings' people.
pub(super) fn album_ratings_sql() -> String {
    format!(
        "WITH album(id) AS (SELECT ?1) \
         SELECT COALESCE(own.has_profile, 0) AS can_rate, \
                COALESCE(own.has_profile, 0) \
                    OR EXISTS (SELECT 1 FROM ratings AS r WHERE r.album_id = album.id) AS shown, \
                (SELECT r.stars FROM ratings AS r \
                 WHERE r.album_id = album.id AND r.person_id = own.member) AS mine, \
                (SELECT json_group_array(json_object('name', p.name, 'stars', r.stars) \
                        ORDER BY p.name, p.id) \
                 FROM ratings AS r JOIN people AS p ON p.id = r.person_id \
                 WHERE r.album_id = album.id AND r.person_id IS NOT own.member) AS others \
         FROM album LEFT JOIN ({own}) AS own ON 1",
        own = own_member_sql(),
    )
}

/// The stars this device's person gave the track `track_alias`, an
/// expression for a statement over `tracks`: NULL for a track they did not
/// rate, and for every track while they have no profile. SQLite finds the
/// rating through `ratings`' key on the person and the track.
pub fn own_track_stars_sql(track_alias: &str) -> String {
    format!(
        "(SELECT r.stars FROM ratings AS r WHERE r.track_id = {track_alias}.id \
          AND r.person_id = (SELECT own.member FROM ({own}) AS own))",
        own = own_member_sql(),
    )
}

/// What the album `album_id`'s page reads of its ratings, for the person
/// using this device.
pub fn album_ratings_on(
    sql: &SqlReadContext<'_>,
    album_id: &str,
) -> rusqlite::Result<AlbumRatings> {
    sql.query_row(&album_ratings_sql(), [album_id], |row| {
        let stars_of = |value: i64| crate::collections::stars_from_sql(value);
        let others: Vec<StoredRating> = bae_store::read::json_column(row, "others")?;
        Ok(AlbumRatings {
            can_rate: row.get("can_rate")?,
            shown: row.get("shown")?,
            mine: row.get::<_, Option<i64>>("mine")?.map(stars_of).transpose()?,
            others: others
                .into_iter()
                .map(|other| Ok(OtherRating { name: other.name, stars: stars_of(other.stars)? }))
                .collect::<rusqlite::Result<_>>()?,
        })
    })
}

/// One other person's rating as [`album_ratings_sql`] writes it into its
/// `others` array.
#[derive(serde::Deserialize)]
struct StoredRating {
    name: String,
    stars: i64,
}

/// The place, counted from 0 among the playlist's other entries, that a move
/// of the entry `moved` to `place` puts it at; none when the playlist holds no
/// such entry, or no entry `place` names.
fn place_among_others_on(
    sql: &CovenWrite<'_>,
    playlist: uuid::Uuid,
    moved: uuid::Uuid,
    place: EntryPlace,
) -> Result<Option<i64>, DbError> {
    // The entries before `beside`, other than the moved one; `after` counts
    // `beside` itself as well.
    let before_entry = |beside: uuid::Uuid, after: bool| -> Result<Option<i64>, DbError> {
        Ok(sql
            .query_row(
                &format!(
                    "SELECT (SELECT COUNT(*) FROM playlist_entries AS other \
                             WHERE other.playlist_id = ?1 AND other.position_id <> ?2 \
                               AND other.position {compare} beside.position) \
                     FROM playlist_entries AS beside \
                     WHERE beside.playlist_id = ?1 AND beside.position_id = ?3 \
                       AND beside.position_id <> ?2 \
                       AND EXISTS (SELECT 1 FROM playlist_entries \
                                   WHERE playlist_id = ?1 AND position_id = ?2)",
                    compare = if after { "<=" } else { "<" },
                ),
                params![playlist.to_string(), moved.to_string(), beside.to_string()],
                |row| row.get(0),
            )
            .optional()?)
    };
    match place {
        EntryPlace::Before(beside) => before_entry(beside, false),
        EntryPlace::After(beside) => before_entry(beside, true),
        EntryPlace::Up | EntryPlace::Down => {
            let own: Option<i64> = sql
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM playlist_entries AS other \
                             WHERE other.playlist_id = ?1 AND other.position < own.position) \
                     FROM playlist_entries AS own \
                     WHERE own.playlist_id = ?1 AND own.position_id = ?2",
                    params![playlist.to_string(), moved.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(own.map(|own| if place == EntryPlace::Up { (own - 1).max(0) } else { own + 1 }))
        }
    }
}
