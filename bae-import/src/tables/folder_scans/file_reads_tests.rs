//! What the stored scans of a root say was read of each file, read for the
//! next pass.

use super::*;
use crate::folder_scanner::{
    CandidateFile, CategorizedFiles, FileRole, FolderScanError, FolderSidecar, ReleaseFileScope,
    ScanItem, ScannedFile, SidecarFiles,
};

/// A damaged store whose rows say two things about one version of a file,
/// an image a release holds as damaged and a sidecar as whole, is no scan's
/// reading: reading what the scans read fails as the scan's own error.
#[tokio::test]
async fn two_rows_disagreeing_about_a_file_fail_the_read_as_the_scan() {
    let (db, tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db.clone());
    let root = tmp.path().join("watch");
    let folder = root.join("Album");
    let files = CategorizedFiles {
        files: vec![
            CandidateFile {
                file: ScannedFile::new(folder.join("01.flac"), "01.flac".into(), 123, 1)
                    .with_test_flac_audio(),
                role: FileRole::Audio,
            },
            CandidateFile {
                file: ScannedFile::new(folder.join("cover.jpg"), "cover.jpg".into(), 10, 1),
                role: FileRole::DamagedImage,
            },
        ],
        parts: Vec::new(),
    };
    let candidate =
        crate::tables::tests::candidate_with(&root, "Album", files, ReleaseFileScope::Direct);
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Local).await.unwrap();
    let sidecar = FolderSidecar {
        watched_folder_path: root.clone(),
        files: SidecarFiles::Valid(vec![CandidateFile {
            file: ScannedFile::new(folder.join("back.jpg"), "back.jpg".into(), 10, 1),
            role: FileRole::Artwork,
        }]),
        folder: folder.clone(),
    };
    tables
        .save_folder_sidecar(&root, generation, &sidecar)
        .await
        .unwrap()
        .expect("the current scan takes the sidecar");
    tables
        .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate))
        .await
        .unwrap()
        .expect("the current scan takes the release");
    tables.finish_folder_scan(&root, generation, None).await.unwrap();
    ImportTables::store_sidecar_file_as_for_test(tmp.path(), "back.jpg", "cover.jpg").await;

    let Err(error) = tables
        .load_known_of_root(
            &root,
            Arc::new(crate::file_tag_reader::LoftyFileTagReader),
            coven::FileBudget::for_test(),
        )
        .await
    else {
        panic!("one image read as both damaged and whole is no scan's reading");
    };

    assert!(matches!(error, crate::ImportError::Scan(FolderScanError::Other(_))), "{error:?}");
}

/// The releases a file decision settles, the watched folders' scans, the
/// directories a root's last scan saw and the watched-folder checks are each
/// one statement.
#[tokio::test]
async fn each_read_a_pass_asks_for_is_one_statement() {
    let (db, tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db.clone());
    let root = tmp.path().join("watch");
    let files = crate::tables::tests::track_files_candidate(&[("01.flac", 123)]);
    let content_hash = files.content_hash();
    let candidates = ["First", "Second"].map(|name| {
        crate::tables::tests::candidate_with(&root, name, files.clone(), ReleaseFileScope::Direct)
    });
    tables.add_watched_import_folder(&root).await.unwrap();
    let generation = tables.begin_folder_scan(&root, crate::VolumeKind::Network).await.unwrap();
    for candidate in &candidates {
        tables
            .save_folder_scan_item(&root, generation, &ScanItem::Valid(candidate.clone()))
            .await
            .unwrap()
            .expect("the current scan takes the release");
    }
    let directories = vec![(root.clone(), 1)];
    tables.record_folder_scan_directories(&root, &directories).await.unwrap();
    let counter = db.track_work_for_test().await.unwrap();

    let releases = tables.load_releases_reading(&content_hash, 0).await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1, "the releases reading the files");
    assert_eq!(releases.len(), 2);

    let scans = tables.load_stored_folder_scans().await.unwrap();
    assert_eq!(counter.take().app_read_statements, 1, "the watched folders' scans");
    assert!(scans[0].last_scan.as_ref().is_some_and(|scan| scan.on_network_volume));

    assert_eq!(tables.load_folder_scan_directories(&root).await.unwrap(), directories);
    assert_eq!(counter.take().app_read_statements, 1, "the directories the last scan saw");

    assert!(tables.is_watched_import_folder(&root).await.unwrap());
    assert_eq!(counter.take().app_read_statements, 1, "whether a folder is watched");

    assert_eq!(tables.watched_import_folders_below(tmp.path()).await.unwrap(), [root]);
    assert_eq!(counter.take().app_read_statements, 1, "the watched folders below a folder");
}
