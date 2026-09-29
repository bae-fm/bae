use super::*;

fn candidate_read(files: &CategorizedFiles) -> crate::import::CandidateAsRead {
    crate::import::CandidateAsRead {
        content_hash: files.content_hash(),
        file_edit_revision: 4,
        metadata_revision: 7,
    }
}

#[test]
fn omitted_whole_audio_stays_between_surviving_rows_with_its_viewed_revision() {
    let tmp = tempfile::TempDir::new().unwrap();
    for number in 1..=3 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());
    let durations = source_durations(&files).unwrap();
    let mut draft = crate::import::pane::blank_candidate_draft(&files);
    let removed = draft.tracks.remove(1);
    draft.tracks[0].edit.title = "Edited first track".into();
    let read = candidate_read(&files);
    let table = draft_mapping_table(&files, &durations, &draft, &read, &[]);
    let rows = mappings(&table);
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| track_file(row).name.as_str())
            .collect::<Vec<_>>(),
        ["01.flac", "02.flac", "03.flac"]
    );
    assert!(matches!(
        &rows[1].becomes,
        MappingBecomes::NotIncluded { audio, candidate }
            if audio == &removed.edit.file && candidate == &read
    ));
    assert_eq!(rows[1].duration_ms, Some(1_000));
    assert!(track_file(rows[1]).preview_target.is_some());
    assert_eq!(mapping_tracks(&table), draft.release_edit().tracks);
}

#[test]
fn all_omitted_cue_slices_keep_the_sheet_and_current_exact_audio_offers() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 3)).unwrap();
    let mut files = scan(tmp.path());
    let durations = source_durations(&files).unwrap();
    let mut draft = crate::import::pane::blank_candidate_draft(&files);
    let audio = draft
        .tracks
        .iter()
        .map(|track| track.edit.file.clone())
        .collect::<Vec<_>>();
    draft.tracks.clear();
    let read = candidate_read(&files);
    let table = draft_mapping_table(&files, &durations, &draft, &read, &[]);
    let [section] = table.track_sections.as_slice() else {
        panic!("all omitted slices must retain their selected sheet header");
    };
    let MappingTrackSectionContent::Sheet { sheet, entries } = &section.content else {
        panic!("the selected sheet remains editable");
    };
    assert_eq!(sheet.sheet_id, "disc.cue");
    assert_eq!(entries.len(), 3);
    for (row, expected) in entries.iter().zip(audio) {
        assert!(matches!(&row.becomes,
            MappingBecomes::NotIncluded { audio, candidate }
                if audio == &expected && candidate == &read));
        assert!(matches!(&row.source, MappingSource::SheetEntry(_)));
        assert!(row.duration_ms.is_some());
    }
    assert!(mapping_tracks(&table).is_empty());

    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("disc.cue".into(), SheetDisc::Ignored);
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            sheet_discs,
            ..Default::default()
        })
        .unwrap();
    let table = draft_mapping_table(&files, &durations, &draft, &candidate_read(&files), &[]);
    let rows = mappings(&table);
    assert_eq!(rows.len(), 1);
    assert!(matches!(&rows[0].becomes,
        MappingBecomes::NotIncluded { audio: AudioFile::Standalone { file_id }, .. }
            if file_id == "disc.flac"));
}

#[test]
fn unused_source_offers_do_not_reorder_existing_swapped_tracks() {
    let tmp = tempfile::TempDir::new().unwrap();
    for number in 1..=4 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());
    let durations = source_durations(&files).unwrap();
    let mut draft = crate::import::pane::blank_candidate_draft(&files);
    draft.tracks.remove(1);
    let first_audio = draft.tracks[0].edit.file.clone();
    draft.tracks[0].edit.file = draft.tracks[2].edit.file.clone();
    draft.tracks[2].edit.file = first_audio;
    let table = draft_mapping_table(&files, &durations, &draft, &candidate_read(&files), &[]);
    assert_eq!(mapping_tracks(&table), draft.release_edit().tracks);
    assert_eq!(mappings(&table).len(), 4);
    assert_eq!(track_file(mappings(&table)[0]).name, "02.flac");
}

#[test]
fn source_disc_assignments_do_not_change_included_metadata_positions() {
    let tmp = tempfile::TempDir::new().unwrap();
    write_flac(&tmp.path().join("disc.flac"));
    fs::write(tmp.path().join("disc.cue"), cue_sheet_text("disc.flac", 3)).unwrap();
    let files = scan(tmp.path());
    let mut draft = crate::import::pane::blank_candidate_draft(&files);
    draft.tracks.remove(1);
    draft.pressing.facts = crate::pressing::made_of(crate::pressing::Medium::Vinyl, 1);
    for track in &mut draft.tracks {
        track.edit.side = None;
    }
    let table = draft_mapping_table(
        &files,
        &SourceDurations::default(),
        &draft,
        &candidate_read(&files),
        &[],
    );
    let positions: Vec<_> = mappings(&table)
        .into_iter()
        .filter_map(|row| match &row.becomes {
            MappingBecomes::Track { position, .. } => Some(position.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        table.track_sections.iter().all(|section| {
            !matches!(section.side, crate::album_detail::TrackSide::Sided { .. })
        }),
        "available CUE audio does not establish vinyl sides"
    );
    assert_eq!(positions, ["1", "3"]);
    assert_eq!(mapping_tracks(&table), draft.release_edit().tracks);
}

#[test]
fn multi_file_assignments_survive_omitted_tracks_and_ignored_sheets_without_probing() {
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
    let mut draft = crate::import::pane::blank_candidate_draft(&files);
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
    for included in [3, 1, 0] {
        draft.tracks.truncate(included);
        let table = draft_mapping_table(&files, &durations, &draft, &candidate_read(&files), &[]);
        let MappingTrackSectionContent::Sheet { sheet, entries } = &table.track_sections[0].content
        else {
            panic!("the CUE header survives removed tracks");
        };
        assert_eq!(
            sheet.bound,
            SheetBound::DescribesFiles {
                audio_file_count: 2
            }
        );
        assert_eq!(sheet.reference_options, expected);
        assert_eq!(entries.len(), 3);
        assert_eq!(mapping_tracks(&table).len(), included);
    }
    let mut sheet_discs = SheetDiscEdits::default();
    sheet_discs.set("album.cue".into(), SheetDisc::Ignored);
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            sheet_discs,
            ..Default::default()
        })
        .unwrap();
    let table = draft_mapping_table(&files, &durations, &draft, &candidate_read(&files), &[]);
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

/// A row read from the applied release shows the length the release lists
/// for it, so a pairing whose two lengths disagree shows both; a row the
/// release does not name shows its file's.
#[test]
fn a_row_read_from_the_release_shows_the_release_s_length() {
    let tmp = tempfile::TempDir::new().unwrap();
    for number in 1..=2 {
        write_flac(&tmp.path().join(format!("{number:02}.flac")));
    }
    let files = scan(tmp.path());
    let durations = source_durations(&files).unwrap();
    let mut draft = crate::import::pane::blank_candidate_draft(&files);
    draft.tracks[0].source_index = Some(0);
    let table = draft_mapping_table(
        &files,
        &durations,
        &draft,
        &candidate_read(&files),
        &[Some(200_000)],
    );
    let rows = mappings(&table);
    assert_eq!(rows[0].duration_ms, Some(200_000));
    assert_eq!(track_file(rows[0]).duration_ms, Some(1_000));
    assert_eq!(rows[1].duration_ms, Some(1_000));
}
