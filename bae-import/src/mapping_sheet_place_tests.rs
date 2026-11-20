//! The place pop-up of a CUE sheet's caption: the sides and discs the
//! mapping table offers a sheet, and the one it names.

use super::*;

#[tokio::test]
async fn a_sheet_place_menu_keeps_the_assigned_disc_available() {
    let tmp = TestFolder::new();
    write_flac(&tmp, "disc.flac");
    tmp.write("disc.cue", cue_sheet_text("disc.flac", 2));
    let mut files = scan(tmp.path());
    assign_discs(&mut files, &[("disc.cue", 2)]);
    let table = blank_table(&files).await;
    let [TrackTableRun::Sheet { sheet, .. }] = runs(&table).as_slice() else {
        panic!("the selected CUE groups its tracks");
    };
    assert!(sheet.side_choices.iter().any(|choice| choice.side.medium == 2));
}

/// Each resolved sheet's place menu offers every disc from one up to the
/// count of resolved sheets, and the disc each sheet is on, each named as its
/// disc.
#[tokio::test]
async fn a_sheet_place_menu_counts_up_to_the_resolved_sheets() {
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
    let table = blank_table(&files).await;
    let menus: Vec<Vec<bae_types::mediums::TrackSide>> = runs(&table)
        .into_iter()
        .map(|run| match run {
            TrackTableRun::Sheet { sheet, .. } => {
                sheet.side_choices.iter().map(|choice| choice.heading.clone()).collect()
            }
            TrackTableRun::Track(_) => panic!("each CUE groups its tracks"),
        })
        .collect();
    let discs: Vec<_> =
        [1, 2, 3, 5].map(|disc| bae_types::mediums::TrackSide::Disc { disc }).to_vec();
    assert_eq!(menus, vec![discs; 3]);
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
