//! Tests for import operations tracking
//!
//! This suite exercises the DbImport facility that tracks import operations
//! from when the user clicks "Import" through completion or failure.
//!
//! The imports table provides:
//! - A stable ID for progress subscriptions before release exists
//! - Status tracking (preparing -> importing -> complete/failed)
//! - Display info (album title, artist) during the prepare phase
//! - Link to release_id after phase 0 completes
//!
//! Key scenarios tested:
//! - Normal import lifecycle (preparing -> importing -> complete)
//! - Failed imports (error handling)
//! - Stuck imports (preparing with no release_id)
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
    let database = Database::new(db_path.to_str().unwrap()).await.unwrap();
    (database, temp_dir)
}

/// Test creating an import record and retrieving it.
#[tokio::test]
async fn test_insert_and_get_import() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    let import = DbImport::new(
        "test-import-1",
        "Neon Dreams",
        "The Wanderers",
        "/music/wanderers/neon-dreams",
    );

    db.insert_import(&import).await.unwrap();

    let retrieved = db.get_import("test-import-1").await.unwrap();
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, "test-import-1");
    assert_eq!(retrieved.album_title, "Neon Dreams");
    assert_eq!(retrieved.artist_name, "The Wanderers");
    assert_eq!(retrieved.folder_path, "/music/wanderers/neon-dreams");
    assert_eq!(retrieved.status, ImportOperationStatus::Preparing);
    assert!(retrieved.release_id.is_none());
    assert!(retrieved.error_message.is_none());
}

/// Test the complete import lifecycle: preparing -> importing -> complete.
/// This is the happy path when everything works.
#[tokio::test]
async fn test_import_lifecycle_success() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // User clicks Import - create import record in preparing state
    let import = DbImport::new(
        "lifecycle-import",
        "Midnight Echo",
        "Solar Flare",
        "/music/solar-flare/midnight-echo",
    );
    db.insert_import(&import).await.unwrap();

    // Should appear in active imports
    let active = db.get_active_imports().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].status, ImportOperationStatus::Preparing);

    // Transition to importing state (in real code, this happens after linking to release)
    db.update_import_status("lifecycle-import", ImportOperationStatus::Importing)
        .await
        .unwrap();

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
    let completed = db.get_import("lifecycle-import").await.unwrap().unwrap();
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
    let failed = db.get_import("failing-import").await.unwrap().unwrap();
    assert_eq!(failed.status, ImportOperationStatus::Failed);
    assert_eq!(
        failed.error_message,
        Some("Network connection lost".to_string())
    );
}

/// Test that get_active_imports returns both preparing and importing status.
#[tokio::test]
async fn test_get_active_imports_includes_preparing_and_importing() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // Create one in preparing state
    let import1 = DbImport::new("import-preparing", "Album 1", "Artist 1", "/path/1");
    db.insert_import(&import1).await.unwrap();

    // Create one in importing state
    let import2 = DbImport::new("import-importing", "Album 2", "Artist 2", "/path/2");
    db.insert_import(&import2).await.unwrap();
    db.update_import_status("import-importing", ImportOperationStatus::Importing)
        .await
        .unwrap();

    // Create one complete (should NOT appear)
    let import3 = DbImport::new("import-complete", "Album 3", "Artist 3", "/path/3");
    db.insert_import(&import3).await.unwrap();
    db.update_import_status("import-complete", ImportOperationStatus::Complete)
        .await
        .unwrap();

    // Create one failed (should NOT appear)
    let import4 = DbImport::new("import-failed", "Album 4", "Artist 4", "/path/4");
    db.insert_import(&import4).await.unwrap();
    db.update_import_error("import-failed", "Some error")
        .await
        .unwrap();

    let active = db.get_active_imports().await.unwrap();
    assert_eq!(active.len(), 2);

    let ids: Vec<&str> = active.iter().map(|i| i.id.as_str()).collect();
    assert!(ids.contains(&"import-preparing"));
    assert!(ids.contains(&"import-importing"));
}

/// Test the bug scenario: a stuck "preparing" import that failed before
/// creating a release. This import has no release_id and stays in "preparing"
/// state forever, reappearing after app restart.
#[tokio::test]
async fn test_stuck_preparing_import_scenario() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // Simulate: user clicks Import, import record is created...
    let import = DbImport::new(
        "stuck-import",
        "Crimson Horizon",
        "Velvet Thunder",
        "/Torrents/Velvet Thunder - Crimson Horizon",
    );
    db.insert_import(&import).await.unwrap();

    // ...but then HTTP request fails before phase 0 completes
    // The import is stuck in "preparing" with no release_id

    // Simulate app restart: get_active_imports should return the stuck import
    let active = db.get_active_imports().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "stuck-import");
    assert_eq!(active[0].album_title, "Crimson Horizon");
    assert_eq!(active[0].status, ImportOperationStatus::Preparing);
    assert!(
        active[0].release_id.is_none(),
        "Stuck import should have no release_id"
    );
}

/// Test that deleting an import removes it from the database.
/// This is needed for the UI dismiss functionality to properly clean up
/// stuck imports so they don't reappear after app restart.
#[tokio::test]
async fn test_delete_import_removes_from_database() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    let import = DbImport::new("to-delete", "Test Album", "Test Artist", "/path/to/album");
    db.insert_import(&import).await.unwrap();

    // Verify it exists
    assert!(db.get_import("to-delete").await.unwrap().is_some());
    assert_eq!(db.get_active_imports().await.unwrap().len(), 1);

    // Delete it (this is what UI dismiss should do)
    db.delete_import("to-delete").await.unwrap();

    // Should be completely gone
    assert!(db.get_import("to-delete").await.unwrap().is_none());
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

/// Test multiple concurrent imports are tracked independently.
#[tokio::test]
async fn test_multiple_concurrent_imports() {
    tracing_init();
    let (db, _temp) = create_test_db().await;

    // Start three imports
    for i in 1..=3 {
        let import = DbImport::new(
            &format!("concurrent-{}", i),
            &format!("Album {}", i),
            &format!("Artist {}", i),
            &format!("/path/{}", i),
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

    // Insert with 1 second delays to ensure different timestamps
    // (SQLite stores timestamps at second precision)
    let import1 = DbImport::new("first", "First Album", "Artist", "/path/1");
    db.insert_import(&import1).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let import2 = DbImport::new("second", "Second Album", "Artist", "/path/2");
    db.insert_import(&import2).await.unwrap();

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let import3 = DbImport::new("third", "Third Album", "Artist", "/path/3");
    db.insert_import(&import3).await.unwrap();

    let active = db.get_active_imports().await.unwrap();
    assert_eq!(active.len(), 3);

    // Newest should be first
    assert_eq!(active[0].id, "third");
    assert_eq!(active[1].id, "second");
    assert_eq!(active[2].id, "first");
}
