#![cfg(feature = "test-utils")]
//! The device-local `playback_state` table: save, load, replace, clear.

use bae_core::db::{Database, DbPlaybackContext, DbPlaybackState, LoadedPlaybackState};
use tempfile::TempDir;

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
    )
    .await
    .expect("Failed to create database");
    (database, temp_dir)
}

#[tokio::test]
async fn playback_state_saves_loads_replaces_and_clears() {
    let (db, _tmp) = setup_db().await;

    // Nothing stored to start.
    assert!(matches!(
        db.load_playback_state().await.unwrap(),
        LoadedPlaybackState::Absent
    ));

    // A shuffled context: the flag rides the nullable INTEGER column alongside
    // the source, and reads back as a bool.
    let row = DbPlaybackState {
        context: Some(DbPlaybackContext {
            source: "rel-1".to_string(),
            shuffled: true,
        }),
        manual: r#"["t1","t2"]"#.to_string(),
        repeat: "context".to_string(),
        current_track_id: Some("t5".to_string()),
        position_ms: Some(42_000),
        volume: 0.5,
        is_muted: false,
    };
    db.save_playback_state(&row).await.unwrap();

    let LoadedPlaybackState::Present(loaded) = db.load_playback_state().await.unwrap() else {
        panic!("a row");
    };
    let context = loaded.context.expect("a context");
    assert_eq!(context.source, "rel-1");
    assert!(context.shuffled);
    assert_eq!(loaded.manual, r#"["t1","t2"]"#);
    assert_eq!(loaded.repeat, "context");
    assert_eq!(loaded.current_track_id.as_deref(), Some("t5"));
    assert_eq!(loaded.position_ms, Some(42_000));
    assert!((loaded.volume - 0.5).abs() < f32::EPSILON);
    assert!(!loaded.is_muted);

    // Saving again replaces the single row (id = 'current') rather than appending
    // — a no-context single track with nulls where the context columns were.
    let single = DbPlaybackState {
        context: None,
        manual: "[]".to_string(),
        repeat: "off".to_string(),
        current_track_id: Some("solo".to_string()),
        position_ms: None,
        volume: 1.0,
        is_muted: true,
    };
    db.save_playback_state(&single).await.unwrap();
    let LoadedPlaybackState::Present(loaded) = db.load_playback_state().await.unwrap() else {
        panic!("a row");
    };
    assert_eq!(loaded.context, None);
    assert_eq!(loaded.current_track_id.as_deref(), Some("solo"));
    assert!(loaded.is_muted);

    // Clear removes it.
    db.clear_playback_state().await.unwrap();
    assert!(matches!(
        db.load_playback_state().await.unwrap(),
        LoadedPlaybackState::Absent
    ));
}
