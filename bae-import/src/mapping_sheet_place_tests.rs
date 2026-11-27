//! The place pop-up of a CUE sheet's caption: the sides and discs the
//! mapping table offers a sheet, and the one it names; and what the table
//! shows of audio the draft leaves out.

use super::*;

/// A sheet's place menu offers the draft's discs and no other, whatever
/// discs the files' decisions name, and names the disc its tracks are on.
#[tokio::test]
async fn a_sheet_place_menu_offers_the_drafts_discs_alone() {
    let tmp = TestFolder::new();
    for stem in ["alpha", "beta", "gamma"] {
        write_flac(&tmp, format!("{stem}.flac"));
        tmp.write(
            tmp.path().join(format!("{stem}.cue")),
            cue_sheet_text(&format!("{stem}.flac"), 2),
        );
    }
    let mut files = scan(tmp.path());
    assign_discs(&mut files, &[("alpha.cue", 1), ("beta.cue", 2), ("gamma.cue", 5)]);
    let disc = || bae_types::mediums::ReleaseMedium::one_sided(None);
    // A source of two discs, the second holding both later sheets' tracks.
    let draft = numbered_draft(
        &files,
        vec![disc(), disc()],
        &[(1, 1, 1), (1, 1, 2), (2, 1, 1), (2, 1, 2), (2, 1, 3), (2, 1, 4)],
    );
    let table = mapping_table(&files, &draft).await.unwrap();
    let discs: Vec<_> = [1, 2].map(|disc| bae_types::mediums::TrackSide::Disc { disc }).to_vec();
    let sheets: Vec<_> = runs(&table)
        .into_iter()
        .map(|run| match run {
            TrackTableRun::Sheet { sheet, .. } => (
                sheet.side_choices.iter().map(|choice| choice.heading.clone()).collect::<Vec<_>>(),
                sheet.assigned.as_ref().map(|choice| choice.side.medium),
            ),
            TrackTableRun::Track(_) => panic!("each CUE groups its tracks"),
        })
        .collect();
    assert_eq!(sheets, [(discs.clone(), Some(1)), (discs.clone(), Some(2)), (discs, Some(2))]);
}

/// A draft that leaves audio out of the import draws the rows it has, and its
/// Files list leaves that audio out too: the release has no such file.
#[tokio::test]
async fn audio_a_draft_leaves_out_has_no_row_and_no_place_in_the_files_list() {
    let tmp = TestFolder::new();
    for number in 1..=3 {
        write_flac(&tmp, format!("{number:02}.flac"));
    }
    tmp.write("rip.log", "a rip log");
    let files = scan(tmp.path());

    let mut draft = blank_candidate_draft(&files);
    draft.tracks.remove(1);
    let table = mapping_table(&files, &draft).await.unwrap();

    assert_eq!(mapping_tracks(&table), draft.release_edit().tracks);
    assert_eq!(table.track_count, 2);
    assert_eq!(
        table_files(&table)
            .iter()
            .map(|row| match row {
                MappingFileRow::File(file) => file.name.clone(),
                MappingFileRow::Sheet(sheet) => sheet.name.clone(),
            })
            .collect::<Vec<_>>(),
        ["rip.log"]
    );
}

/// The pane names the audio no draft track reads. A CUE track reads the file
/// it plays from and the file its pregap is in, so a draft of the second
/// track alone leaves nothing out, and one of the first leaves out the second
/// file.
#[tokio::test]
async fn the_audio_a_draft_leaves_out_keeps_the_file_a_pregap_is_in() {
    let tmp = TestFolder::new();
    write_flac(&tmp, "01.flac");
    write_flac(&tmp, "02.flac");
    tmp.write(
        tmp.path().join("album.cue"),
        concat!(
            "PERFORMER \"Artist Name\"\n",
            "TITLE \"Album Title\"\n",
            "FILE \"01.flac\" WAVE\n",
            "  TRACK 01 AUDIO\n",
            "    INDEX 01 00:00:00\n",
            "  TRACK 02 AUDIO\n",
            "    INDEX 00 00:00:60\n",
            "FILE \"02.flac\" WAVE\n",
            "    INDEX 01 00:00:00\n",
        ),
    );
    let files = scan(tmp.path());
    let folder = StoredFolder::of(&files).await;
    for (kept, left_out) in [(1, None), (0, Some("02.flac"))] {
        let mut draft = blank_candidate_draft(&files);
        draft.tracks = vec![draft.tracks.swap_remove(kept)];
        folder.table(&draft).await.unwrap();
        assert_eq!(
            folder.tables.load_left_out_audio_for_test(&folder.key).await.unwrap(),
            left_out.map(|file: &str| crate::LeftOutAudio {
                file_count: 1,
                files: vec![file.to_string()],
            })
        );
    }
}

/// A draft track that plays no audio unit of the folder is a broken draft:
/// the import's read refuses it instead of counting its file as left out.
#[tokio::test]
async fn the_import_s_read_refuses_a_draft_track_with_no_audio_unit() {
    let tmp = TestFolder::new();
    write_flac(&tmp, "01.flac");
    let files = scan(tmp.path());
    let folder = StoredFolder::of(&files).await;
    let mut draft = blank_candidate_draft(&files);
    draft.tracks[0].edit.file =
        crate::types::AudioFile::Standalone { file_id: "02.flac".to_string() };
    folder.table(&draft).await.unwrap_err();

    folder.tables.load_import_preparation(&folder.hash, &folder.key).await.unwrap_err();
}

/// A sheet whose tracks the draft leaves out heads its line of the Files list
/// on none of the draft's discs, which its menu still offers.
#[tokio::test]
async fn a_sheet_the_draft_leaves_out_names_no_disc() {
    let tmp = TestFolder::new();
    for stem in ["alpha", "beta"] {
        write_flac(&tmp, format!("{stem}.flac"));
        tmp.write(
            tmp.path().join(format!("{stem}.cue")),
            cue_sheet_text(&format!("{stem}.flac"), 2),
        );
    }
    let files = scan(tmp.path());
    let mut draft = blank_candidate_draft(&files);
    draft.mediums.truncate(1);
    draft.tracks.truncate(2);
    let table = mapping_table(&files, &draft).await.unwrap();
    let [MappingFileRow::Sheet(sheet)] = table_files(&table) else {
        panic!("the left-out sheet is the Files list's one line: {:?}", table_files(&table));
    };
    assert_eq!(sheet.sheet_id, "beta.cue");
    assert_eq!(sheet.assigned, None);
    assert_eq!(sheet.side_choices.iter().map(|choice| choice.side.medium).collect::<Vec<_>>(), [1]);
}

/// A sheet on a record offers each of the record's sides, lettered as the
/// table heads them, and its caption names the side it is on.
#[tokio::test]
async fn a_sheet_place_menu_offers_each_side_of_a_record() {
    use bae_types::mediums::{MediumSide, TrackSide};
    let tmp = TestFolder::new();
    write_flac(&tmp, "disc.flac");
    tmp.write("disc.cue", cue_sheet_text("disc.flac", 2));
    let mut files = scan(tmp.path());
    let mut sheet_places = SheetPlaceEdits::default();
    sheet_places.set("disc.cue".to_string(), SheetPlace::On(MediumSide { medium: 1, side: 2 }));
    files
        .apply_candidate_file_edits(&CandidateFileEdits { sheet_places, ..Default::default() })
        .expect("the place preserves a valid candidate");
    let draft = numbered_draft(&files, record(), &[(1, 2, 1), (1, 2, 2)]);
    let table = mapping_table(&files, &draft).await.unwrap();
    let [TrackTableRun::Sheet { sheet, .. }] = runs(&table).as_slice() else {
        panic!("the selected CUE groups its tracks");
    };
    let sided = |letter: &str| TrackSide::Sided { side_letter: letter.to_string() };
    assert_eq!(
        sheet
            .side_choices
            .iter()
            .map(|choice| (choice.side, choice.heading.clone()))
            .collect::<Vec<_>>(),
        [
            (MediumSide { medium: 1, side: 1 }, sided("A")),
            (MediumSide { medium: 1, side: 2 }, sided("B")),
        ]
    );
    assert_eq!(
        sheet.assigned.as_ref().map(|choice| (choice.side, choice.heading.clone())),
        Some((MediumSide { medium: 1, side: 2 }, sided("B")))
    );
}

/// A sheet stored on the second side of a disc that is not a record names
/// the disc, whose sides share its heading.
#[tokio::test]
async fn a_sheet_on_a_later_side_of_a_disc_names_the_disc() {
    use bae_types::mediums::MediumSide;
    let tmp = TestFolder::new();
    write_flac(&tmp, "disc.flac");
    tmp.write("disc.cue", cue_sheet_text("disc.flac", 2));
    let mut files = scan(tmp.path());
    let mut sheet_places = SheetPlaceEdits::default();
    sheet_places.set("disc.cue".to_string(), SheetPlace::On(MediumSide { medium: 1, side: 2 }));
    files
        .apply_candidate_file_edits(&CandidateFileEdits { sheet_places, ..Default::default() })
        .expect("the place preserves a valid candidate");
    let table = blank_table(&files).await;
    let [TrackTableRun::Sheet { sheet, .. }] = runs(&table).as_slice() else {
        panic!("the selected CUE groups its tracks");
    };
    assert_eq!(
        sheet.assigned.as_ref().map(|choice| (choice.side, choice.heading.clone())),
        Some((MediumSide::first_of(1), bae_types::mediums::TrackSide::Disc { disc: 1 }))
    );
    assert!(sheet.side_choices.iter().any(|choice| choice.side == MediumSide::first_of(1)));
}
