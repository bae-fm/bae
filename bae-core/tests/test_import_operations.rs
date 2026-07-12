#![cfg(feature = "test-utils")]
//! Tests for import operations tracking
//!
//! This suite exercises the DbImport facility that tracks import operations
//! from when the user clicks "Import" through completion or failure.
//!
//! The imports table provides:
//! - A stable ID for progress subscriptions before release exists
//! - Status tracking (importing -> complete/failed)
//! - Display info (album title, artist) during the prepare phase
//! - Link to release_id after phase 0 completes
//!
//! Key scenarios tested:
//! - Normal import lifecycle (importing -> complete)
//! - Failed imports (error handling)
//! - Stuck imports (importing with no release_id)
//! - Clearing/dismissing imports from the UI
//! - App restart loading active imports from DB

use bae_core::db::{Database, DbImport, ImportOperationStatus};
use tempfile::TempDir;

fn tracing_init() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_line_number(true)
        .with_target(false)
        .with_file(true)
        .try_init();
}

async fn create_test_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .unwrap();
    (database, temp_dir)
}

/// Test creating an import record and retrieving it.
#[tokio::test]
async fn test_insert_and_get_import() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    let import = DbImport::new(
        "test-import-1",
        "Album Title",
        "Artist Name",
        "/music/library/album-title",
        chrono::Utc::now(),
    );

    db.insert_import(&import).await.unwrap();

    let retrieved = db.find_import_by_id("test-import-1").await.unwrap();
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, "test-import-1");
    assert_eq!(retrieved.album_title, "Album Title");
    assert_eq!(retrieved.artist_name, "Artist Name");
    assert_eq!(retrieved.folder_path, "/music/library/album-title");
    assert_eq!(retrieved.status, ImportOperationStatus::Importing);
    assert!(retrieved.release_id.is_none());
    assert!(retrieved.error_message.is_none());
}

/// Test that deleting a non-existent import doesn't error.
#[tokio::test]
async fn test_delete_nonexistent_import_is_ok() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // Should not error when deleting something that doesn't exist
    let result = db.delete_import("nonexistent-id").await;
    assert!(result.is_ok());
}

/// Each import row is tracked independently through its status changes: three
/// rows start active, then completing / failing / deleting one each drops it
/// from `get_active_imports` — checked by id, not just count — and each
/// transition's durable effect is verified directly: a completed row persists as
/// Complete, a failed row persists as Failed carrying its recorded error message,
/// a deleted row is gone from the table entirely (not merely filtered out of
/// active). Exercises the DB row bookkeeping, not concurrency (the rows all run
/// on one thread, in sequence).
#[tokio::test]
async fn active_imports_reflect_per_row_status_transitions() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // Start three imports.
    for i in 1..=3 {
        let import = DbImport::new(
            &format!("concurrent-{}", i),
            &format!("Album {}", i),
            &format!("Artist {}", i),
            &format!("/path/{}", i),
            chrono::Utc::now(),
        );
        db.insert_import(&import).await.unwrap();
    }
    assert_eq!(db.get_active_imports().await.unwrap().len(), 3);

    // Complete one: it drops from active by id, and the row persists as Complete.
    db.update_import_status("concurrent-1", ImportOperationStatus::Complete)
        .await
        .unwrap();
    let active = db.get_active_imports().await.unwrap();
    let active_ids: Vec<&str> = active.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(active.len(), 2);
    assert!(
        active_ids.contains(&"concurrent-2") && active_ids.contains(&"concurrent-3"),
        "the two still-importing rows stay active"
    );
    assert!(
        !active_ids.contains(&"concurrent-1"),
        "the completed row is excluded by id, not just by count"
    );
    assert_eq!(
        db.find_import_by_id("concurrent-1")
            .await
            .unwrap()
            .unwrap()
            .status,
        ImportOperationStatus::Complete,
        "the completed row persists with Complete status"
    );

    // Fail one: it drops from active, and the row persists as Failed with its
    // recorded error message.
    db.update_import_error("concurrent-2", "Network connection lost")
        .await
        .unwrap();
    assert_eq!(db.get_active_imports().await.unwrap().len(), 1);
    let failed = db.find_import_by_id("concurrent-2").await.unwrap().unwrap();
    assert_eq!(failed.status, ImportOperationStatus::Failed);
    assert_eq!(
        failed.error_message,
        Some("Network connection lost".to_string())
    );

    // Delete one: gone from active AND removed from the table.
    db.delete_import("concurrent-3").await.unwrap();
    assert!(db.get_active_imports().await.unwrap().is_empty());
    assert!(
        db.find_import_by_id("concurrent-3")
            .await
            .unwrap()
            .is_none(),
        "a deleted row is removed from the table, not just excluded from active"
    );
}

/// Test that active imports are ordered by created_at DESC (newest first).
#[tokio::test]
async fn test_active_imports_ordered_by_created_at_desc() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // `created_at` is stored at second precision, so the rows must carry
    // distinct whole-second timestamps for the DESC order to be well-defined.
    // Set them explicitly instead of sleeping between inserts.
    let base = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let import1 = DbImport::new("first", "First Album", "Artist", "/path/1", base);
    db.insert_import(&import1).await.unwrap();

    let import2 = DbImport::new(
        "second",
        "Second Album",
        "Artist",
        "/path/2",
        base + chrono::Duration::seconds(1),
    );
    db.insert_import(&import2).await.unwrap();

    let import3 = DbImport::new(
        "third",
        "Third Album",
        "Artist",
        "/path/3",
        base + chrono::Duration::seconds(2),
    );
    db.insert_import(&import3).await.unwrap();

    let active = db.get_active_imports().await.unwrap();
    assert_eq!(active.len(), 3);

    // Newest should be first
    assert_eq!(active[0].id, "third");
    assert_eq!(active[1].id, "second");
    assert_eq!(active[2].id, "first");
}
