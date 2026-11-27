use super::rating_systems::MIN_HEART_STARS;
use super::*;
#[cfg(any(test, feature = "test-utils"))]
use crate::collections::Rating;
use crate::collections::{
    AlbumRatings, BesideEntry, CollectionError, EntryPlace, InsertPlace, OtherRating, Playlist,
    PlaylistEntry, RatingTarget, Stars,
};
use crate::people::{PersonProfile, Sharing};
use bae_store::WriteStatements;

sqlive_compose::write!(
    KeepRating(
        id: &'p str,
        person: coven::MemberId,
        album: Option<&'p str>,
        track: Option<&'p str>,
        stars: i64,
    ),
    r#"
    INSERT INTO ratings (id, person_id, album_id, track_id, stars)
    VALUES (:id, :person, :album, :track, :stars)
    ON CONFLICT (id) DO UPDATE SET stars = excluded.stars
"#
);

sqlive_compose::write!(DeleteRating(id: &'p str), r#"DELETE FROM ratings WHERE id = :id"#);

sqlive_compose::write!(
    InsertPlaylist(
        id: &'p str,
        person: coven::MemberId,
        name: &'p str,
        audience: &'p coven::Audience,
    ),
    r#"
    INSERT INTO playlists (id, person_id, name, audience)
    VALUES (:id, :person, :name, :audience)
"#
);

sqlive_compose::write!(
    SetPlaylist(id: &'p str, name: &'p str, audience: &'p coven::Audience),
    r#"UPDATE playlists SET name = :name, audience = :audience WHERE id = :id"#
);

sqlive_compose::write!(DeletePlaylist(id: &'p str), r#"DELETE FROM playlists WHERE id = :id"#);

sqlive_compose::write!(
    InsertPlaylistEntry(playlist: &'p str, entry: &'p str, track: &'p str, position: i64),
    r#"
    INSERT INTO playlist_entries (playlist_id, position_id, track_id, position)
    VALUES (:playlist, :entry, :track, :position)
"#
);

// An entry after the playlist's last one.
sqlive_compose::write!(
    AppendPlaylistEntry(playlist: &'p str, entry: &'p str, track: &'p str),
    r#"
    INSERT INTO playlist_entries (playlist_id, position_id, track_id, position)
    SELECT :playlist, :entry, :track, COALESCE(MAX(held.position) + 1, 0)
    FROM playlist_entries AS held
    WHERE held.playlist_id = :playlist
"#
);

sqlive_compose::write!(
    DeletePlaylistEntry(playlist: &'p str, entry: &'p str),
    r#"DELETE FROM playlist_entries WHERE playlist_id = :playlist AND position_id = :entry"#
);

// The entry `entry` at place `to` among the playlist's others, and every entry
// numbered from 0 in that order.
sqlive_compose::write!(
    MovePlaylistEntry(playlist: &'p str, entry: &'p str, to: i64),
    r#"
    WITH others AS (
        SELECT position_id, ROW_NUMBER() OVER (ORDER BY position) - 1 AS place
        FROM playlist_entries WHERE playlist_id = :playlist AND position_id <> :entry),
    placed AS (
        SELECT position_id, CASE WHEN place >= :to THEN place + 1 ELSE place END AS place
        FROM others
        UNION ALL
        SELECT :entry, MIN(:to, (SELECT COUNT(*) FROM others)))
    UPDATE playlist_entries SET position = placed.place FROM placed
    WHERE playlist_entries.playlist_id = :playlist
      AND playlist_entries.position_id = placed.position_id
"#
);

// The playlist's entries numbered from 0, with `entries` numbers left free
// before the entry now at place `at`.
sqlive_compose::write!(
    MakeRoomForPlaylistEntries(playlist: &'p str, at: i64, entries: i64),
    r#"
    WITH placed AS (
        SELECT position_id, ROW_NUMBER() OVER (ORDER BY position) - 1 AS place
        FROM playlist_entries WHERE playlist_id = :playlist)
    UPDATE playlist_entries
    SET position = CASE WHEN placed.place >= :at THEN placed.place + :entries
                        ELSE placed.place END
    FROM placed
    WHERE playlist_entries.playlist_id = :playlist
      AND playlist_entries.position_id = placed.position_id
"#
);

impl People {
    pub async fn set_rating(
        &self,
        target: RatingTarget,
        stars: Option<Stars>,
    ) -> Result<(), CollectionError> {
        let (kind, target_id, album, track) = match target {
            RatingTarget::Album(id) => ("album", id.clone(), Some(id), None),
            RatingTarget::Track(id) => ("track", id.clone(), None, Some(id)),
        };
        self.write_as_self(move |sql, person| {
            let person = person.id;
            let id = format!("{person}:{kind}:{target_id}");
            match stars {
                Some(stars) => {
                    sql.run(&KeepRatingParams {
                        id: &id,
                        person,
                        album: album.as_deref(),
                        track: track.as_deref(),
                        stars: stars as i64,
                    })?;
                }
                None => {
                    sql.run(&DeleteRatingParams { id: &id })?;
                }
            }
            Ok(Ok(()))
        })
        .await
    }

    /// Turn this person's heart on `target` on or off. A heart is five stars
    /// and no heart is no rating, whatever stars the target held before.
    pub async fn set_hearted(
        &self,
        target: RatingTarget,
        hearted: bool,
    ) -> Result<(), CollectionError> {
        self.set_rating(target, hearted.then_some(Stars::Five)).await
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
        self.write_as_self(move |sql, person| {
            let playlist = Playlist {
                id,
                person: person.id,
                name,
                audience: sharing.audience(person.private_circle),
            };
            let id = playlist.id.to_string();
            sql.run(&InsertPlaylistParams {
                id: &id,
                person: playlist.person,
                name: &playlist.name,
                audience: &playlist.audience,
            })?;
            let track_ids = tracks.ordered_leaving_out_missing_picks(sql)?;
            assert!(positions.len() >= track_ids.len(), "a position id for each entry");
            for (position, (position_id, track_id)) in positions.iter().zip(&track_ids).enumerate()
            {
                sql.run(&InsertPlaylistEntryParams {
                    playlist: &id,
                    entry: &position_id.to_string(),
                    track: track_id,
                    position: position as i64,
                })?;
            }
            Ok(Ok(playlist))
        })
        .await
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
        self.change_playlist(id, move |sql, person| {
            sql.run(&SetPlaylistParams {
                id: &id.to_string(),
                name: &name,
                audience: &sharing.audience(person.private_circle),
            })?;
            Ok(Ok(()))
        })
        .await
    }

    pub async fn delete_playlist(&self, id: uuid::Uuid) -> Result<(), CollectionError> {
        self.change_playlist(id, move |sql, _| {
            sql.run(&DeletePlaylistParams { id: &id.to_string() })?;
            Ok(Ok(()))
        })
        .await
    }

    pub async fn playlist_entries(&self, id: uuid::Uuid) -> Result<Vec<PlaylistEntry>, DbError> {
        self.inner.store.read(move |sql| entries(&sql, id)).await
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
        self.change_playlist(playlist, move |sql, _| {
            let track_ids = tracks.ordered_on(sql, bae_records::TrackReading::Named)?;
            assert_eq!(positions.len(), track_ids.len(), "a playlist entry for each track");
            for (position, track) in positions.iter().zip(&track_ids) {
                sql.run(&AppendPlaylistEntryParams {
                    playlist: &playlist.to_string(),
                    entry: &position.to_string(),
                    track,
                })?;
            }
            Ok(Ok(()))
        })
        .await
    }

    pub async fn remove_playlist_entry(
        &self,
        playlist: uuid::Uuid,
        position: uuid::Uuid,
    ) -> Result<(), CollectionError> {
        self.change_playlist(playlist, move |sql, _| {
            let changed = sql.run(&DeletePlaylistEntryParams {
                playlist: &playlist.to_string(),
                entry: &position.to_string(),
            })?;
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
        self.change_playlist(playlist, move |sql, _| {
            let Some(to) = place_among_others_on(sql, playlist, position, place)? else {
                return Ok(Err(CollectionError::Missing));
            };
            sql.run(&MovePlaylistEntryParams {
                playlist: &playlist.to_string(),
                entry: &position.to_string(),
                to,
            })?;
            Ok(Ok(()))
        })
        .await
    }

    /// Adds an entry for each of `tracks` at `place` in the playlist, in the
    /// list's order, with the fresh position ids `positions` (one per track),
    /// and renumbers every entry from 0 in the resulting order, in one write.
    /// Fails as missing when `place` is beside an entry the playlist does not
    /// hold; a track the library lacks fails the write.
    pub async fn insert_playlist_entries(
        &self,
        playlist: uuid::Uuid,
        positions: Vec<uuid::Uuid>,
        tracks: bae_records::TrackList,
        place: InsertPlace,
    ) -> Result<(), CollectionError> {
        self.change_playlist(playlist, move |sql, _| {
            let at = match place {
                InsertPlace::Start => Some(0),
                InsertPlace::Beside(beside) => entries_before_on(sql, playlist, beside, None)?,
            };
            let Some(at) = at else {
                return Ok(Err(CollectionError::Missing));
            };
            let track_ids = tracks.ordered_on(sql, bae_records::TrackReading::Named)?;
            assert_eq!(positions.len(), track_ids.len(), "a playlist entry for each track");
            sql.run(&MakeRoomForPlaylistEntriesParams {
                playlist: &playlist.to_string(),
                at,
                entries: track_ids.len() as i64,
            })?;
            for (offset, (position, track)) in positions.iter().zip(&track_ids).enumerate() {
                sql.run(&InsertPlaylistEntryParams {
                    playlist: &playlist.to_string(),
                    entry: &position.to_string(),
                    track,
                    position: at + offset as i64,
                })?;
            }
            Ok(Ok(()))
        })
        .await
    }

    /// Make `change` to the playlist `id` in one write, which refuses a
    /// playlist the library lacks and one of another person's. `change` is
    /// handed this device's person.
    async fn change_playlist(
        &self,
        id: uuid::Uuid,
        change: impl for<'ctx> FnOnce(
                &CovenWrite<'ctx>,
                PersonProfile,
            ) -> Result<Result<(), CollectionError>, DbError>
            + Send
            + 'static,
    ) -> Result<(), CollectionError> {
        self.write_as_self(move |sql, person| {
            let owner: Option<String> = sql
                .query_row(
                    "SELECT person_id FROM playlists WHERE id = ?",
                    [id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            match owner {
                None => Ok(Err(CollectionError::Missing)),
                Some(owner) if owner != person.id.to_string() => Ok(Err(CollectionError::NotOwner)),
                Some(_) => change(sql, person),
            }
        })
        .await
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

// One playlist's entries, in order; `playlist_entries_position` serves the
// lookup and the order.
sqlive_compose::read!(
    writer EntriesOfPlaylist(playlist: String),
    r#"
    SELECT entry.position_id, entry.track_id
    FROM playlist_entries AS entry
    WHERE entry.playlist_id = :playlist
    ORDER BY entry.position
"#
);

fn entries(sql: &SqlReadContext<'_>, playlist: uuid::Uuid) -> Result<Vec<PlaylistEntry>, DbError> {
    let statement = EntriesOfPlaylistParams { playlist: playlist.to_string() };
    bae_store::statement_rows(sql, &statement)?
        .into_iter()
        .map(|row| {
            let position_id = stored_uuid("a playlist entry's id", &row.position_id)?;
            Ok(PlaylistEntry { position_id, track_id: row.track_id })
        })
        .collect()
}

#[cfg(test)]
#[path = "collections_tests.rs"]
mod tests;

/// The member using this device; `None` until their registration has applied.
/// A removed device keeps its member, as the device keeps writing to its own
/// copy as that person.
const OWN_MEMBER_SQL: &str = "SELECT m.member AS member FROM coven_members AS m WHERE m.is_self";

/// The rating system for albums and the album `?1`'s ratings that it shows.
/// SQLite finds the album's ratings through `ratings_album`, and names
/// other raters by name then id.
pub fn album_ratings_sql() -> String {
    format!(
        "WITH album AS ( \
             SELECT ?1 AS id, albums, \
                    CASE albums WHEN '{stars}' THEN 1 WHEN '{heart}' THEN {minimum_heart} END AS minimum \
             FROM ({RATING_SYSTEMS_SQL}) AS systems \
         ), shown AS MATERIALIZED ( \
             SELECT album.albums, \
                    (SELECT r.stars FROM ratings AS r \
                     WHERE r.album_id = album.id AND r.person_id = own.member \
                       AND r.stars >= album.minimum) AS mine, \
                    (SELECT json_group_array(json_object('name', p.name, 'stars', r.stars) \
                            ORDER BY p.name, p.id) \
                     FROM ratings AS r JOIN people AS p ON p.id = r.person_id \
                     WHERE r.album_id = album.id AND r.person_id IS NOT own.member \
                       AND r.stars >= album.minimum) AS others \
             FROM album LEFT JOIN ({own}) AS own ON 1 \
         ) \
         SELECT *, mine IS NOT NULL OR json_array_length(others) > 0 AS has_raters FROM shown",
        stars = RatingSystem::Stars.as_str(),
        heart = RatingSystem::Heart.as_str(),
        minimum_heart = MIN_HEART_STARS as i64,
        own = OWN_MEMBER_SQL,
    )
}

/// Read the rating system and ratings that [`album_ratings_sql`] selects.
pub fn album_ratings_row(row: &Row<'_>) -> rusqlite::Result<(RatingSystem, AlbumRatings)> {
    let stars_of = crate::collections::stars_from_sql;
    let others: Vec<StoredRating> = bae_store::read::json_column(row, "others")?;
    let ratings = AlbumRatings {
        mine: row.get::<_, Option<i64>>("mine")?.map(stars_of).transpose()?,
        others: others
            .into_iter()
            .map(|other| Ok(OtherRating { name: other.name, stars: stars_of(other.stars)? }))
            .collect::<rusqlite::Result<_>>()?,
        has_raters: row.get("has_raters")?,
    };
    Ok((bae_store::read::parsed_column(row, "albums")?, ratings))
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
    match place {
        EntryPlace::Beside(beside) => entries_before_on(sql, playlist, beside, Some(moved)),
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

/// How many of the playlist's entries, other than `moved`, sit before the
/// place `beside` names; none when the playlist holds no entry `beside`
/// names, `beside` names `moved`, or the playlist holds no `moved`.
fn entries_before_on(
    sql: &CovenWrite<'_>,
    playlist: uuid::Uuid,
    beside: BesideEntry,
    moved: Option<uuid::Uuid>,
) -> Result<Option<i64>, DbError> {
    // An entry just after `beside` follows every entry up to `beside` itself.
    let (entry, compare) = match beside {
        BesideEntry::Before(entry) => (entry, "<"),
        BesideEntry::After(entry) => (entry, "<="),
    };
    Ok(sql
        .query_row(
            &format!(
                "SELECT (SELECT COUNT(*) FROM playlist_entries AS other \
                         WHERE other.playlist_id = ?1 AND other.position_id IS NOT ?2 \
                           AND other.position {compare} beside.position) \
                 FROM playlist_entries AS beside \
                 WHERE beside.playlist_id = ?1 AND beside.position_id = ?3 \
                   AND beside.position_id IS NOT ?2 \
                   AND (?2 IS NULL OR EXISTS (SELECT 1 FROM playlist_entries \
                                              WHERE playlist_id = ?1 AND position_id = ?2))"
            ),
            params![playlist.to_string(), moved.map(|moved| moved.to_string()), entry.to_string()],
            |row| row.get(0),
        )
        .optional()?)
}
