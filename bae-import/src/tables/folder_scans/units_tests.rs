//! The layout a scan stores with a candidate's files.

use super::units::StoredUnit;
use crate::mapping::tests::{cue_sheet_text, write_flac};
use crate::tables::folder_scans::tests::stored_folder;
use crate::tables::tests::*;
use crate::tables::ImportTables;

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

/// The folder's front-cover-named image is the blank cover the scan stores
/// with its files, over a file the cover rules rank lower; a folder with no
/// image has none.
#[tokio::test]
async fn scan_stores_the_front_cover_named_file_as_the_blank_cover() {
    let with_art = |folder: &std::path::Path| {
        write_flac(&folder.join("01.flac"));
        std::fs::write(folder.join("back.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        std::fs::write(folder.join("Front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    };
    let without_art = |folder: &std::path::Path| write_flac(&folder.join("01.flac"));
    let (db, _tmp, root) = watched_root().await;
    let covered = stored_folder(&db, &root, "Covered", with_art).await;
    assert_eq!(
        ImportTables::new(db).blank_cover_file_for_test(&covered).await,
        Some("Front.jpg".to_string())
    );
    let (db, _tmp, root) = watched_root().await;
    let bare = stored_folder(&db, &root, "Bare", without_art).await;
    assert_eq!(ImportTables::new(db).blank_cover_file_for_test(&bare).await, None);
}
