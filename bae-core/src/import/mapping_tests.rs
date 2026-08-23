use super::*;
use crate::import::folder_scanner::{
    collect_release_candidate_files_with_scope, StoredCandidateEdits,
};
use crate::import::probe::{probe_durations, ProbedDurations};
use crate::import::track_slots::{slot_table, SourceTrack};
use crate::import::TrackUserEdit;
use std::fs;
use std::path::Path;

/// 44.1 kHz / 2-channel / 16-bit STREAMINFO declaring one second of audio —
/// enough for the scan's validation and the container probe.
///
/// The 34-byte STREAMINFO packs the sample rate (20 bits), channels − 1
/// (3 bits) and bits-per-sample − 1 (5 bits) across three bytes, then the
/// total sample count and an MD5 signature.
fn synthetic_flac_bytes() -> Vec<u8> {
    const CHANNELS_MINUS_1: u8 = 1;
    const BPS_MINUS_1: u8 = 15;
    let sample_rate: u32 = 44_100;

    let mut buf = Vec::new();
    buf.extend_from_slice(b"fLaC");
    buf.extend_from_slice(&[0x80, 0x00, 0x00, 34]);
    buf.extend_from_slice(&[0x10, 0x00, 0x10, 0x00]);
    buf.extend_from_slice(&[0u8; 6]);
    buf.push((sample_rate >> 12) as u8);
    buf.push(((sample_rate >> 4) & 0xFF) as u8);
    buf.push((((sample_rate & 0x0F) as u8) << 4) | (CHANNELS_MINUS_1 << 1) | (BPS_MINUS_1 >> 4));
    buf.push((BPS_MINUS_1 & 0x0F) << 4);
    buf.extend_from_slice(&44_100u32.to_be_bytes());
    buf.extend_from_slice(&[0u8; 16]);
    buf.resize(18_000, 0);
    buf
}

fn write_flac(path: &Path) {
    fs::write(path, synthetic_flac_bytes()).expect("write flac");
}

/// A sheet naming one container for the whole disc, its entries a fifth of
/// a second apart so every entry but the last has a length of its own.
fn cue_sheet_text(audio_file_name: &str, count: usize) -> String {
    let mut text = String::from("PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n");
    text.push_str(&format!("FILE \"{audio_file_name}\" WAVE\n"));
    for index in 0..count {
        text.push_str(&format!("  TRACK {:02} AUDIO\n", index + 1));
        text.push_str(&format!("    TITLE \"Sheet Track {}\"\n", index + 1));
        text.push_str(&format!("    INDEX 01 00:00:{:02}\n", index * 15));
    }
    text
}

fn scan(root: &Path) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        root,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan succeeds")
}

fn source_tracks(count: usize) -> Vec<SourceTrack> {
    (0..count)
        .map(|index| SourceTrack {
            edit: TrackUserEdit {
                title: format!("Track Title {}", index + 1),
                side: 1,
                track_number: Some(index as i32 + 1),
                artist_names: Vec::new(),
                file: None,
            },
            position: (index + 1).to_string(),
            duration_ms: Some(180_000),
        })
        .collect()
}

fn becomes(row: &MappingRow) -> Vec<&MappingBecomes> {
    match row {
        MappingRow::Unit(unit) => vec![&unit.becomes],
        MappingRow::Sheet { entries, .. } => entries.iter().map(|e| &e.becomes).collect(),
        MappingRow::Directory(_) => Vec::new(),
    }
}

fn file_row(row: &MappingRow) -> &MappingFile {
    match row {
        MappingRow::Unit(MappingUnit {
            source: MappingSource::File(file),
            ..
        }) => file,
        other => panic!("expected a file row, got {other:?}"),
    }
}

/// Nothing is picked yet, so every audio row is an open question — but a
/// rip log is still carried, because a role is a fact about the folder and
/// needs no release.
#[test]
fn with_no_pick_the_audio_rows_await_one_and_the_rest_still_say_what_they_become() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");
    fs::write(tmp.path().join("rip.log"), b"log").expect("write log");

    let table = mapping_table(&scan(tmp.path()), None, &ProbedDurations::default());

    assert!(table.reconciliation.is_none());
    assert_eq!(table.images.len(), 1);
    assert!(matches!(
        becomes(&table.rows[0])[0],
        MappingBecomes::AwaitingPick
    ));
    assert_eq!(file_row(&table.rows[0]).name, "01.flac");
    assert!(matches!(
        becomes(&table.rows[1])[0],
        MappingBecomes::AwaitingPick
    ));
    assert_eq!(file_row(&table.rows[1]).name, "02.flac");
    assert!(matches!(becomes(&table.rows[2])[0], MappingBecomes::Kept));
    assert_eq!(file_row(&table.rows[2]).name, "rip.log");
    // A row nothing has opened has no probed length to show.
    assert_eq!(file_row(&table.rows[0]).probed_duration_ms, None);
    assert_eq!(file_row(&table.rows[0]).role, MappingRole::Audio);
    assert_eq!(file_row(&table.rows[2]).role, MappingRole::Document);
}

/// The folder's images are one gallery beside the table rows, with the one that
/// leads the release marked.
#[test]
fn the_folder_s_images_are_a_gallery_beside_the_table_rows() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");
    fs::write(tmp.path().join("back.jpg"), fake_jpeg()).expect("write back");
    fs::create_dir(tmp.path().join("scans")).expect("scans dir");
    for name in ["scan1.jpg", "scan2.jpg", "scan3.jpg"] {
        fs::write(tmp.path().join("scans").join(name), fake_jpeg()).expect("write scan");
    }

    let table = mapping_table(&scan(tmp.path()), None, &ProbedDurations::default());

    assert_eq!(table.images.len(), 5);
    assert_eq!(
        table
            .images
            .iter()
            .map(|image| image.file_id.as_str())
            .collect::<Vec<_>>(),
        [
            "back.jpg",
            "cover.jpg",
            "scans/scan1.jpg",
            "scans/scan2.jpg",
            "scans/scan3.jpg",
        ],
        "the gallery preserves the scan's authoritative order"
    );
    assert_eq!(
        table.images.iter().filter(|image| image.is_cover).count(),
        1,
        "exactly one image leads the release"
    );
    // A directory of images is not collapsed away from the gallery — its
    // files are in it, each with the path a thumbnail reads.
    assert!(table
        .images
        .iter()
        .any(|image| image.file_id == "scans/scan1.jpg" && image.path.exists()));
    assert_eq!(table.rows.len(), 1);
    assert_eq!(file_row(&table.rows[0]).name, "01.flac");
}

/// A bound sheet is one group row over its entries: the entries carry the
/// sheet's own titles and timings on the left, and on the right each is the
/// track the pick puts on that slice.
#[test]
fn a_sheet_s_entries_carry_its_own_titles_and_bind_to_its_slices() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 3),
    )
    .expect("write cue");

    let files = scan(tmp.path());
    let durations = probe_durations(&files);
    let slots = slot_table(&source_tracks(3), &files, &durations);
    let table = mapping_table(
        &files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix: "import-track",
            source: TracklistSource::Release,
        }),
        &durations,
    );

    assert_eq!(table.rows.len(), 1, "the sheet is the folder's only row");
    let MappingRow::Sheet { sheet, entries } = &table.rows[0] else {
        panic!("expected a sheet row, got {:?}", table.rows[0]);
    };
    assert_eq!(sheet.sheet_id, "CDImage.cue");
    assert_eq!(sheet.assignment, SheetDisc::Disc { number: 1 });
    assert_eq!(sheet.path, tmp.path().join("CDImage.cue"));
    let SheetBound::Describes(container) = &sheet.bound else {
        panic!("expected a bound sheet, got {:?}", sheet.bound);
    };
    assert_eq!(container.name, "CDImage.flac");
    assert_eq!(entries.len(), 3);

    for (index, entry) in entries.iter().enumerate() {
        let MappingSource::SheetEntry(source) = &entry.source else {
            panic!("expected a sheet entry, got {:?}", entry.source);
        };
        assert_eq!(source.index, index as u32);
        assert_eq!(source.number, index as u32 + 1);
        assert_eq!(
            source.title.as_deref(),
            Some(&*format!("Sheet Track {}", index + 1))
        );
        assert_eq!(source.container_id, "CDImage.flac");
        // Every entry but the last has a next-entry boundary in the sheet.
        assert_eq!(source.duration_ms.is_some(), index < 2);

        let MappingBecomes::Track { track, .. } = &entry.becomes else {
            panic!("expected a track, got {:?}", entry.becomes);
        };
        assert_eq!(
            track.file,
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: index as u32,
            }),
        );
        // The right half is the release's tracklist, not the sheet's.
        assert_eq!(track.title, format!("Track Title {}", index + 1));
    }
    assert_eq!(
        table.reconciliation,
        Some(SlotReconciliation::Agrees { count: 3 }),
    );
}

/// A release naming more tracks than the folder holds closes the table with
/// one empty-left row per track nothing backs.
#[test]
fn tracks_the_folder_has_nothing_for_close_the_table() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));

    let files = scan(tmp.path());
    let durations = probe_durations(&files);
    let slots = slot_table(&source_tracks(4), &files, &durations);
    let table = mapping_table(
        &files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix: "import-track",
            source: TracklistSource::Release,
        }),
        &durations,
    );

    assert_eq!(table.rows.len(), 4);
    assert!(matches!(
        table.rows[2],
        MappingRow::Unit(MappingUnit {
            source: MappingSource::Missing,
            ..
        }),
    ));
    let MappingRow::Unit(MappingUnit {
        becomes: MappingBecomes::Track { track, .. },
        ..
    }) = &table.rows[3]
    else {
        panic!("expected a track row, got {:?}", table.rows[3]);
    };
    assert_eq!(track.title, "Track Title 4");
    assert_eq!(track.file, None, "nothing on disk backs it");
    assert_eq!(
        table.reconciliation,
        Some(SlotReconciliation::MoreTracks {
            files: 2,
            tracks: 4,
        }),
    );
}

/// The tracks the commit writes are the table's own rows, in the order the
/// table lays them out, each addressable on its own.
#[test]
fn the_commit_tracks_are_the_table_s_rows_in_order() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 2),
    )
    .expect("write cue");
    write_flac(&tmp.path().join("bonus.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");

    let files = scan(tmp.path());
    let durations = probe_durations(&files);
    let slots = slot_table(&source_tracks(4), &files, &durations);
    let table = mapping_table(
        &files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix: "import-track",
            source: TracklistSource::Release,
        }),
        &durations,
    );

    let tracks = mapping_tracks(&table);
    assert_eq!(
        tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        vec![
            "import-track-0",
            "import-track-1",
            "import-track-2",
            "import-track-3",
        ],
    );
    assert_eq!(
        tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        vec![
            "Track Title 1",
            "Track Title 2",
            "Track Title 3",
            "Track Title 4",
        ],
    );
    // The cover is not a track, the bonus file leads the sheet's two slices
    // exactly as the folder's audio units do (case-insensitive name order),
    // and the fourth track is the one the folder has nothing for.
    assert_eq!(
        tracks.iter().map(|t| t.file.clone()).collect::<Vec<_>>(),
        vec![
            Some(AudioFile::Standalone {
                file_id: "bonus.flac".to_string(),
            }),
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: 0,
            }),
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: 1,
            }),
            None,
        ],
    );
}

/// A sheet whose `FILE` directive names audio that is not in the folder
/// describes nothing — and says what it was looking for, so the header can
/// state it while it offers the folder's own audio instead. It also carries
/// its own path, which is what opens it in the document viewer.
#[test]
fn a_sheet_that_describes_nothing_says_what_it_asked_for() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.wav", 3),
    )
    .expect("write cue");

    let table = mapping_table(&scan(tmp.path()), None, &ProbedDurations::default());
    // The sheet is named where it sits on disk, after the loose audio that
    // sorts before it — a sheet that carves nothing occupies no run.
    let Some(MappingRow::Sheet { sheet, entries }) = table
        .rows
        .iter()
        .find(|row| matches!(row, MappingRow::Sheet { .. }))
    else {
        panic!("expected a sheet row among {:?}", table.rows);
    };
    assert_eq!(
        sheet.bound,
        SheetBound::Unresolved {
            requested: vec!["CDImage.wav".to_string()],
        },
    );
    assert_eq!(sheet.path, tmp.path().join("CDImage.cue"));
    assert!(entries.is_empty(), "it carves nothing");
}

/// Editing a row writes the track back onto the row that commits it, found
/// by the track's own id, and leaves every other row alone.
#[test]
fn with_track_writes_the_edited_row_back_by_its_id() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");

    let files = scan(tmp.path());
    let durations = probe_durations(&files);
    let slots = slot_table(&source_tracks(2), &files, &durations);
    let table = mapping_table(
        &files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix: "import-track",
            source: TracklistSource::Release,
        }),
        &durations,
    );

    let mut edited = mapping_tracks(&table)[1].clone();
    edited.title = "Renamed".to_string();
    let table = mapping_with_track(table, edited);

    let titles: Vec<String> = mapping_tracks(&table)
        .into_iter()
        .map(|track| track.title)
        .collect();
    assert_eq!(titles, vec!["Track Title 1", "Renamed"]);
    assert_eq!(table.images[0].file_id, "cover.jpg");
    assert_eq!(
        table.reconciliation,
        Some(SlotReconciliation::Agrees { count: 2 }),
        "naming a row changes nothing about the tally",
    );
}

/// Dropping a track the folder has nothing for takes its row out and
/// restates the tally over what is left.
#[test]
fn without_track_drops_the_row_and_restates_the_tally() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");

    let files = scan(tmp.path());
    let durations = probe_durations(&files);
    let slots = slot_table(&source_tracks(3), &files, &durations);
    let table = mapping_table(
        &files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix: "import-track",
            source: TracklistSource::Release,
        }),
        &durations,
    );
    assert_eq!(
        table.reconciliation,
        Some(SlotReconciliation::MoreTracks {
            files: 2,
            tracks: 3,
        }),
    );

    let table = mapping_without_track(table, "import-track-2");

    assert_eq!(table.rows.len(), 2);
    assert_eq!(mapping_tracks(&table).len(), 2);
    assert_eq!(table.images[0].file_id, "cover.jpg");
    assert_eq!(
        table.reconciliation,
        Some(SlotReconciliation::Agrees { count: 2 }),
    );
}

/// A table with no tally keeps none through an edit: the folder's own tags
/// cannot disagree with the folder, however many rows are left.
#[test]
fn an_edit_to_a_table_with_no_tally_leaves_it_without_one() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));

    let files = scan(tmp.path());
    let durations = probe_durations(&files);
    let slots = slot_table(&source_tracks(2), &files, &durations);
    let table = mapping_table(
        &files,
        Some(PickedTracklist {
            slots: &slots,
            track_id_prefix: "unknown-track",
            source: TracklistSource::FileTags,
        }),
        &durations,
    );
    assert!(table.reconciliation.is_none());

    let table = mapping_without_track(table, "unknown-track-0");

    assert_eq!(mapping_tracks(&table).len(), 1);
    assert!(table.reconciliation.is_none());
}

/// Projecting the table reads no audio at all. The lengths come from the
/// measurements identification stored, so re-opening a candidate costs
/// nothing on disk however often it happens — which is what lets the pane
/// draw itself from a query.
#[test]
fn projecting_the_table_opens_no_audio() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("CDImage.flac"));
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.flac", 2),
    )
    .expect("write cue");
    write_flac(&tmp.path().join("bonus.flac"));

    let files = scan(tmp.path());
    let durations = probe_durations(&files);
    let opens_after_probing: Vec<u64> = ["CDImage.flac", "bonus.flac"]
        .iter()
        .map(|name| crate::audio_codec::probe_opens_for(&tmp.path().join(name)))
        .collect();

    let seed = || {
        let slots = slot_table(&source_tracks(3), &files, &durations);
        mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
            &durations,
        )
    };
    let first = seed();
    let second = seed();

    for (index, name) in ["CDImage.flac", "bonus.flac"].iter().enumerate() {
        assert_eq!(
            crate::audio_codec::probe_opens_for(&tmp.path().join(name)),
            opens_after_probing[index],
            "{name} must not be read again by projecting the table",
        );
    }
    assert_eq!(mapping_tracks(&first), mapping_tracks(&second));
}

/// JPEG magic bytes — what the scan's image validation reads.
fn fake_jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]
}
