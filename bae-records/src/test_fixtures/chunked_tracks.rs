//! A release with more tracks than one SQLite statement binds.

use bae_store::query::SQL_MAX_IN_VARS;

pub async fn chunked_track_db(
    schema: bae_store::Schema,
) -> (bae_store::Store, tempfile::TempDir, Vec<String>) {
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    let track_ids = track_ids(SQL_MAX_IN_VARS * 45);
    let seed_track_ids = track_ids.clone();
    db.write(move |sql| seed_release(&sql, &seed_track_ids)).await.unwrap();
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
) -> Result<(), crate::IdentityWriteError> {
    let titles: Vec<String> =
        (0..track_ids.len()).map(|index| format!("Track Title {index}")).collect();
    let tracks: Vec<(&str, &str)> =
        track_ids.iter().map(String::as_str).zip(titles.iter().map(String::as_str)).collect();
    super::artist_album_release().tracks(super::RELEASE_A, &tracks).write_on(conn)
}
