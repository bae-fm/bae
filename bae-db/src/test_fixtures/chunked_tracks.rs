//! A release with more tracks than one SQLite statement binds.

use bae_store::query::SQL_MAX_IN_VARS;
use rusqlite::params;

pub async fn chunked_track_db(
    schema: bae_store::Schema,
) -> (bae_store::Store, tempfile::TempDir, Vec<String>) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    let track_ids = track_ids(SQL_MAX_IN_VARS * 45);
    let seed_track_ids = track_ids.clone();
    db.call(move |conn| seed_release(conn, &seed_track_ids)).await.unwrap();
    (db, tmp, track_ids)
}

/// `count` track ids, the same on every call.
pub fn track_ids(count: usize) -> Vec<String> {
    (0..count).map(|index| bae_util::test_ids::test_uuid(&format!("track-{index}"))).collect()
}

/// Write the fixture artist, album and release, its one medium, and a track
/// on it for each of `track_ids`, in order.
pub fn seed_release(
    conn: &coven::CovenWrite<'_>,
    track_ids: &[String],
) -> Result<(), bae_store::DbError> {
    super::seed_artist_album_release(conn)?;
    // The ids are minted in Rust, not in SQL: coven takes only canonical
    // UUIDs on a synced row, and the test's assertions need the same values
    // the seed wrote.
    let medium_id = crate::identity::release_medium_id("0252dedb-ee39-4547-8803-438dbeb57a64", 1);
    conn.execute(
        "INSERT INTO release_mediums (id, release_id, position, medium, side_count, created_at) \
         VALUES (?1, '0252dedb-ee39-4547-8803-438dbeb57a64', 1, 'cd', 1, '2026-01-01T00:00:00Z')",
        params![medium_id],
    )?;
    for (index, track_id) in track_ids.iter().enumerate() {
        conn.execute(
            "INSERT INTO tracks \
                 (position, id, release_id, title, medium_id, side, track_number, duration_ms, created_at) \
             VALUES (?3, ?1, '0252dedb-ee39-4547-8803-438dbeb57a64', ?2, ?4, 1, ?3 + 1, 1000, '2026-01-01T00:00:00Z')",
            params![track_id, format!("Track Title {index}"), index as i64, medium_id],
        )?;
    }
    Ok(())
}
