use super::*;
use crate::import::folder_scanner::{
    collect_release_candidate_files_with_scope, StoredCandidateEdits,
};
use crate::import::ReleaseFileScope;
use std::path::Path;
use tempfile::TempDir;

fn categorize(dir: &Path) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        dir,
        ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap()
}

/// A folder of loose tracks yields one file row per track, and the total is
/// their sum.
#[test]
fn loose_tracks_yield_one_file_row_each() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let fixtures = Path::new("tests/fixtures/flac");
    for name in ["01 Test Track 1.flac", "02 Test Track 2.flac"] {
        std::fs::copy(fixtures.join(name), dir.join(name)).unwrap();
    }

    let probed = source_durations(&categorize(dir));
    assert_eq!(probed.units.len(), 2, "one row per audio file: {probed:?}");
    assert!(probed
        .units
        .iter()
        .all(|unit| matches!(unit.audio, AudioFile::Standalone { .. })));
    let sum: u64 = probed
        .units
        .iter()
        .map(|unit| unit.duration_ms.expect("the fixtures probe"))
        .sum();
    assert_eq!(probed.total_ms(), sum);
}

/// Durations describe the bytes accepted by the scan. A later disk mutation
/// cannot silently replace those facts; import validates the stored digest
/// before consuming them.
#[test]
fn changed_file_bytes_do_not_replace_scanned_durations() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let fixtures = Path::new("tests/fixtures/flac");
    for name in ["01 Test Track 1.flac", "02 Test Track 2.flac"] {
        std::fs::copy(fixtures.join(name), dir.join(name)).unwrap();
    }
    let files = categorize(dir);
    std::fs::write(dir.join("02 Test Track 2.flac"), b"not audio at all").unwrap();

    let probed = source_durations(&files);
    let scanned = probed
        .duration_of(&AudioFile::Standalone {
            file_id: "02 Test Track 2.flac".to_string(),
        })
        .expect("the scanned file has a row")
        .expect("the scan persisted its duration");
    assert!(scanned > 0);
    assert!(probed.total_ms() > 0);
}

/// A CUE-carved container yields one file row for the container itself and one
/// slice row per track the sheet describes, the last closed by the container's
/// own total.
#[test]
fn a_cue_carved_container_yields_a_row_per_slice() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    let fixture = Path::new("tests/fixtures/cue_flac");
    for entry in std::fs::read_dir(fixture).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.join(entry.file_name())).unwrap();
    }

    let files = categorize(dir);
    let sheets = files.carving_sheets();
    let Some(sheet) = sheets.first() else {
        panic!("the fixture binds a carving sheet");
    };
    let sheet_id = sheet.file.relative_path.clone();
    let container_id = sheet.audio.relative_path.clone();
    let track_count = sheet.sheet.playable_track_count();

    let probed = source_durations(&files);
    let container = probed
        .duration_of(&AudioFile::Standalone {
            file_id: container_id.clone(),
        })
        .expect("the container has a file row")
        .expect("the container probes");
    let slices: Vec<Option<u64>> = (0..track_count)
        .map(|index| {
            probed
                .duration_of(&AudioFile::SheetSlice {
                    file_id: container_id.clone(),
                    sheet_id: sheet_id.clone(),
                    index: index as u32,
                })
                .expect("every carved track has a slice row")
        })
        .collect();
    assert_eq!(slices.len(), track_count);
    assert!(slices.iter().all(Option::is_some), "{slices:?}");
    let carved: u64 = slices.iter().map(|slice| slice.unwrap()).sum();
    assert!(
        carved <= container,
        "the carved tracks fit in the container: {carved} vs {container}"
    );
    let every_file: u64 = files
        .audio()
        .map(|file| {
            probed
                .duration_of(&AudioFile::Standalone {
                    file_id: file.relative_path.clone(),
                })
                .expect("every audio file has a row")
                .expect("every fixture file probes")
        })
        .sum();
    assert_eq!(
        probed.total_ms(),
        every_file,
        "the container counts once and its slices not at all"
    );
}
