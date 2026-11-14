//! The people of a library with the shape of a real one, and what they made
//! of its albums: three profiles, each rating a tenth of the albums, and 200
//! playlists of the first member, of 10 to 500 entries, and a last one of
//! 10,000. Set-based, as the bulk fixtures of the lists are; every playlist
//! entry names the first track of an album the library holds.

use coven::CovenWrite;

/// How many playlists the first person made, apart from the long one.
pub const PLAYLISTS: u64 = 200;
/// How many entries the long playlist holds.
pub const LONG_PLAYLIST: u64 = 10_000;

/// Person `n`'s id.
pub fn person_id(n: u64) -> String {
    format!("00000000-0005-4000-8000-{n:012x}")
}

/// Playlist `n`'s id; the long playlist is number [`PLAYLISTS`].
pub fn playlist_id(n: u64) -> String {
    format!("00000000-0004-4000-8000-{n:012x}")
}

/// The profiles, and the ratings of albums `start..end` and, when `start`
/// is 0, the playlists. `first_track_id_sql` names album `i`'s first
/// track as an SQL expression over `i`, `album_id_sql` the album.
pub fn insert_on(
    sql: &CovenWrite<'_>,
    start: u64,
    end: u64,
    album_id_sql: &str,
    first_track_id_sql: &str,
) -> Result<(), bae_store::DbError> {
    if start == 0 {
        for n in 0..3 {
            sql.execute(
                "INSERT INTO people (id, name, private_circle, history_sharing) \
                 VALUES (?1, ?2, ?3, 'private')",
                rusqlite::params![person_id(n), format!("Person {n}"), format!("circle-{n}")],
            )?;
        }
        for n in 0..=PLAYLISTS {
            let entries = if n == PLAYLISTS { LONG_PLAYLIST } else { 10 + (n * 37) % 491 };
            sql.execute(
                "INSERT INTO playlists (id, person_id, name, audience) VALUES (?1, ?2, ?3, 'private')",
                rusqlite::params![playlist_id(n), person_id(0), format!("Playlist {n:03}")],
            )?;
            sql.execute_batch(&format!(
                "WITH RECURSIVE j(p) AS (SELECT 0 UNION ALL SELECT p + 1 FROM j WHERE p + 1 < {entries}) \
                 INSERT INTO playlist_entries (playlist_id, position_id, track_id, position) \
                 SELECT '{playlist}', printf('%08x-0002-4000-8000-%012x', {n}, p), \
                        (SELECT {first_track_id_sql} FROM (SELECT p % {end} AS i)), p FROM j;",
                playlist = playlist_id(n),
            ))?;
        }
    }
    if start < end {
        let rows = bae_store::test_fixtures::window_cost::numbered(start, end);
        for n in 0..3 {
            sql.execute_batch(&format!(
                "{rows} INSERT INTO ratings (id, person_id, album_id, stars) \
                 SELECT printf('%08x-0003-4000-8000-%012x', i, {n}), '{person}', {album_id_sql}, 1 + i % 5 \
                 FROM k WHERE (i + {n}) % 10 = 0;",
                person = person_id(n),
            ))?;
        }
    }
    Ok(())
}
