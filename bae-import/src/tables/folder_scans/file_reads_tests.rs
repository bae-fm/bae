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

    let Err(error) =
        tables.load_file_reads(&root, Arc::new(crate::file_tag_reader::LoftyFileTagReader)).await
    else {
        panic!("one image read as both damaged and whole is no scan's reading");
    };

    assert!(matches!(error, crate::ImportError::Scan(FolderScanError::Other(_))), "{error:?}");
}
