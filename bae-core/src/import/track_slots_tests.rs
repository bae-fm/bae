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

fn source_tracks(count: usize) -> Vec<SourceTrack> {
    (0..count)
        .map(|index| SourceTrack {
            edit: TrackUserEdit {
                title: format!("Track Title {}", index + 1),
                side: 1,
                track_number: Some(index as i32 + 1),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: None,
            },
            position: Some((index + 1).to_string()),
            // Three minutes each, which is what the synthetic sheets lay
            // their tracks out at.
            duration_ms: Some(180_000),
        })
        .collect()
}

/// The table's rows for `source` against the folder at `root`, with the
/// folder's audio measured — the table itself opens nothing.
fn slots(source: &[SourceTrack], files: &CategorizedFiles) -> Vec<TrackSlot> {
    let durations = source_durations(files).expect("scanned fixture audio has durations");
    slot_table(source, files, &durations).rows
}

fn scan(root: &Path) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan succeeds")
}

fn file_ids(slots: &[TrackSlot]) -> Vec<Option<&str>> {
    slots
        .iter()
        .map(|slot| slot.file().map(AudioFile::file_id))
        .collect()
}

/// Thirteen files against a twelve-track source: twelve rows agree and the
/// thirteenth file gets a slot of its own, at the end of the folder's own
/// order rather than in a footer. Nothing fails.
#[test]
fn extra_audio_becomes_a_file_only_slot_in_disk_order() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for index in 1..=13 {
        write_flac(&tmp.path().join(format!("{index:02}.flac")));
    }

    let slots = slots(&source_tracks(12), &scan(tmp.path()));

    assert_eq!(slots.len(), 13);
    assert_eq!(
        slots
            .iter()
            .filter(|slot| matches!(slot, TrackSlot::Paired { .. }))
            .count(),
        12,
    );
    assert!(matches!(slots[12], TrackSlot::FileOnly { .. }));
    assert_eq!(
        file_ids(&slots),
        (1..=13)
            .map(|index| format!("{index:02}.flac"))
            .collect::<Vec<_>>()
            .iter()
            .map(|id| Some(id.as_str()))
            .collect::<Vec<_>>(),
    );
    // The unnamed slot keeps a place in the numbering rather than
    // restarting it.
    let unnamed = slots[12].track();
    assert_eq!(unnamed.title, "");
    assert_eq!(unnamed.side, 1);
    assert_eq!(unnamed.track_number, Some(13));
}

/// Fourteen source tracks against thirteen files: the fourteenth is a slot
/// with no audio, and every other row still pairs.
#[test]
fn a_track_with_no_audio_becomes_a_track_only_slot() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for index in 1..=13 {
        write_flac(&tmp.path().join(format!("{index:02}.flac")));
    }

    let slots = slots(&source_tracks(14), &scan(tmp.path()));

    assert_eq!(slots.len(), 14);
    assert_eq!(
        slots
            .iter()
            .filter(|slot| matches!(slot, TrackSlot::Paired { .. }))
            .count(),
        13,
    );
    match &slots[13] {
        TrackSlot::TrackOnly { track, .. } => {
            assert_eq!(track.title, "Track Title 14");
            assert!(track.file.is_none());
        }
        other => panic!("expected a TrackOnly slot, got {other:?}"),
    }
}

/// A disc image plus two loose bonus tracks. The sheet's slices and the two
/// standalone files coexist, in disk order — neither set is dropped for the
/// other, which is the additive property.
#[test]
fn a_disc_image_and_loose_audio_produce_slots_for_both() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_sheet_audio(&tmp.path().join("CDImage.flac"), 11);
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 11),
    )
    .expect("write cue");
    write_flac(&tmp.path().join("bonus-1.flac"));
    write_flac(&tmp.path().join("bonus-2.flac"));

    let files = scan(tmp.path());
    let slots = slots(&source_tracks(13), &files);

    assert_eq!(slots.len(), 13);
    // Disk order (case-insensitive): the bonus files sort before the disc
    // image, so their slots lead. The image's eleven slices follow in sheet
    // order.
    assert_eq!(
        file_ids(&slots),
        vec![
            Some("bonus-1.flac"),
            Some("bonus-2.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
            Some("CDImage.flac"),
        ],
    );
    let slice_indices: Vec<u32> = slots
        .iter()
        .filter_map(|slot| match slot.file() {
            Some(AudioFile::SheetSlice { index, .. }) => Some(*index),
            _ => None,
        })
        .collect();
    assert_eq!(slice_indices, (0..11).collect::<Vec<_>>());
    assert!(slots
        .iter()
        .all(|slot| matches!(slot, TrackSlot::Paired { .. })));
    // The loose files are standalone slots, not slices of the image.
    assert!(matches!(
        slots[0].file(),
        Some(AudioFile::Standalone { .. })
    ));
}

/// A sheet describing more tracks than the source names lands in the same
/// table as any other disagreement: extra slices become `FileOnly` slots,
/// and nothing errors on the way.
#[test]
fn a_sheet_disagreeing_with_the_source_lands_in_the_same_table() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_sheet_audio(&tmp.path().join("CDImage.flac"), 12);
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 12),
    )
    .expect("write cue");

    let files = scan(tmp.path());

    let more_slices_than_tracks = slots(&source_tracks(10), &files);
    assert_eq!(more_slices_than_tracks.len(), 12);
    assert_eq!(
        more_slices_than_tracks
            .iter()
            .filter(|slot| matches!(slot, TrackSlot::FileOnly { .. }))
            .count(),
        2,
    );

    let fewer_slices_than_tracks = slots(&source_tracks(14), &files);
    assert_eq!(fewer_slices_than_tracks.len(), 14);
    assert_eq!(
        fewer_slices_than_tracks
            .iter()
            .filter(|slot| matches!(slot, TrackSlot::TrackOnly { .. }))
            .count(),
        2,
    );
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

    let total: usize = (1..=10).sum();
    let slots = slots(&source_tracks(total), &scan(tmp.path()));

    assert_eq!(slots.len(), total);
    let mut expected = Vec::new();
    for disc in 1..=10 {
        for _ in 0..disc {
            expected.push(Some(format!("CD{disc}/CDImage.flac")));
        }
    }
    assert_eq!(
        file_ids(&slots),
        expected.iter().map(|id| id.as_deref()).collect::<Vec<_>>(),
    );
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

/// Re-pairing two slots is a swap of their bindings, and the swapped rows
/// are what `resolve_track_files` binds — the correction is not re-derived
/// away by position.
#[test]
fn a_corrected_pairing_is_what_gets_bound() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for index in 1..=3 {
        write_flac(&tmp.path().join(format!("{index:02}.flac")));
    }
    let files = scan(tmp.path());
    let mut slots = slots(&source_tracks(3), &files);

    // The rip named its files in the wrong order: track 1 is really 02.flac
    // and track 2 is really 01.flac.
    let first = slots[0].track().file.clone();
    let second = slots[1].track().file.clone();
    match &mut slots[0] {
        TrackSlot::Paired { track, .. } => track.file = second,
        other => panic!("expected a Paired slot, got {other:?}"),
    }
    match &mut slots[1] {
        TrackSlot::Paired { track, .. } => track.file = first,
        other => panic!("expected a Paired slot, got {other:?}"),
    }

    let now = chrono::Utc::now();
    let rows: Vec<(DbTrack, AudioFile)> = slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            let track = slot.into_track();
            (
                DbTrack {
                    id: format!("track-{index}"),
                    release_id: "release-1".to_string(),
                    title: track.title.clone(),
                    side: track.side,
                    track_number: track.track_number,
                    duration_ms: None,
                    discogs_position: None,
                    created_at: now,
                },
                track.file.expect("every row is paired"),
            )
        })
        .collect();

    let track_files = resolve_track_files(rows, &files).expect("binding succeeds");
    let bound: Vec<(&str, String)> = track_files
        .iter()
        .map(|track_file| {
            (
                track_file.db_track().id.as_str(),
                track_file
                    .file_path()
                    .file_name()
                    .expect("file name")
                    .to_string_lossy()
                    .into_owned(),
            )
        })
        .collect();
    assert_eq!(
        bound,
        vec![
            ("track-0", "02.flac".to_string()),
            ("track-1", "01.flac".to_string()),
            ("track-2", "03.flac".to_string()),
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
                side: 1,
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

    assert_eq!(track_files[0].db_track().title, "hidden track");
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
    let slots = slots(&source_tracks(4), &files);

    let now = chrono::Utc::now();
    let rows: Vec<(DbTrack, AudioFile)> = slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            let track = slot.into_track();
            (
                DbTrack {
                    id: format!("track-{index}"),
                    release_id: "release-1".to_string(),
                    title: track.title.clone(),
                    side: track.side,
                    track_number: track.track_number,
                    duration_ms: None,
                    discogs_position: None,
                    created_at: now,
                },
                track.file.expect("every row is paired"),
            )
        })
        .collect();

    let track_files = resolve_track_files(rows, &files).expect("binding succeeds");
    assert_eq!(track_files.len(), 4);

    let mut analyses = Vec::new();
    for (position, track_file) in track_files.iter().enumerate() {
        match track_file {
            TrackFile::CueBacked {
                cue_index,
                cue_pair,
                file_path,
                db_track,
            } => {
                assert_eq!(*cue_index, position);
                assert_eq!(file_path.file_name().unwrap(), "CDImage.flac");
                assert!(
                    db_track.duration_ms.is_some(),
                    "every slice gets a duration",
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

/// Every paired row carries both lengths — the file's own, probed off disk,
/// and the one the source states. That pair is what catches a pairing which
/// is complete but wrong; counting cannot see it, and neither number is
/// derivable from the other side.
#[test]
fn a_paired_row_carries_the_probed_length_and_the_source_s() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for index in 1..=3 {
        write_flac(&tmp.path().join(format!("{index:02}.flac")));
    }

    let rows = slots(&source_tracks(3), &scan(tmp.path()));

    for row in &rows {
        match row {
            TrackSlot::Paired {
                source_duration_ms,
                file,
                ..
            } => {
                // The synthetic FLAC declares one second of audio; the
                // source says three minutes. The row shows both, and says
                // they disagree — which is the whole point of showing two.
                assert_eq!(file.duration_ms, Some(1_000));
                assert_eq!(*source_duration_ms, Some(180_000));
                assert!(lengths_disagree(file.duration_ms, *source_duration_ms));
            }
            other => panic!("expected a Paired row, got {other:?}"),
        }
    }
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
    let table = slot_table(&source_tracks(2), &files, &durations);

    let lengths: Vec<Option<u64>> = table.audio.iter().map(|file| file.duration_ms).collect();
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
        Some(crate::import::folder_scanner::SheetBinding::Unresolved)
    ));
    assert!(matches!(
        files.sheet_binding_options("CDImage.cue").as_slice(),
        [crate::import::folder_scanner::SheetBindingOption {
            file_id,
            offer: SheetBindingOffer::RefusedTiming,
        }] if file_id == "CDImage.flac"
    ));
}

/// One container carved into several rows reads as one run down the link
/// column: first, middle, last. A file backing a row on its own is whole.
#[test]
fn a_container_s_rows_read_as_one_run() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_sheet_audio(&tmp.path().join("CDImage.flac"), 3);
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 3),
    )
    .expect("write cue");
    write_flac(&tmp.path().join("bonus.flac"));

    let files = scan(tmp.path());
    let durations = source_durations(&files).expect("scanned fixture audio has durations");
    let table = slot_table(&source_tracks(4), &files, &durations);

    assert_eq!(
        table.audio.iter().map(|file| file.span).collect::<Vec<_>>(),
        vec![
            SlotSpan::Whole,
            SlotSpan::ContainerStart,
            SlotSpan::ContainerMiddle,
            SlotSpan::ContainerEnd,
        ],
    );
    // Every slice names the container, which is what a reader is being told
    // by the run: three rows, one file.
    assert!(table.audio[1..]
        .iter()
        .all(|file| file.name == "CDImage.flac"));
}

/// The tally names which way the two sides disagree, and says so without
/// refusing anything.
#[test]
fn the_tally_names_the_disagreement() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for index in 1..=13 {
        write_flac(&tmp.path().join(format!("{index:02}.flac")));
    }
    let files = scan(tmp.path());
    let durations = source_durations(&files).expect("scanned fixture audio has durations");

    assert_eq!(
        slot_table(&source_tracks(13), &files, &durations).reconciliation,
        SlotReconciliation::Agrees { count: 13 },
    );
    assert_eq!(
        slot_table(&source_tracks(12), &files, &durations).reconciliation,
        SlotReconciliation::MoreFiles {
            files: 13,
            tracks: 12,
        },
    );
    assert_eq!(
        slot_table(&source_tracks(14), &files, &durations).reconciliation,
        SlotReconciliation::MoreTracks {
            files: 13,
            tracks: 14,
        },
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
            DbTrack {
                id: "track-0".to_string(),
                release_id: "release-1".to_string(),
                title: "Track Title".to_string(),
                side: 1,
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
