//! The layout a scan stores with a candidate's files.

use super::units::StoredUnit;
use crate::mapping::tests::{cue_sheet_text, scan, write_flac};
use crate::tables::tests::*;
use crate::tables::ImportTables;
use bae_store::Store;

/// Scan the folder `name` below `root`, holding the files `setup` writes,
/// and store it as a valid candidate; its content hash.
async fn stored_folder(
    db: &Store,
    root: &std::path::Path,
    name: &str,
    setup: impl FnOnce(&std::path::Path),
) -> String {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    setup(&folder);
    let files = scan(&folder);
    let candidate = crate::folder_scanner::FolderCandidate {
        path: folder.clone(),
        file_root: folder.clone(),
        name: name.to_owned(),
        files,
        watched_folder_path: root.to_path_buf(),
        scope: crate::folder_scanner::ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        relative_folder: crate::RelativeFolder::of(std::path::Path::new(name)).unwrap(),
        grouping: None,
    };
    let hash = candidate.files.content_hash();
    let tables = ImportTables::new(db.clone());
    tables.add_watched_import_folder(root).await.unwrap();
    let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
    tables
        .save_folder_scan_item(root, generation, &crate::folder_scanner::ScanItem::Valid(candidate))
        .await
        .unwrap();
    tables.finish_folder_scan(root, generation, None).await.unwrap();
    hash
}

/// A loose bonus track and a disc image carved by its sheet into three.
fn bonus_and_disc(folder: &std::path::Path) {
    write_flac(&folder.join("a bonus.flac"));
    write_flac(&folder.join("disc.flac"));
    std::fs::write(folder.join("disc.cue"), cue_sheet_text("disc.flac", 3)).unwrap();
}

#[tokio::test]
async fn scan_writes_one_unit_per_track_in_layout_order() {
    let (db, _tmp, root) = watched_root().await;
    let hash = stored_folder(&db, &root, "Album", bonus_and_disc).await;
    let units = ImportTables::new(db.clone()).load_units_for_test(&hash).await;
    assert_eq!(
        units
            .iter()
            .map(|unit| (unit.file_id.as_str(), unit.slice_index, unit.disc, unit.number))
            .collect::<Vec<_>>(),
        vec![
            ("a bonus.flac", None, None, 1),
            ("disc.flac", Some(0), Some(1), 2),
            ("disc.flac", Some(1), Some(1), 3),
            ("disc.flac", Some(2), Some(1), 4),
        ]
    );
}

/// The sheet's entries sit 15 frames (a fifth of a second) apart in a
/// one-second container: each slice runs to the next boundary, and the last to
/// the container's end.
#[tokio::test]
async fn units_time_each_cue_slice_to_the_next_boundary_or_the_container_end() {
    let (db, _tmp, root) = watched_root().await;
    let hash = stored_folder(&db, &root, "Album", bonus_and_disc).await;
    let units = ImportTables::new(db.clone()).load_units_for_test(&hash).await;
    let slices: Vec<&StoredUnit> = units.iter().filter(|unit| unit.sheet_id.is_some()).collect();
    assert_eq!(slices.iter().map(|unit| unit.duration_ms).collect::<Vec<_>>(), [200, 200, 600]);
    assert_eq!(
        slices.iter().map(|unit| (unit.start_sample, unit.end_sample)).collect::<Vec<_>>(),
        [(Some(0), Some(8820)), (Some(8820), Some(17640)), (Some(17640), None)]
    );
    assert_eq!(units[0].duration_ms, 1000, "a loose file plays for its own length");
}
