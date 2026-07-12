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

/// Test the complete import lifecycle: importing -> complete.
/// This is the happy path when everything works.
#[tokio::test]
async fn test_import_lifecycle_success() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // Create import record (starts at Importing since all validation already passed)
    let import = DbImport::new(
        "lifecycle-import",
        "Album Title",
        "Artist Name",
        "/music/library/album-title",
        chrono::Utc::now(),
    );
    db.insert_import(&import).await.unwrap();

    // Should appear in active imports
    let active = db.get_active_imports().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].status, ImportOperationStatus::Importing);

    // Import completes successfully
    db.update_import_status("lifecycle-import", ImportOperationStatus::Complete)
        .await
        .unwrap();

    // Should no longer appear in active imports
    let active = db.get_active_imports().await.unwrap();
    assert!(active.is_empty());

    // But can still be retrieved directly
    let completed = db
        .find_import_by_id("lifecycle-import")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, ImportOperationStatus::Complete);
}

/// Test that failed imports are removed from active imports.
#[tokio::test]
async fn test_import_lifecycle_failure() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    let import = DbImport::new(
        "failing-import",
        "Test Album",
        "Test Artist",
        "/path/to/album",
        chrono::Utc::now(),
    );
    db.insert_import(&import).await.unwrap();

    // Simulate failure during import
    db.update_import_error("failing-import", "Network connection lost")
        .await
        .unwrap();

    // Should not appear in active imports
    let active = db.get_active_imports().await.unwrap();
    assert!(active.is_empty());

    // Verify error was recorded
    let failed = db
        .find_import_by_id("failing-import")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, ImportOperationStatus::Failed);
    assert_eq!(
        failed.error_message,
        Some("Network connection lost".to_string())
    );
}

/// Test that get_active_imports excludes complete and failed imports.
#[tokio::test]
async fn test_get_active_imports_excludes_complete_and_failed() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    let import1 = DbImport::new(
        "import-active-1",
        "Album 1",
        "Artist 1",
        "/path/1",
        chrono::Utc::now(),
    );
    db.insert_import(&import1).await.unwrap();

    let import2 = DbImport::new(
        "import-active-2",
        "Album 2",
        "Artist 2",
        "/path/2",
        chrono::Utc::now(),
    );
    db.insert_import(&import2).await.unwrap();

    // Create one complete (should NOT appear)
    let import3 = DbImport::new(
        "import-complete",
        "Album 3",
        "Artist 3",
        "/path/3",
        chrono::Utc::now(),
    );
    db.insert_import(&import3).await.unwrap();
    db.update_import_status("import-complete", ImportOperationStatus::Complete)
        .await
        .unwrap();

    // Create one failed (should NOT appear)
    let import4 = DbImport::new(
        "import-failed",
        "Album 4",
        "Artist 4",
        "/path/4",
        chrono::Utc::now(),
    );
    db.insert_import(&import4).await.unwrap();
    db.update_import_error("import-failed", "Some error")
        .await
        .unwrap();

    let active = db.get_active_imports().await.unwrap();
    assert_eq!(active.len(), 2);

    let ids: Vec<&str> = active.iter().map(|i| i.id.as_str()).collect();
    assert!(ids.contains(&"import-active-1"));
    assert!(ids.contains(&"import-active-2"));
}

/// Test that deleting an import removes it from the database.
/// This is needed for the UI dismiss functionality to properly clean up
/// stuck imports so they don't reappear after app restart.
#[tokio::test]
async fn test_delete_import_removes_from_database() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    let import = DbImport::new(
        "to-delete",
        "Test Album",
        "Test Artist",
        "/path/to/album",
        chrono::Utc::now(),
    );
    db.insert_import(&import).await.unwrap();

    // Verify it exists
    assert!(db.find_import_by_id("to-delete").await.unwrap().is_some());
    assert_eq!(db.get_active_imports().await.unwrap().len(), 1);

    // Delete it (this is what UI dismiss should do)
    db.delete_import("to-delete").await.unwrap();

    // Should be completely gone
    assert!(db.find_import_by_id("to-delete").await.unwrap().is_none());
    assert!(db.get_active_imports().await.unwrap().is_empty());
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

/// Each import row is tracked independently through status changes: three
/// rows start active, then completing / failing / deleting one each drops it
/// from `get_active_imports`. Exercises the DB row bookkeeping, not concurrency
/// (the "imports" all run on one thread, in sequence).
#[tokio::test]
async fn active_imports_reflect_per_row_status_transitions() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // Start three imports
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

    // All three should be active
    assert_eq!(db.get_active_imports().await.unwrap().len(), 3);

    // Complete one
    db.update_import_status("concurrent-1", ImportOperationStatus::Complete)
        .await
        .unwrap();
    assert_eq!(db.get_active_imports().await.unwrap().len(), 2);

    // Fail one
    db.update_import_error("concurrent-2", "Failed")
        .await
        .unwrap();
    assert_eq!(db.get_active_imports().await.unwrap().len(), 1);

    // Delete one
    db.delete_import("concurrent-3").await.unwrap();
    assert!(db.get_active_imports().await.unwrap().is_empty());
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
