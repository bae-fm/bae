//! Binding an import's tracks to the audio their samples come from.

use super::*;
use crate::audio_layout::audio_units;
use crate::audio_layout::tests::{cue_sheet_text, rows_for, scan, write_flac, write_sheet_audio};
use std::fs;

/// A slot nobody named commits under its file's own name. An empty title is
/// a track that cannot be found again, and the file name is what the slot
/// table showed on that row.
#[test]
fn an_unnamed_slot_is_titled_after_its_file() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("hidden track.flac"));
    let files = scan(tmp.path());

    let track_files = resolve_track_files(
        vec![(
            bae_records::TrackRow {
                id: "track-0".to_string(),
                release_id: "release-1".to_string(),
                title: "   ".to_string(),
                medium_id: bae_records::first_medium_id("release-1"),
                side: std::num::NonZeroU8::MIN,
                track_number: std::num::NonZeroU32::MIN,
                created_at: chrono::Utc::now(),
            },
            AudioFile::Standalone { file_id: "hidden track.flac".to_string() },
        )],
        &files,
    )
    .expect("binding succeeds");

    assert_eq!(track_files[0].db_track.title, "hidden track");
}

/// Every slice of a disc image binds to the container, carries its own
/// index into the sheet, and shares one parsed analysis.
#[test]
fn sheet_slices_bind_to_their_container_and_share_one_analysis() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_sheet_audio(&tmp.path().join("CDImage.flac"), 4);
    fs::write(tmp.path().join("CDImage.cue"), cue_sheet_text("CDImage.flac", 4))
        .expect("write cue");
    let files = scan(tmp.path());
    let rows = rows_for(audio_units(&files));

    let track_files = resolve_track_files(rows, &files).expect("binding succeeds");
    assert_eq!(track_files.len(), 4);

    let mut analyses = Vec::new();
    for (position, track_file) in track_files.iter().enumerate() {
        assert!(track_file.db_track.duration_ms > 0, "every slice gets a duration",);
        match &track_file.audio {
            TrackAudio::CueBacked { cue_index, cue_pair } => {
                assert_eq!(*cue_index, position);
                assert_eq!(cue_pair.audio_files[0].path.file_name().unwrap(), "CDImage.flac");
                analyses.push(Arc::as_ptr(cue_pair));
            }
            other => panic!("expected a CueBacked track file, got {other:?}"),
        }
    }
    assert!(
        analyses.windows(2).all(|pair| pair[0] == pair[1]),
        "one sheet is parsed and probed once for all its slices",
    );
}

/// Audio a binding names that is no longer in the folder is the one thing
/// that still refuses: there are no samples to write.
#[test]
fn audio_that_left_the_folder_refuses() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    let files = scan(tmp.path());

    let err = resolve_track_files(
        vec![(
            bae_records::TrackRow {
                id: "track-0".to_string(),
                release_id: "release-1".to_string(),
                title: "Track Title".to_string(),
                medium_id: bae_records::first_medium_id("release-1"),
                side: std::num::NonZeroU8::MIN,
                track_number: std::num::NonZeroU32::MIN,
                created_at: chrono::Utc::now(),
            },
            AudioFile::Standalone { file_id: "02.flac".to_string() },
        )],
        &files,
    )
    .expect_err("audio that is gone cannot be bound");
    assert!(
        matches!(&err, ImportError::UnusableFile { detail } if detail.contains("02.flac")),
        "got: {err}",
    );
}
