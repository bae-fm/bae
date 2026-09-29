use super::*;
use crate::import::folder_scanner::{
    collect_release_candidate_files_with_scope, CandidateFileEdits, SheetBindingOffer,
    SheetBindingOption, SheetDiscEdits, StoredCandidateEdits,
};
use crate::import::pane::blank_candidate_draft;
use crate::import::probe::{source_durations, SourceDurations};
use crate::pressing::{made_of, Medium};
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

fn assign_discs(files: &mut CategorizedFiles, assignments: &[(&str, u32)]) {
    let mut sheet_discs = SheetDiscEdits::default();
    for (sheet_id, number) in assignments {
        sheet_discs.set((*sheet_id).to_string(), SheetDisc::Disc { number: *number });
    }
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            sheet_discs,
            ..Default::default()
        })
        .expect("disc assignments preserve a valid candidate");
}

fn mappings(table: &MappingTable) -> Vec<&TrackMapping> {
    table
        .track_sections
        .iter()
        .flat_map(MappingTrackSection::mappings)
        .collect()
}

fn track_file(mapping: &TrackMapping) -> &MappingFile {
    match mapping {
        TrackMapping {
            source: MappingSource::File(file),
            ..
        } => file,
        other => panic!("expected a file row, got {other:?}"),
    }
}

/// The table the folder's blank draft renders.
fn blank_table(files: &CategorizedFiles, durations: &SourceDurations) -> MappingTable {
    mapping_table(files, durations, &blank_candidate_draft(files), &[])
        .expect("a blank draft is drawn over its own folder")
}

/// The draft over `files` with each track's side and number as given.
fn numbered_draft(files: &CategorizedFiles, positions: &[(i32, i32)]) -> CandidateDraft {
    let mut draft = blank_candidate_draft(files);
    for (track, (side, number)) in draft.tracks.iter_mut().zip(positions) {
        track.edit.side = Some(*side);
        track.edit.track_number = *number;
    }
    draft
}

fn positions(section: &MappingTrackSection) -> Vec<&str> {
    section
        .mappings()
        .iter()
        .map(|mapping| mapping.position.as_str())
        .collect()
}

/// Every audio row is one of the draft's tracks, and a rip log is carried
/// beside them, because a role is a fact about the folder.
#[test]
fn audio_rows_are_the_draft_s_tracks_and_the_rest_are_carried() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));
    fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");
    fs::write(tmp.path().join("rip.log"), b"log").expect("write log");

    let files = scan(tmp.path());
    let table = blank_table(&files, &SourceDurations::default());
    let mappings = mappings(&table);

    assert_eq!(table.images.len(), 1);
    assert_eq!(track_file(mappings[0]).name, "01.flac");
    assert_eq!(track_file(mappings[1]).name, "02.flac");
    assert_eq!(mapping_tracks(&table), blank_candidate_draft(&files).release_edit().tracks);
    let MappingFileRow::File(file) = &table.files[0] else {
        panic!("expected a carried file, got {:?}", table.files[0]);
    };
    assert_eq!(file.name, "rip.log");
    // A row nothing has measured has no probed length to show.
    assert_eq!(track_file(mappings[0]).duration_ms, None);
    assert_eq!(track_file(mappings[0]).role, MappingRole::Audio);
    assert_eq!(file.role, MappingRole::Document);
}

/// With no release applied, a row shows its stored probe and nothing
/// disagrees with it.
#[test]
fn a_track_the_release_names_no_length_for_uses_its_stored_probe() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("01.flac"));
    let files = scan(tmp.path());
    let durations = source_durations(&files).expect("scanned fixture audio has durations");

    let table = blank_table(&files, &durations);

    let mapping = mappings(&table)[0];
    assert_eq!(mapping.duration_ms, Some(1_000));
    assert!(!mapping.lengths_disagree);
}

/// A row read from the applied release shows the length the release lists
/// for it and says when that is far from its file's; a row the release does
/// not name shows its file's.
#[test]
fn a_row_read_from_the_release_shows_the_release_s_length() {
    let tmp = tempfile::TempDir::new().unwrap();
    for number in 1..=2 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());
    let durations = source_durations(&files).unwrap();
    let mut draft = blank_candidate_draft(&files);
    draft.tracks[0].source_index = Some(0);
    let table = mapping_table(&files, &durations, &draft, &[Some(200_000)]).unwrap();
    let rows = mappings(&table);
    assert_eq!(rows[0].duration_ms, Some(200_000));
    assert!(rows[0].lengths_disagree);
    assert_eq!(track_file(rows[0]).duration_ms, Some(1_000));
    assert_eq!(rows[1].duration_ms, Some(1_000));
    assert!(!rows[1].lengths_disagree);
}

/// The folder's images are one gallery beside the table rows.
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

    let table = blank_table(&scan(tmp.path()), &SourceDurations::default());

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
    // A directory of images is not collapsed away from the gallery — its
    // files are in it, each with the path a thumbnail reads.
    assert!(table
        .images
        .iter()
        .any(|image| image.file_id == "scans/scan1.jpg" && image.path.exists()));
    assert_eq!(table.track_sections.len(), 1);
    assert_eq!(track_file(mappings(&table)[0]).name, "01.flac");
}

/// A bound sheet is one group row over its entries: the entries carry the
/// sheet's own titles and timings on the left, and on the right each is the
/// draft's track on that slice.
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
    let durations = source_durations(&files).expect("scanned fixture audio has durations");
    let mut draft = blank_candidate_draft(&files);
    for (index, track) in draft.tracks.iter_mut().enumerate() {
        track.edit.title = format!("Track Title {}", index + 1);
    }
    let table = mapping_table(&files, &durations, &draft, &[]).unwrap();

    assert_eq!(
        table.track_sections.len(),
        1,
        "the sheet is the folder's only group"
    );
    let MappingTrackSectionContent::Sheet { sheet, entries } = &table.track_sections[0].content
    else {
        panic!(
            "expected a sheet section, got {:?}",
            table.track_sections[0]
        );
    };
    assert_eq!(sheet.sheet_id, "CDImage.cue");
    assert_eq!(sheet.assignment, SheetDisc::Disc { number: 1 });
    assert_eq!(sheet.path, tmp.path().join("CDImage.cue"));
    assert_eq!(
        sheet.reference_options,
        vec![SheetReferenceOptions {
            file_reference: "CDImage.flac".into(),
            file_id: Some("CDImage.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "CDImage.flac".into(),
                offer: SheetBindingOffer::Offered,
            }],
        }]
    );
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
        // The first entries end at the next sheet boundary; the final entry
        // ends at the container duration already stored by the scan.
        assert_eq!(source.duration_ms, Some(if index < 2 { 200 } else { 600 }));
        assert_eq!(
            entry.track.file,
            Some(AudioFile::SheetSlice {
                file_id: "CDImage.flac".to_string(),
                sheet_id: "CDImage.cue".to_string(),
                index: index as u32,
            }),
        );
        // The right half is the draft's tracklist, not the sheet's.
        assert_eq!(entry.track.title, format!("Track Title {}", index + 1));
    }
}

#[test]
fn standalone_tracks_are_sectioned_by_release_side() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for number in 1..=4 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }

    let files = scan(tmp.path());
    let mut draft = numbered_draft(&files, &[(1, 1), (1, 2), (2, 1), (2, 2)]);
    draft.pressing.facts = made_of(Medium::Vinyl, 1);
    let table = mapping_table(&files, &SourceDurations::default(), &draft, &[]).unwrap();

    assert_eq!(table.track_sections.len(), 2);
    assert_eq!(
        table.track_sections[0].side,
        crate::album_detail::TrackSide::Sided {
            side_letter: "A".to_string(),
        }
    );
    assert_eq!(
        table.track_sections[1].side,
        crate::album_detail::TrackSide::Sided {
            side_letter: "B".to_string(),
        }
    );
    assert_eq!(positions(&table.track_sections[0]), ["A1", "A2"]);
    assert_eq!(positions(&table.track_sections[1]), ["B1", "B2"]);
}

#[test]
fn each_cue_is_one_section_on_its_assigned_disc() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for stem in ["alpha", "beta"] {
        write_flac(&tmp.path().join(format!("{stem}.flac")));
        fs::write(
            tmp.path().join(format!("{stem}.cue")),
            cue_sheet_text(&format!("{stem}.flac"), 2),
        )
        .expect("write cue");
    }

    let mut files = scan(tmp.path());
    assign_discs(&mut files, &[("alpha.cue", 2), ("beta.cue", 1)]);
    let mut draft = numbered_draft(&files, &[(1, 1), (1, 2), (2, 1), (2, 2)]);
    draft.pressing.facts = made_of(Medium::Cd, 2);
    let table = mapping_table(&files, &SourceDurations::default(), &draft, &[]).unwrap();

    assert_eq!(table.track_sections.len(), 2);
    for (section, (disc, sheet_id)) in table
        .track_sections
        .iter()
        .zip([(1, "beta.cue"), (2, "alpha.cue")])
    {
        assert_eq!(section.side, crate::album_detail::TrackSide::Disc { disc });
        let MappingTrackSectionContent::Sheet { sheet, entries } = &section.content else {
            panic!("a CUE disc is represented by its sheet and entries");
        };
        assert_eq!(sheet.sheet_id, sheet_id);
        assert_eq!(entries.len(), 2);
        assert_eq!(positions(section), ["1", "2"]);
    }
}

/// A release that lays one sheet's entries across two sides of a record heads
/// each side's run of them with the sheet.
#[test]
fn a_sheet_the_release_splits_across_sides_heads_each_side() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("LP.flac"));
    fs::write(tmp.path().join("LP.cue"), cue_sheet_text("LP.flac", 4)).expect("write cue");

    let files = scan(tmp.path());
    let mut draft = numbered_draft(&files, &[(1, 1), (1, 2), (2, 1), (2, 2)]);
    draft.pressing.facts = made_of(Medium::Vinyl, 1);
    let table = mapping_table(&files, &SourceDurations::default(), &draft, &[]).unwrap();

    assert_eq!(table.track_sections.len(), 2);
    for (section, letter) in table.track_sections.iter().zip(["A", "B"]) {
        assert_eq!(
            section.side,
            crate::album_detail::TrackSide::Sided {
                side_letter: letter.to_string(),
            }
        );
        let MappingTrackSectionContent::Sheet { sheet, entries } = &section.content else {
            panic!("each side's entries sit under the sheet");
        };
        assert_eq!(sheet.sheet_id, "LP.cue");
        assert_eq!(entries.len(), 2);
    }
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
    let table = blank_table(&files, &SourceDurations::default());

    let tracks = mapping_tracks(&table);
    assert_eq!(
        tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        vec!["candidate-track-0", "candidate-track-1", "candidate-track-2"],
    );
    // The cover is not a track, and the bonus file leads the sheet's two
    // slices exactly as the folder's audio units do (case-insensitive name
    // order).
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
        ],
    );
}

/// A draft that does not play the folder's audio units in order is a stored
/// state that contradicts the folder, and drawing it is refused rather than
/// pairing rows with the wrong audio.
#[test]
fn a_draft_not_drawn_over_the_folder_s_audio_is_refused() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    for number in 1..=2 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());
    let durations = SourceDurations::default();

    let mut swapped = blank_candidate_draft(&files);
    swapped.tracks.swap(0, 1);
    assert!(matches!(
        mapping_table(&files, &durations, &swapped, &[]),
        Err(ImportError::Internal { .. })
    ));

    let mut short = blank_candidate_draft(&files);
    short.tracks.pop();
    assert!(matches!(
        mapping_table(&files, &durations, &short, &[]),
        Err(ImportError::Internal { .. })
    ));

    let mut long = blank_candidate_draft(&files);
    long.tracks.push(long.tracks[0].clone());
    assert!(matches!(
        mapping_table(&files, &durations, &long, &[]),
        Err(ImportError::Internal { .. })
    ));
}

/// A sheet whose `FILE` directive names audio that is not in the folder
/// describes nothing — and says what it was looking for, so the header can
/// state it while it offers the folder's own audio instead. It also carries
/// its own path, which is what opens it in the document viewer.
#[test]
fn a_sheet_that_describes_nothing_says_what_it_asked_for() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    // Two audio files beside a one-file sheet: nothing for it to take as a
    // last resort, so it describes nothing.
    write_flac(&tmp.path().join("01.flac"));
    write_flac(&tmp.path().join("02.flac"));
    fs::write(
        tmp.path().join("CDImage.cue"),
        cue_sheet_text("CDImage.wav", 3),
    )
    .expect("write cue");

    let table = blank_table(&scan(tmp.path()), &SourceDurations::default());
    // The sheet is named where it sits on disk, after the loose audio that
    // sorts before it — a sheet that carves nothing occupies no run.
    let Some(MappingFileRow::Sheet(sheet)) = table
        .files
        .iter()
        .find(|row| matches!(row, MappingFileRow::Sheet(_)))
    else {
        panic!("expected a sheet row among {:?}", table.files);
    };
    assert_eq!(
        sheet.bound,
        SheetBound::Unresolved {
            requested: vec!["CDImage.wav".to_string()],
        },
    );
    assert_eq!(sheet.path, tmp.path().join("CDImage.cue"));
}

/// The projection keeps the two meanings of a sheet separate: one that carves
/// audio heads those track rows, while one that carves nothing remains a file
/// the user can resolve from the files section.
#[test]
fn associated_sheets_group_tracks_and_unassociated_sheets_remain_files() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 2))
        .expect("write associated cue");
    fs::write(
        tmp.path().join("unresolved.cue"),
        cue_sheet_text("absent.wav", 2),
    )
    .expect("write unassociated cue");

    let table = blank_table(&scan(tmp.path()), &SourceDurations::default());

    assert!(matches!(
        table.track_sections.as_slice(),
        [MappingTrackSection {
            content: MappingTrackSectionContent::Sheet { sheet, entries },
            ..
        }]
            if sheet.sheet_id == "disc.cue" && entries.len() == 2
    ));
    assert!(matches!(
        table.files.as_slice(),
        [MappingFileRow::Sheet(sheet)] if sheet.sheet_id == "unresolved.cue"
    ));
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
    let durations = source_durations(&files).expect("scanned fixture audio has durations");
    let opens_after_probing: Vec<u64> = ["CDImage.flac", "bonus.flac"]
        .iter()
        .map(|name| crate::audio_codec::probe_opens_for(&tmp.path().join(name)))
        .collect();

    let first = blank_table(&files, &durations);
    let second = blank_table(&files, &durations);

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

#[test]
fn invalid_cue_times_are_not_reported_as_missing_audio() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 7)).unwrap();
    let table = blank_table(&scan(tmp.path()), &SourceDurations::default());
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("the refused CUE remains listed");
    };
    assert_eq!(sheet.bound, SheetBound::RefusedTiming);
    assert_eq!(
        sheet.reference_options,
        vec![SheetReferenceOptions {
            file_reference: "disc.flac".into(),
            file_id: Some("disc.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "disc.flac".into(),
                offer: SheetBindingOffer::RefusedTiming,
            }],
        }]
    );
}

#[test]
fn a_sheet_disc_menu_keeps_the_assigned_disc_available() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 2)).unwrap();
    let mut files = scan(tmp.path());
    assign_discs(&mut files, &[("disc.cue", 2)]);
    let table = blank_table(&files, &SourceDurations::default());
    let MappingTrackSectionContent::Sheet { sheet, .. } = &table.track_sections[0].content else {
        panic!("the selected CUE groups its tracks");
    };
    assert!(sheet.disc_options.contains(&2));
}

#[test]
fn partial_sheet_keeps_assigned_and_missing_references_even_without_choices() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("first.flac"));
    fs::write(tmp.path().join("album.cue"),
        "FILE \"first.wav\" WAVE\n TRACK 01 AUDIO\n INDEX 01 00:00:00\nFILE \"absent.wav\" WAVE\n TRACK 02 AUDIO\n INDEX 01 00:00:00\n"
    ).unwrap();
    let table = blank_table(&scan(tmp.path()), &SourceDurations::default());
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("partial CUE remains listed");
    };
    assert_eq!(
        sheet.bound,
        SheetBound::Unresolved {
            requested: vec!["absent.wav".into()]
        }
    );
    assert_eq!(
        sheet.reference_options,
        vec![
            SheetReferenceOptions {
                file_reference: "first.wav".into(),
                file_id: Some("first.flac".into()),
                options: vec![SheetBindingOption {
                    file_id: "first.flac".into(),
                    offer: SheetBindingOffer::Offered
                }],
            },
            SheetReferenceOptions {
                file_reference: "absent.wav".into(),
                file_id: None,
                options: vec![]
            },
        ]
    );
}

#[test]
fn codec_refusal_does_not_invent_a_current_assignment() {
    let tmp = tempfile::TempDir::new().unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures/audio-format/placeholder-mp3.mp3"),
        tmp.path().join("disc.mp3"),
    )
    .unwrap();
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.mp3", 1)).unwrap();
    let table = blank_table(&scan(tmp.path()), &SourceDurations::default());
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("refused CUE remains listed");
    };
    assert_eq!(
        sheet.bound,
        SheetBound::RefusedCodec {
            codec: "MP3".into()
        }
    );
    assert_eq!(
        sheet.reference_options,
        vec![SheetReferenceOptions {
            file_reference: "disc.mp3".into(),
            file_id: None,
            options: vec![SheetBindingOption {
                file_id: "disc.mp3".into(),
                offer: SheetBindingOffer::RefusedCodec {
                    codec: "MP3".into()
                }
            }],
        }]
    );
}

#[test]
fn the_first_of_competing_sheets_carves_and_the_rest_stay_listed() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    for name in ["first.cue", "second.cue"] {
        fs::write(tmp.path().join(name), cue_sheet_text("disc.flac", 2)).unwrap();
    }
    let table = blank_table(&scan(tmp.path()), &SourceDurations::default());
    assert!(matches!(
        table.track_sections.as_slice(),
        [MappingTrackSection {
            content: MappingTrackSectionContent::Sheet { sheet, entries },
            ..
        }]
            if sheet.sheet_id == "first.cue" && entries.len() == 2
    ));
    // The sheet that lost the container stays listed, ignored, still bound
    // to it and still offering it, so a person can make it the one that
    // carves instead.
    let [MappingFileRow::Sheet(sheet)] = table.files.as_slice() else {
        panic!("the competing CUE remains listed: {:?}", table.files);
    };
    assert_eq!(sheet.sheet_id, "second.cue");
    assert_eq!(sheet.assignment, SheetDisc::Ignored);
    assert!(
        matches!(&sheet.bound, SheetBound::Describes(container) if container.file_id == "disc.flac")
    );
    assert_eq!(
        sheet.reference_options[0].file_id.as_deref(),
        Some("disc.flac")
    );
}

/// A CUE's slices on a vinyl pressing whose draft states no sides read as one
/// flat run: the folder's sheets do not make up sides the metadata never
/// named.
#[test]
fn source_disc_assignments_do_not_invent_sides() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 3)).unwrap();
    let files = scan(tmp.path());
    let mut draft = blank_candidate_draft(&files);
    draft.pressing.facts = made_of(Medium::Vinyl, 1);
    for track in &mut draft.tracks {
        track.edit.side = None;
    }
    let table = mapping_table(&files, &SourceDurations::default(), &draft, &[]).unwrap();
    assert!(
        table.track_sections.iter().all(|section| {
            !matches!(section.side, crate::album_detail::TrackSide::Sided { .. })
        }),
        "available CUE audio does not establish vinyl sides"
    );
    let positions: Vec<_> = mappings(&table)
        .into_iter()
        .map(|row| row.position.as_str())
        .collect();
    assert_eq!(positions, ["1", "2", "3"]);
}

/// A sheet naming two audio files heads its entries as one group, and once
/// ignored stays a listed file still describing both — all without opening
/// either file again.
#[test]
fn a_multi_file_sheet_stays_described_when_ignored_without_probing() {
    let tmp = tempfile::TempDir::new().unwrap();
    for name in ["first.flac", "second.flac"] {
        write_flac(&tmp.path().join(name));
    }
    fs::write(tmp.path().join("album.cue"),
        "FILE \"first.wav\" WAVE\n TRACK 01 AUDIO\n INDEX 01 00:00:00\n TRACK 02 AUDIO\n INDEX 01 00:00:15\nFILE \"second.wav\" WAVE\n TRACK 03 AUDIO\n INDEX 01 00:00:00\n"
    ).unwrap();
    let mut files = scan(tmp.path());
    let durations = source_durations(&files).unwrap();
    let opens = ["first.flac", "second.flac"]
        .map(|name| crate::audio_codec::probe_opens_for(&tmp.path().join(name)));
    let expected = vec![
        SheetReferenceOptions {
            file_reference: "first.wav".into(),
            file_id: Some("first.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "first.flac".into(),
                offer: SheetBindingOffer::Offered,
            }],
        },
        SheetReferenceOptions {
            file_reference: "second.wav".into(),
            file_id: Some("second.flac".into()),
            options: vec![SheetBindingOption {
                file_id: "second.flac".into(),
                offer: SheetBindingOffer::Offered,
            }],
        },
    ];
    let table = blank_table(&files, &durations);
    let MappingTrackSectionContent::Sheet { sheet, entries } = &table.track_sections[0].content
    else {
        panic!("the CUE heads its entries");
    };
    assert_eq!(
        sheet.bound,
        SheetBound::DescribesFiles {
            audio_file_count: 2
        }
    );
    assert_eq!(sheet.reference_options, expected);
    assert_eq!(entries.len(), 3);

    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("album.cue".into(), SheetDisc::Ignored);
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            sheet_discs,
            ..Default::default()
        })
        .unwrap();
    let table = blank_table(&files, &durations);
    let MappingFileRow::Sheet(sheet) = &table.files[0] else {
        panic!("ignored CUE remains a file");
    };
    assert_eq!(sheet.assignment, SheetDisc::Ignored);
    assert_eq!(
        sheet.bound,
        SheetBound::DescribesFiles {
            audio_file_count: 2
        }
    );
    assert_eq!(sheet.reference_options, expected);
    for (name, count) in ["first.flac", "second.flac"].into_iter().zip(opens) {
        assert_eq!(
            crate::audio_codec::probe_opens_for(&tmp.path().join(name)),
            count
        );
    }
}
