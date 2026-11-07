//! How many reads of the store storing a batch of scan items makes: the same
//! few for one item as for many.

use super::*;
use crate::scan_passes::tests::*;
use crate::service::tests::*;

/// Store `items` again under a new generation of `root`, and count the reads
/// of the store it made.
async fn reads_to_store(
    test: &TestService,
    root: &Path,
    items: Vec<ScanItem>,
) -> Result<u64, crate::ImportError> {
    let (scan, _events) = test.scan();
    let services = &scan.services.services;
    let generation = services.tables.begin_folder_scan(root, crate::VolumeKind::Local).await?;
    let reads = Arc::new(crate::folder_scanner::FileReads::new(Arc::new(
        crate::file_tag_reader::LoftyFileTagReader,
    )));
    let before = test.db.reads_asked();
    let stored = store_scan_items(
        root,
        generation,
        items.into_iter().map(FolderEntry::Release).collect(),
        &reads,
        services,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .await?;
    assert!(stored, "the new generation is the root's");
    Ok(test.db.reads_asked() - before)
}

/// Each kind of read a batch needs — the file decisions, what the store holds
/// of the items' files, and the recheck of the decisions' revisions — is made
/// once for the batch, not once for each item.
#[tokio::test]
async fn a_batch_of_scan_items_reads_the_store_as_often_as_one_item() {
    let test = setup_import_service().await;
    let root = test.temp.path().join("watched");
    for albums in 1..=8 {
        write_disc(&root.join(format!("Album {albums}")), albums);
    }
    let root = crate::tables::watched_path::canonical_absolute_root(&root).unwrap();
    test.tables().add_watched_import_folder(&root).await.unwrap();
    let (scan, _events) = test.scan();
    scan.rescan(&root).await.unwrap();
    let items = test.tables().load_folder_scan_items(&root).await.unwrap();
    assert_eq!(items.len(), 8, "every album was scanned");

    let one = reads_to_store(&test, &root, items[..1].to_vec()).await.unwrap();
    let eight = reads_to_store(&test, &root, items).await.unwrap();

    assert_eq!(eight, one, "a batch of eight reads the store as often as one item");
    assert_eq!(eight, 3, "the decisions, what the files seed, and the revisions' recheck");
}
