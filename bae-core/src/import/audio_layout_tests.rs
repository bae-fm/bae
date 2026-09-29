use super::*;
use crate::import::folder_scanner::{
    collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingOffer, SheetDisc,
    SheetDiscEdits, StoredCandidateEdits,
};
use crate::import::probe::source_durations;
use std::fs;
use std::path::Path;

/// Synthetic FLAC bytes valid enough to round-trip through the scan's
/// audio validation and the CUE container probe.
///
/// 44.1 kHz / 2-channel / 16-bit STREAMINFO declaring `duration_ms` of audio.
fn synthetic_flac_bytes(duration_ms: u64) -> Vec<u8> {
    let sample_rate: u32 = 44_100;
    let channels: u32 = 2;
    let bps: u32 = 16;
    let total_samples = u64::from(sample_rate) * duration_ms / 1_000;

    let mut buf = Vec::new();
    buf.extend_from_slice(b"fLaC");

    // STREAMINFO block header: last-block=1, type=0, length=34.
    buf.extend_from_slice(&[0x80, 0x00, 0x00, 34]);

    // STREAMINFO data: 34 bytes laid out as
    //   [0..2]   min block size
    //   [2..4]   max block size
    //   [4..7]   min frame size
    //   [7..10]  max frame size
    //   [10..13] sample rate (20 bits) | channels-1 (3) | bps-1 high bit
    //   [13]     bps-1 low 4 bits | total_samples high 4 bits
    //   [14..18] total_samples low 32 bits
    //   [18..34] MD5 signature
    buf.extend_from_slice(&[0x10, 0x00, 0x10, 0x00]);
    buf.extend_from_slice(&[0u8; 6]);

    let ch_minus_1 = (channels - 1) & 0x07;
    let bps_minus_1 = (bps - 1) & 0x1F;
    let ts_high = ((total_samples >> 32) & 0x0F) as u8;

    buf.push((sample_rate >> 12) as u8);
    buf.push(((sample_rate >> 4) & 0xFF) as u8);
    buf.push(
        (((sample_rate & 0x0F) as u8) << 4)
            | ((ch_minus_1 as u8) << 1)
            | ((bps_minus_1 >> 4) as u8),
    );
    buf.push((((bps_minus_1 & 0x0F) as u8) << 4) | ts_high);
    buf.extend_from_slice(&((total_samples & 0xFFFF_FFFF) as u32).to_be_bytes());
    buf.extend_from_slice(&[0u8; 16]);

    debug_assert_eq!(buf.len(), 42);
    buf.resize(18_000, 0);
    buf
}

/// A sheet naming one container for the whole disc, with `count` playable
/// tracks three minutes apart.
fn cue_sheet_text(audio_file_name: &str, count: usize) -> String {
    let mut text = String::from("PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n");
    text.push_str(&format!("FILE \"{audio_file_name}\" WAVE\n"));
    for index in 0..count {
        text.push_str(&format!("  TRACK {:02} AUDIO\n", index + 1));
        text.push_str(&format!("    TITLE \"Track {}\"\n", index + 1));
        text.push_str(&format!("    INDEX 01 {:02}:00:00\n", index * 3));
    }
    text
}

fn write_flac(path: &Path) {
    fs::write(path, synthetic_flac_bytes(1_000)).expect("write flac");
}

fn write_sheet_audio(path: &Path, track_count: usize) {
    let duration_ms = u64::try_from(track_count)
        .expect("fixture track count fits u64")
        .checked_mul(180_000)
        .expect("fixture duration fits u64");
    fs::write(path, synthetic_flac_bytes(duration_ms)).expect("write sheet audio");
}

fn scan(root: &Path) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan succeeds")
}

fn file_ids(units: &[AudioFile]) -> Vec<&str> {
    units.iter().map(AudioFile::file_id).collect()
}

/// A blank row over every unit, in the units' order — what the draft commits
/// when nobody named anything.
fn rows_for(units: Vec<AudioFile>) -> Vec<(DbTrack, AudioFile)> {
    let now = chrono::Utc::now();
    units
        .into_iter()
        .enumerate()
        .map(|(index, unit)| {
            (
                DbTrack {
                    id: format!("track-{index}"),
                    release_id: "release-1".to_string(),
                    title: format!("Track Title {}", index + 1),
                    side: Some(1),
                    track_number: Some(index as i32 + 1),
                    duration_ms: None,
                    discogs_position: None,
                    created_at: now,
                },
                unit,
            )
        })
        .collect()
}

/// Loose audio is one unit per file, in the folder's own order.
#[test]
fn loose_audio_is_one_unit_per_file_in_disk_order() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for index in 1..=13 {
        write_flac(&tmp.path().join(format!("{index:02}.flac")));
    }

    let units = audio_units(&scan(tmp.path()));

    assert_eq!(
        file_ids(&units),
        (1..=13)
            .map(|index| format!("{index:02}.flac"))
            .collect::<Vec<_>>(),
    );
    assert!(units
        .iter()
        .all(|unit| matches!(unit, AudioFile::Standalone { .. })));
}

/// A disc image plus two loose bonus tracks. The sheet's slices and the two
/// standalone files coexist, in disk order — neither set is dropped for the
/// other, which is the additive property.
#[test]
fn a_disc_image_and_loose_audio_both_become_units() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_sheet_audio(&tmp.path().join("CDImage.flac"), 11);
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 11),
    )
    .expect("write cue");
    write_flac(&tmp.path().join("bonus-1.flac"));
    write_flac(&tmp.path().join("bonus-2.flac"));

    let units = audio_units(&scan(tmp.path()));

    // Disk order (case-insensitive): the bonus files sort before the disc
    // image, so their units lead. The image's eleven slices follow in sheet
    // order.
    let mut expected = vec!["bonus-1.flac", "bonus-2.flac"];
    expected.extend(std::iter::repeat_n("CDImage.flac", 11));
    assert_eq!(file_ids(&units), expected);
    let slice_indices: Vec<u32> = units
        .iter()
        .filter_map(|unit| match unit {
            AudioFile::SheetSlice { index, .. } => Some(*index),
            AudioFile::Standalone { .. } => None,
        })
        .collect();
    assert_eq!(slice_indices, (0..11).collect::<Vec<_>>());
    assert!(matches!(units[0], AudioFile::Standalone { .. }));
}

/// Multi-disc rips lay their discs down in the order a person reads them:
/// `CD10` after `CD9`, not after `CD1`.
#[test]
fn discs_are_laid_down_in_natural_order() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for disc in 1..=10 {
        let dir = tmp.path().join(format!("CD{disc}"));
        fs::create_dir_all(&dir).expect("mkdir");
        write_sheet_audio(&dir.join("CDImage.flac"), disc);
        fs::write(
            dir.join("CDImage.cue"),
            cue_sheet_text("CDImage.flac", disc),
        )
        .expect("write cue");
    }

    let units = audio_units(&scan(tmp.path()));

    let mut expected = Vec::new();
    for disc in 1..=10 {
        for _ in 0..disc {
            expected.push(format!("CD{disc}/CDImage.flac"));
        }
    }
    assert_eq!(file_ids(&units), expected);
}

/// Settle `files` as if the user had made these disc assignments.
fn assign_discs(files: &mut CategorizedFiles, assignments: &[(&str, SheetDisc)]) {
    let mut sheet_discs = SheetDiscEdits::default();
    for (sheet_id, disc) in assignments {
        sheet_discs.set((*sheet_id).to_string(), *disc);
    }
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            sheet_discs,
            ..Default::default()
        })
        .expect("the folder stays valid");
}

/// Cue filenames are arbitrary, so the assignment is what says which sheet
/// holds which disc. Two sheets whose names read the other way round still
/// lay disc one's tracks down first.
#[test]
fn the_disc_assignment_orders_the_units_the_filenames_do_not() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_sheet_audio(&tmp.path().join("alpha.flac"), 2);
    fs::write(
        tmp.path().join("alpha.cue"),
        cue_sheet_text("alpha.flac", 2),
    )
    .expect("write cue");
    write_sheet_audio(&tmp.path().join("beta.flac"), 3);
    fs::write(tmp.path().join("beta.cue"), cue_sheet_text("beta.flac", 3)).expect("write cue");

    let mut files = scan(tmp.path());
    assign_discs(
        &mut files,
        &[
            ("alpha.cue", SheetDisc::Disc { number: 2 }),
            ("beta.cue", SheetDisc::Disc { number: 1 }),
        ],
    );

    let slice = |sheet: &str, container: &str, index: u32| AudioFile::SheetSlice {
        file_id: container.to_string(),
        sheet_id: sheet.to_string(),
        index,
    };
    assert_eq!(
        audio_units(&files),
        vec![
            slice("beta.cue", "beta.flac", 0),
            slice("beta.cue", "beta.flac", 1),
            slice("beta.cue", "beta.flac", 2),
            slice("alpha.cue", "alpha.flac", 0),
            slice("alpha.cue", "alpha.flac", 1),
        ],
    );
}

/// A sheet taken out of the tracklist stops speaking for its container, so
/// the container is loose audio again. Everything else the folder offers is
/// where it was.
#[test]
fn an_ignored_sheet_leaves_its_container_a_track_of_its_own() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_sheet_audio(&tmp.path().join("CDImage.flac"), 3);
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 3),
    )
    .expect("write cue");
    write_flac(&tmp.path().join("bonus.flac"));

    let mut files = scan(tmp.path());
    assert_eq!(audio_units(&files).len(), 4, "three slices and the bonus");

    assign_discs(&mut files, &[("CDImage.cue", SheetDisc::Ignored)]);

    assert_eq!(
        audio_units(&files),
        vec![
            AudioFile::Standalone {
                file_id: "bonus.flac".to_string(),
            },
            AudioFile::Standalone {
                file_id: "CDImage.flac".to_string(),
            },
        ],
    );
}

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
            DbTrack {
                id: "track-0".to_string(),
                release_id: "release-1".to_string(),
                title: "   ".to_string(),
                side: Some(1),
                track_number: Some(1),
                duration_ms: None,
                discogs_position: None,
                created_at: chrono::Utc::now(),
            },
            AudioFile::Standalone {
                file_id: "hidden track.flac".to_string(),
            },
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
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 4),
    )
    .expect("write cue");
    let files = scan(tmp.path());
    let rows = rows_for(audio_units(&files));

    let track_files = resolve_track_files(rows, &files).expect("binding succeeds");
    assert_eq!(track_files.len(), 4);

    let mut analyses = Vec::new();
    for (position, track_file) in track_files.iter().enumerate() {
        assert!(
            track_file.db_track.duration_ms.is_some(),
            "every slice gets a duration",
        );
        match &track_file.audio {
            TrackAudio::CueBacked {
                cue_index,
                cue_pair,
            } => {
                assert_eq!(*cue_index, position);
                assert_eq!(
                    cue_pair.audio_files[0].path.file_name().unwrap(),
                    "CDImage.flac"
                );
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

/// The row decides one way for both surfaces. A rip that differs by a
/// pregap and a rounded second reads as agreement; one that differs by a
/// whole different take does not; and a length nobody could read is not a
/// disagreement, because there is nothing to compare.
#[test]
fn a_length_nobody_can_read_is_not_a_disagreement() {
    assert!(!lengths_disagree(Some(180_000), Some(180_000)));
    assert!(!lengths_disagree(
        Some(180_000),
        Some(180_000 + LENGTH_DISAGREEMENT_MS)
    ));
    assert!(lengths_disagree(
        Some(180_000),
        Some(180_001 + LENGTH_DISAGREEMENT_MS)
    ));
    assert!(lengths_disagree(Some(180_000), Some(120_000)));
    assert!(!lengths_disagree(None, Some(180_000)));
    assert!(!lengths_disagree(Some(180_000), None));
    assert!(!lengths_disagree(None, None));
}

/// A slice's length comes from the sheet's own timing, and the last slice —
/// which has no next-track boundary in the sheet — from the container's
/// total minus its start. The same reading the commit writes.
#[test]
fn a_sheet_s_slices_each_carry_their_own_length() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(
        tmp.path().join("CDImage.cue"),
        // Two tracks, the second starting half a second in, out of one
        // second of audio.
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\nFILE \"CDImage.flac\" WAVE\n  \
         TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 01 00:00:38\n",
    )
    .expect("write cue");

    let files = scan(tmp.path());
    let durations = source_durations(&files).expect("scanned fixture audio has durations");

    let lengths: Vec<Option<u64>> = audio_units(&files)
        .iter()
        .map(|unit| durations.duration_of(unit))
        .collect();
    // 38 frames of 1/75s is ~506ms; the tail is what is left of the second.
    assert_eq!(lengths.len(), 2);
    assert_eq!(lengths[0], Some(506));
    assert_eq!(lengths[1], Some(494));
}

#[test]
fn a_sheet_whose_timing_exceeds_its_audio_leaves_the_container_standalone() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 2),
    )
    .expect("write cue");

    let files = scan(tmp.path());

    assert_eq!(
        audio_units(&files),
        vec![AudioFile::Standalone {
            file_id: "CDImage.flac".to_string(),
        }]
    );
    assert!(matches!(
        files.track_sheets().next().map(|sheet| sheet.binding),
        Some(crate::import::folder_scanner::SheetBinding::Unresolved { .. })
    ));
    assert!(matches!(
        files.sheet_binding_options("CDImage.cue")[0].options.as_slice(),
        [crate::import::folder_scanner::SheetBindingOption {
            file_id,
            offer: SheetBindingOffer::RefusedTiming,
        }] if file_id == "CDImage.flac"
    ));
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
            DbTrack {
                id: "track-0".to_string(),
                release_id: "release-1".to_string(),
                title: "Track Title".to_string(),
                side: Some(1),
                track_number: Some(1),
                duration_ms: None,
                discogs_position: None,
                created_at: chrono::Utc::now(),
            },
            AudioFile::Standalone {
                file_id: "02.flac".to_string(),
            },
        )],
        &files,
    )
    .expect_err("audio that is gone cannot be bound");
    assert!(
        matches!(&err, ImportError::UnusableFile { detail } if detail.contains("02.flac")),
        "got: {err}",
    );
}
