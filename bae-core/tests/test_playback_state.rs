#![cfg(feature = "test-utils")]
//! The device-local `playback_state` table: save, load, replace, clear.

use bae_core::db::{Database, DbPlaybackState};
use tempfile::TempDir;

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(bae_core::clock::SystemClock),
    )
    .await
    .expect("Failed to create database");
    (database, temp_dir)
}

#[tokio::test]
async fn playback_state_saves_loads_replaces_and_clears() {
    let (db, _tmp) = setup_db().await;

    // Nothing stored to start.
    assert!(db.load_playback_state().await.unwrap().is_none());

    // A shuffled context whose seed has the high bit set: it must survive the
    // SQLite i64 column round-trip.
    let seed: u64 = 0xFFFF_FFFF_FFFF_FFFF;
    let row = DbPlaybackState {
        source: Some("rel-1".to_string()),
        shuffle_seed: Some(seed as i64),
        cursor: Some(3),
        manual: r#"["t1","t2"]"#.to_string(),
        repeat: "context".to_string(),
        current_track_id: Some("t5".to_string()),
        position_ms: Some(42_000),
        volume: 0.5,
        is_muted: false,
    };
    db.save_playback_state(&row).await.unwrap();

    let loaded = db.load_playback_state().await.unwrap().expect("a row");
    assert_eq!(loaded.source.as_deref(), Some("rel-1"));
    assert_eq!(loaded.shuffle_seed.map(|s| s as u64), Some(seed));
    assert_eq!(loaded.cursor, Some(3));
    assert_eq!(loaded.manual, r#"["t1","t2"]"#);
    assert_eq!(loaded.repeat, "context");
    assert_eq!(loaded.current_track_id.as_deref(), Some("t5"));
    assert_eq!(loaded.position_ms, Some(42_000));
    assert!((loaded.volume - 0.5).abs() < f32::EPSILON);
    assert!(!loaded.is_muted);

    // Saving again replaces the single row (id = 'current') rather than appending
    // — a no-context single track with nulls where the context fields were.
    let single = DbPlaybackState {
        source: None,
        shuffle_seed: None,
        cursor: None,
        manual: "[]".to_string(),
        repeat: "off".to_string(),
        current_track_id: Some("solo".to_string()),
        position_ms: None,
        volume: 1.0,
        is_muted: true,
    };
    db.save_playback_state(&single).await.unwrap();
    let loaded = db.load_playback_state().await.unwrap().expect("a row");
    assert_eq!(loaded.source, None);
    assert_eq!(loaded.shuffle_seed, None);
    assert_eq!(loaded.current_track_id.as_deref(), Some("solo"));
    assert!(loaded.is_muted);

    // Clear removes it.
    db.clear_playback_state().await.unwrap();
    assert!(db.load_playback_state().await.unwrap().is_none());
}
