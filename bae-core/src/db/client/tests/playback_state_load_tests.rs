use super::super::*;
use coven::SystemClock;

async fn empty_db() -> (Database, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("test.db");
    let db = Database::new_test(
        path.to_str().unwrap(),
        Arc::new(SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .unwrap();
    (db, tmp)
}

/// `source` and `shuffled` are written together, so a row carrying one
/// without the other is corrupt: `load_playback_state` reports `Corrupt`
/// rather than inventing a flag or masking it as an absent cache.
#[tokio::test]
async fn mismatched_source_and_shuffled_discards_the_cache() {
    let (db, _tmp) = empty_db().await;

    // Write a row by hand with a present source but a NULL shuffled --
    // `save_playback_state` never produces this, so we insert it directly.
    db.call(|conn| {
        conn.execute(
            "INSERT INTO playback_state \
                 (id, source, shuffled, manual, repeat, \
                  current_track_id, position_ms, volume, is_muted) \
                 VALUES ('current', 'cccb6034-5922-40d2-8d0b-d94619230882', NULL, '[]', 'off', \
                  NULL, NULL, 1.0, 0)",
            [],
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    assert!(matches!(
        db.load_playback_state().await.unwrap(),
        LoadedPlaybackState::Corrupt
    ));
}
