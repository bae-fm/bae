//! The state a scan creates for the files it finds.

use crate::tables::import_state::tests::*;
use crate::tables::tests::*;
use crate::tables::watched_path::host_root;
use bae_store::test_fixtures::empty_db;

/// A candidate discovery stored with a blank draft: nobody wrote it.
#[tokio::test]
async fn a_candidate_with_no_pick_has_no_author() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let files = track_files_candidate(&[("01 Track.flac", 111), ("02 Track.flac", 222)]);
    let hash = store_candidate_state(&db, &files, &host_root("/music/Album")).await;

    let loaded = tables.load_import_candidate_states().await.unwrap();
    assert_eq!(
        loaded.get(&hash).expect("the candidate reads back").metadata_author,
        crate::MetadataAuthor::Nobody
    );
}

/// A candidate's state names the copy of its files scanned last. Removing
/// that copy's watched folder, while another watched folder holds a copy,
/// moves the state to the copy that stays in the same write.
#[tokio::test]
async fn removing_the_named_copy_s_folder_names_the_copy_that_stays() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let first = host_root("/mounted/first");
    let second = host_root("/mounted/second");
    for root in [&first, &second] {
        tables.add_watched_import_folder(root).await.unwrap();
    }
    let second_candidate = scanned_candidate(&second, "Release");
    let first_candidate = scanned_candidate(&first, "Release");
    for (root, candidate) in [(&second, &second_candidate), (&first, &first_candidate)] {
        let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
        tables.save_folder_scan_item(root, generation, candidate).await.unwrap();
        tables.finish_folder_scan(root, generation, None).await.unwrap();
    }
    let crate::folder_scanner::ScanItem::Valid(candidate) = &first_candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = candidate.files.content_hash();
    let named = |state: Option<crate::tables::DbImportCandidateState>| {
        state.expect("the state stays with a copy").candidate_key
    };
    assert_eq!(
        named(tables.load_import_candidate_state(&content_hash).await.unwrap()),
        first_candidate.key()
    );

    tables.remove_watched_import_folders(vec![first.clone()], None).await.unwrap();

    assert_eq!(
        named(tables.load_import_candidate_state(&content_hash).await.unwrap()),
        second_candidate.key()
    );
}
/// A copy the scan reads again as other files leaves its state the same way:
/// the state names the copy that still holds them.
#[tokio::test]
async fn a_named_copy_read_as_other_files_names_the_copy_that_stays() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let first = host_root("/mounted/first");
    let second = host_root("/mounted/second");
    for root in [&first, &second] {
        tables.add_watched_import_folder(root).await.unwrap();
    }
    let second_candidate = scanned_candidate(&second, "Release");
    let first_candidate = scanned_candidate(&first, "Release");
    for (root, candidate) in [(&second, &second_candidate), (&first, &first_candidate)] {
        let generation = tables.begin_folder_scan(root, crate::VolumeKind::Local).await.unwrap();
        tables.save_folder_scan_item(root, generation, candidate).await.unwrap();
        tables.finish_folder_scan(root, generation, None).await.unwrap();
    }
    let crate::folder_scanner::ScanItem::Valid(candidate) = &first_candidate else {
        panic!("the fixture must produce a valid candidate");
    };
    let content_hash = candidate.files.content_hash();
    let mut changed = first_candidate.clone();
    let crate::folder_scanner::ScanItem::Valid(release) = &mut changed else {
        unreachable!("the fixture creates a valid candidate");
    };
    release.files.files[0].file.version.size += 1;

    let generation = tables.begin_folder_scan(&first, crate::VolumeKind::Local).await.unwrap();
    tables.save_folder_scan_item(&first, generation, &changed).await.unwrap();

    let state = tables.load_import_candidate_state(&content_hash).await.unwrap();
    assert_eq!(state.expect("the state stays with a copy").candidate_key, second_candidate.key());
}

/// Writes a loose bonus track, a disc file that its cue sheet splits into two
/// tracks, and a disc file split into three.
fn bonus_and_two_discs(folder: &std::path::Path) {
    use crate::mapping::tests::{cue_sheet_text, write_flac};
    write_flac(&folder.join("a bonus.flac"));
    write_flac(&folder.join("disc 1.flac"));
    std::fs::write(folder.join("disc 1.cue"), cue_sheet_text("disc 1.flac", 2)).unwrap();
    write_flac(&folder.join("disc 2.flac"));
    std::fs::write(folder.join("disc 2.cue"), cue_sheet_text("disc 2.flac", 3)).unwrap();
}

/// The scan stores a folder's draft from its stored audio units: one medium
/// per disc the files state, loose audio (no disc) first, each track on its
/// disc's medium, numbered as its unit is and holding its unit's audio. The
/// blank draft a write starts over from is the same draft.
#[tokio::test]
async fn blank_draft_statement_puts_each_disc_on_its_own_medium_loose_audio_first() {
    use crate::tables::folder_scans::tests::stored_folder;
    use crate::AudioFile;
    let (db, _tmp, root) = watched_root().await;
    let hash = stored_folder(&db, &root, "Album", bonus_and_two_discs).await;
    let tables = crate::tables::ImportTables::new(db.clone());
    let slice = |file: &str, sheet: &str, index| AudioFile::SheetSlice {
        file_id: file.to_string(),
        sheet_id: sheet.to_string(),
        index,
    };
    let expected = [
        (1, 1, AudioFile::Standalone { file_id: "a bonus.flac".to_string() }),
        (2, 2, slice("disc 1.flac", "disc 1.cue", 0)),
        (2, 3, slice("disc 1.flac", "disc 1.cue", 1)),
        (3, 4, slice("disc 2.flac", "disc 2.cue", 0)),
        (3, 5, slice("disc 2.flac", "disc 2.cue", 1)),
        (3, 6, slice("disc 2.flac", "disc 2.cue", 2)),
    ];
    let assert_blank = |draft: &crate::CandidateDraft| {
        assert_eq!(draft.album_title, "");
        assert_eq!(
            draft
                .tracks
                .iter()
                .map(|track| (
                    track.place.medium.get(),
                    track.place.number.get(),
                    track.edit.file.clone()
                ))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(draft.tracks.iter().all(|track| track.edit.title.is_empty()));
        assert_eq!(draft.mediums, [bae_types::mediums::ReleaseMedium::one_sided(None); 3]);
    };

    let stored = tables.load_import_candidate_pane_rows(&hash).await.unwrap();
    assert_blank(&stored.draft);
    assert_blank(
        &tables.load_blank_draft(&hash).await.unwrap().expect("the folder has a blank draft"),
    );
    assert_eq!((stored.draft_blank, stored.draft_valid), (true, false));
}
