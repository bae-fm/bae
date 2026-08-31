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

    let probed = source_durations(&categorize(dir)).expect("valid scanned audio has durations");
    assert_eq!(probed.units.len(), 2, "one row per audio file: {probed:?}");
    assert!(probed
        .units
        .iter()
        .all(|unit| matches!(unit.audio, AudioFile::Standalone { .. })));
    let sum: u64 = probed.units.iter().map(|unit| unit.duration_ms).sum();
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

    let probed = source_durations(&files).expect("valid scanned audio has durations");
    let scanned = probed
        .duration_of(&AudioFile::Standalone {
            file_id: "02 Test Track 2.flac".to_string(),
        })
        .expect("the scanned file has a duration");
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

    let probed = source_durations(&files).expect("valid CUE timing has durations");
    let container = probed
        .duration_of(&AudioFile::Standalone {
            file_id: container_id.clone(),
        })
        .expect("the container has a duration");
    let slices: Vec<u64> = (0..track_count)
        .map(|index| {
            probed
                .duration_of(&AudioFile::SheetSlice {
                    file_id: container_id.clone(),
                    sheet_id: sheet_id.clone(),
                    index: index as u32,
                })
                .expect("every carved track has a duration")
        })
        .collect();
    assert_eq!(slices.len(), track_count);
    let carved: u64 = slices.iter().sum();
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
                .expect("every fixture file has a duration")
        })
        .sum();
    assert_eq!(
        probed.total_ms(),
        every_file,
        "the container counts once and its slices not at all"
    );
}

#[test]
fn a_cue_track_starting_after_its_audio_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    std::fs::copy(
        Path::new("tests/fixtures/flac/01 Test Track 1.flac"),
        dir.join("Audio.flac"),
    )
    .unwrap();
    std::fs::write(
        dir.join("Audio.cue"),
        "FILE \"Audio.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Track Title\"\n    INDEX 01 99:00:00\n",
    )
    .unwrap();

    let files = categorize(dir);
    assert!(matches!(
        files.track_sheets().next().map(|sheet| sheet.binding),
        Some(crate::import::folder_scanner::SheetBinding::Unresolved)
    ));
    assert!(matches!(
        source_durations(&files)
            .expect("unbound sheet leaves standalone audio")
            .units
            .as_slice(),
        [SourceDuration {
            audio: AudioFile::Standalone { .. },
            ..
        }]
    ));
}

#[test]
fn descending_cue_track_boundaries_are_rejected() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    std::fs::copy(
        Path::new("tests/fixtures/flac/01 Test Track 1.flac"),
        dir.join("Audio.flac"),
    )
    .unwrap();
    std::fs::write(
        dir.join("Audio.cue"),
        "FILE \"Audio.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Track One\"\n    INDEX 01 01:00:00\n  TRACK 02 AUDIO\n    TITLE \"Track Two\"\n    INDEX 01 00:30:00\n",
    )
    .unwrap();

    let files = categorize(dir);
    assert!(matches!(
        files.track_sheets().next().map(|sheet| sheet.binding),
        Some(crate::import::folder_scanner::SheetBinding::Unresolved)
    ));
    assert!(matches!(
        source_durations(&files)
            .expect("unbound sheet leaves standalone audio")
            .units
            .as_slice(),
        [SourceDuration {
            audio: AudioFile::Standalone { .. },
            ..
        }]
    ));
}
