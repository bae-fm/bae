//! A folder reading's commit: what it stores and what it prunes.

use crate::folder_scanner::ScanItem;
use crate::tables::release_groupings::tests::{scan_in, scanned, sidecar_of};
use bae_db::watched_path::host_root;

/// Reading one folder again prunes the sidecars it no longer yields at and
/// below that folder, and leaves those of the folders beside it, whatever
/// their names sort next to.
#[tokio::test]
async fn reading_a_folder_again_prunes_only_its_own_sidecars() {
    let (db, _temp, _) = scanned(&[]).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let root = host_root("/music");
    for folder in ["Album", "Album/Disc 1", "Album 2", "Album0"] {
        scan_in(&db, ScanItem::Sidecar(sidecar_of(folder, &["cover.jpg"]))).await;
    }

    let stamp = tables.begin_folder_reading(&root).await.unwrap();
    tables
        .commit_folder_reading(crate::tables::FolderReadingCommit {
            watched_folder_path: root.clone(),
            folder: "Album".into(),
            stamp,
            decision: None,
            scanned_decisions: Vec::new(),
            items: Vec::new(),
            directories: Some(Vec::new()),
        })
        .await
        .unwrap();

    let folder = |name: &str| coven::path_text(&root.join(name));
    assert_eq!(
        db.query_texts_for_test("SELECT coven_path_text(folder) FROM scan_sidecar ORDER BY folder")
            .await
            .unwrap(),
        [folder("Album 2"), folder("Album0")]
    );
}
