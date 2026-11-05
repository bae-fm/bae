//! The candidates set aside as the store keeps them.

use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;

/// A skipped folder's key below the root that is not canonical — a doubled
/// separator — is refused when set aside, and fails the read of the root's
/// skipped candidates when stored anyway. It is spelled with the host's
/// separator, which the read takes the range below the root by.
#[tokio::test]
async fn corrupt_skipped_keys_fail_when_loaded() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/mounted/library");
    tables.add_watched_import_folder(&root).await.unwrap();
    let sep = std::path::MAIN_SEPARATOR;
    let doubled = format!("{}{sep}a{sep}{sep}b", root.display());
    assert!(tables
        .set_import_candidate_skipped(std::path::Path::new(&doubled), true)
        .await
        .is_err());
    db.call(move |conn| {
        conn.execute("INSERT INTO skipped_import_candidates VALUES (?)", [doubled.into_bytes()])?;
        Ok(())
    })
    .await
    .unwrap();
    assert!(tables.load_skipped_import_candidates(&root).await.is_err());
}
