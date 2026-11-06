//! A library of as many albums as the lists' window-cost measurements read,
//! each with an artist, one release, one medium and one track, and a work
//! the artist composed. No write makes a hundred thousand albums within a
//! run, so the rows go in by one set-based statement per table.

use bae_store::test_fixtures::window_cost::numbered;
use bae_store::DbError;
use coven::CovenWrite;

/// A library row's id: a UUID naming its table by `kind` and its row by `i`,
/// as an SQL expression over `i`.
fn row_id_sql(kind: u32) -> String {
    format!("printf('%08x-{kind:04x}-4000-8000-%012x', i, i)")
}

/// Release `i`'s id, as an SQL expression over `i`.
pub fn release_id_sql() -> String {
    row_id_sql(3)
}

/// Track `i`'s id, as an SQL expression over `i`.
pub fn track_id_sql() -> String {
    row_id_sql(6)
}

/// Release `i`'s id.
pub fn release_id(i: u64) -> String {
    format!("{i:08x}-0003-4000-8000-{i:012x}")
}

/// File `j` of release `i`'s id.
pub fn file_id(i: u64, j: u64) -> String {
    format!("{i:08x}-0007-4000-8000-{j:012x}")
}

/// When row `i` was written, as an SQL expression over `i`, and as a time.
const CREATED_SQL: &str =
    "strftime('%Y-%m-%dT%H:%M:%SZ', 1600000000 + (i * 7919) % 50000000, 'unixepoch')";

/// When row `i` was written.
pub fn created_at(i: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(
        1_600_000_000 + i64::try_from((i * 7919) % 50_000_000).unwrap(),
        0,
    )
    .unwrap()
}

/// Rows `start..end`: an artist, an album crediting it with one release, one
/// medium and one track, and a work the artist composed. Titles and names run
/// in an order of their own, unrelated to the ids', and every sort has ties
/// the ids break.
pub fn insert_on(sql: &CovenWrite<'_>, start: u64, end: u64) -> Result<(), DbError> {
    let rows = numbered(start, end);
    let (artist, album, release, credit, medium, track, work, work_artist) = (
        row_id_sql(1),
        row_id_sql(2),
        release_id_sql(),
        row_id_sql(4),
        row_id_sql(5),
        track_id_sql(),
        row_id_sql(8),
        row_id_sql(9),
    );
    let created = CREATED_SQL;
    sql.execute_batch(&format!(
        "{rows} INSERT INTO artists (id, name, name_key, created_at) \
             SELECT {artist}, printf('Artist %06d', (i * 48271) % 1000003), \
                    printf('artist %06d', (i * 48271) % 1000003), {created} FROM k;
         {rows} INSERT INTO albums (id, title, year, created_at) \
             SELECT {album}, printf('Album %05d', (i * 16807) % 99991), \
                    CASE WHEN i % 17 = 0 THEN NULL ELSE 1950 + (i * 31) % 70 END, {created} FROM k;
         {rows} INSERT INTO album_artists (id, album_id, artist_id, position, created_at) \
             SELECT {credit}, {album}, {artist}, 0, {created} FROM k;
         {rows} INSERT INTO releases (id, album_id, album_order, created_at) \
             SELECT {release}, {album}, 0, {created} FROM k;
         {rows} INSERT INTO release_mediums (id, release_id, position, side_count, created_at) \
             SELECT {medium}, {release}, 1, 1, {created} FROM k;
         {rows} INSERT INTO tracks (id, release_id, title, medium_id, side, track_number, \
                                    duration_ms, created_at) \
             SELECT {track}, {release}, printf('Track %06d', i), {medium}, 1, 1, 180000, \
                    {created} FROM k;
         {rows} INSERT INTO works (id, title, musicbrainz_work_id, created_at) \
             SELECT {work}, printf('Work %06d', i), {work}, {created} FROM k;
         {rows} INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) \
             SELECT {work_artist}, {work}, {artist}, 0, 'musicbrainz', {created} FROM k;"
    ))?;
    Ok(())
}
