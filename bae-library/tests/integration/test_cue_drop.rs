//! The release editor's choices to ignore a CUE sheet or bind a `FILE`
//! line to no audio. The draft shows the track changes before Save, and
//! the save adds a track for each audio file that loses all its audio.

use bae_catalogs::discogs::models::DiscogsRelease;
use bae_records::source_audio::SourceAudioLayout;

/// The 3-track CUE image fixture's release, as Discogs lists it.
fn cue_image_release() -> DiscogsRelease {
    bae_catalogs::test_releases::discogs_test_release(
        "cue-drop",
        "Test Album",
        &[("Track One", "0:10"), ("Track Two", "0:10"), ("Track Three", "0:10")],
    )
}

/// The 3-track CUE image fixture, imported.
async fn imported_cue_image(
) -> (bae_library::test_libraries::TestLibrary, bae_library::test_libraries::ImportedRelease) {
    bae_util::test_logs::install();
    bae_library::test_libraries::imported_release_setup(
        cue_image_release(),
        "test",
        coven::IdSource::new_id(&coven::UuidIds).to_string(),
        |album| {
            let fixtures = bae_audio::test_fixtures::cue_flac_dir();
            album.copy_fixture(fixtures.join("Test Album.flac"), album.join("Test Album.flac"));
            album.copy_fixture(fixtures.join("Test Album.cue"), album.join("Test Album.cue"));
        },
    )
    .await
    .expect("import the CUE image")
}

/// The draft of `form`, once it shows what `done` looks for.
async fn draft_where(
    drafts: &bae_library::LiveReleaseEditDraft,
    form: &bae_import::ReleaseEditForm,
    done: impl Fn(&bae_import::ReleaseEditDraft) -> bool,
) -> bae_import::ReleaseEditDraft {
    drafts.set(Some(form.clone())).expect("the draft takes the form");
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let draft = drafts.next().await.expect("read the draft").expect("the library holds it");
            if done(&draft) {
                return draft;
            }
        }
    })
    .await
    .expect("the draft shows the form")
}

/// The rows of the draft's track table, in order, each with its title,
/// its place and what playing it plays.
fn table_rows(
    draft: &bae_import::ReleaseEditDraft,
) -> Vec<(String, String, bae_import::TrackAudition)> {
    draft
        .sides
        .iter()
        .flatten()
        .flat_map(bae_import::TrackTableSide::rows)
        .map(|row| (row.track.title.clone(), row.position.clone(), row.audition.clone()))
        .collect()
}

/// The sheet that carves every track of the draft, from its one run.
fn sheet_of(draft: &bae_import::ReleaseEditDraft) -> bae_import::SheetGroup {
    let sides = draft.sides.as_deref().expect("the release has tracks");
    let [side] = sides else { panic!("the release has one side: {sides:?}") };
    let [bae_import::TrackTableRun::Sheet { sheet, .. }] = side.runs.as_slice() else {
        panic!("one sheet carves every track: {:?}", side.runs);
    };
    sheet.clone()
}

/// The text of the one `FILE` line of the sheet `sheet`.
fn line_of(sheet: &bae_import::SheetGroup) -> String {
    let bae_import::SheetBoundAudio::One(line) = &sheet.bound_audio else {
        panic!("the sheet has one line that plays audio: {sheet:?}")
    };
    line.file_reference.clone()
}

/// The id of the audio file the one line of `sheet` plays from.
fn sheet_audio_file(sheet: &bae_import::SheetGroup) -> String {
    let bae_import::SheetBound::Describes(played) = &sheet.bound else {
        panic!("the sheet describes one file: {sheet:?}")
    };
    played.file_id.clone()
}

/// The sheets the draft's Files list lists, by name, and the other files'
/// names.
fn files_of(draft: &bae_import::ReleaseEditDraft) -> (Vec<bae_import::SheetGroup>, Vec<String>) {
    let mut sheets = Vec::new();
    let mut names = Vec::new();
    for row in draft.files.iter().flatten() {
        match row {
            bae_import::MappingFileRow::Sheet(sheet) => sheets.push(sheet.clone()),
            bae_import::MappingFileRow::File(file) => names.push(file.name.clone()),
        }
    }
    (sheets, names)
}

#[tokio::test]
async fn ignoring_a_cue_sheet_shows_the_replacement_and_saves_it() {
    use bae_import::folder_scanner::SheetPlace;

    let (library, imported) = imported_cue_image().await;
    library
        .people()
        .set_rating(
            bae_people::collections::RatingTarget::Track(imported.track_ids[1].clone()),
            Some(bae_people::collections::Stars::Four),
        )
        .await
        .unwrap();
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let opened = draft_where(&drafts, &form, |_| true).await;
    let sheet = sheet_of(&opened);
    assert!(form.clone().with_sheet_place("missing".to_string(), SheetPlace::Ignored).is_err());
    let ignored = form.with_sheet_place(sheet.sheet_id.clone(), SheetPlace::Ignored).unwrap();
    let changed = draft_where(&drafts, &ignored, |draft| draft.changed).await;
    assert_eq!(
        table_rows(&changed),
        [("Test Album".to_string(), "1".to_string(), bae_import::TrackAudition::Unsaved)]
    );
    assert!(changed.deletes_track_data);
    let (sheets, files) = files_of(&changed);
    let [caption] = sheets.as_slice() else { panic!("one sheet in Files: {sheets:?}") };
    assert!(files.is_empty());
    assert_eq!(caption.sheet_id, sheet.sheet_id);
    assert!(caption.assigned.is_none());
    assert_eq!(sheet_audio_file(caption), sheet_audio_file(&sheet));

    let restored = ignored
        .clone()
        .with_sheet_place(
            sheet.sheet_id,
            SheetPlace::On(sheet.assigned.expect("the sheet is on a side").side),
        )
        .unwrap();
    assert!(!restored.changed());
    assert_eq!(draft_where(&drafts, &restored, |draft| !draft.changed).await, opened);

    let saved = manager.save_release_edit(ignored, bae_library::GonePick::Refuse).await.unwrap();
    assert!(!saved.changed());
    let tracks = library.records().get_tracks_for_release(&imported.release_id).await.unwrap();
    let [track] = tracks.as_slice() else { panic!("one track after Save: {tracks:?}") };
    assert!(!imported.track_ids.contains(&track.id));
    assert_eq!(track.title, "Test Album");
    let audio = library.records().get_release_audio(&imported.release_id).await.unwrap();
    let [segment] = audio.segments.as_slice() else { panic!("one audio segment: {audio:?}") };
    assert_eq!((segment.start_sample, segment.end_sample), (0, None));
    assert!(audio.carved.is_empty());
    let reopened = draft_where(&drafts, &saved, |draft| table_rows(draft).len() == 1).await;
    assert_eq!(files_of(&reopened), (vec![], vec!["Test Album.cue".to_string()]));
}

/// Binding the only line of a CUE sheet to no audio before Save shows one
/// track for the sheet's image in place of the sheet's tracks, and the sheet
/// heading the Files list on the side it was on; binding the line to the
/// file it played from leaves the form as it opened.
#[tokio::test]
async fn a_line_with_no_audio_shows_the_track_its_audio_file_gets() {
    let (library, imported) = imported_cue_image().await;
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let opened = draft_where(&drafts, &form, |_| true).await;
    let sheet = sheet_of(&opened);
    assert_eq!(table_rows(&opened).len(), 3);
    assert!(!opened.deletes_track_data);
    assert_eq!(files_of(&opened), (vec![], vec![]));

    let ignored = form.clone().with_sheet_file(sheet.sheet_id.clone(), line_of(&sheet), None);
    assert!(ignored.changed());
    let draft = draft_where(&drafts, &ignored, |draft| table_rows(draft).len() == 1).await;
    assert_eq!(
        table_rows(&draft),
        [("Test Album".to_string(), "1".to_string(), bae_import::TrackAudition::Unsaved)]
    );
    let (sheets, names) = files_of(&draft);
    let [listed] = sheets.as_slice() else { panic!("one sheet with no tracks: {sheets:?}") };
    assert_eq!(
        (listed.sheet_id.as_str(), listed.name.as_str()),
        (sheet.sheet_id.as_str(), "Test Album.cue")
    );
    assert_eq!(listed.assigned.as_ref().map(|choice| choice.side.side), Some(1));
    assert_eq!(names, Vec::<String>::new());

    let played = sheet_audio_file(&sheet);
    let back = ignored.with_sheet_file(sheet.sheet_id.clone(), line_of(&sheet), Some(played));
    assert!(!back.changed(), "binding the line back to its file leaves nothing to save");
    let restored = draft_where(&drafts, &back, |draft| table_rows(draft).len() == 3).await;
    assert_eq!(restored.sides, opened.sides);
    assert_eq!(restored.files, opened.files);
}

/// Saving a form that binds a CUE sheet's only line to no audio deletes the sheet's tracks, adds
/// one track that plays the sheet's image whole, titled after the image's
/// name, and writes the audio, loudness and place of that track; the sheet
/// stays a file of the release.
#[tokio::test]
async fn saving_a_line_with_no_audio_replaces_its_tracks_with_one_track_for_its_audio() {
    let (library, imported) = imported_cue_image().await;
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let sheet = sheet_of(&draft_where(&drafts, &form, |_| true).await);
    let image = library
        .records()
        .get_files_for_release(&imported.release_id)
        .await
        .unwrap()
        .into_iter()
        .find(|file| file.original_filename == "Test Album.flac")
        .unwrap();

    let ignored = form.with_sheet_file(sheet.sheet_id.clone(), line_of(&sheet), None);
    let saved = manager
        .save_release_edit(ignored, bae_library::GonePick::Refuse)
        .await
        .expect("save the form");
    assert!(!saved.changed(), "the saved form opens on what the save wrote");

    let records = library.records();
    let tracks = records.get_tracks_for_release(&imported.release_id).await.unwrap();
    let [track] = tracks.as_slice() else { panic!("one track: {tracks:?}") };
    assert!(!imported.track_ids.contains(&track.id), "the track is new");
    assert_eq!(track.title, "Test Album");
    assert_eq!((track.side.get(), track.track_number.get()), (1, 1));
    assert_eq!(track.duration_ms, image.source_audio.as_ref().unwrap().duration_ms);

    let audio = records.get_release_audio(&imported.release_id).await.unwrap();
    let [format] = audio.formats.as_slice() else { panic!("one format: {:?}", audio.formats) };
    assert_eq!(format.track_id, track.id);
    assert!(format.track_loudness_lufs.is_some(), "the track's loudness is measured");
    let [segment] = audio.segments.as_slice() else { panic!("one segment: {:?}", audio.segments) };
    assert_eq!(segment.file_id, image.id);
    assert_eq!((segment.start_sample, segment.end_sample), (0, None));
    assert!(audio.carved.is_empty(), "no sheet carves a track any more");
    let files = records.get_files_for_release(&imported.release_id).await.unwrap();
    let layout = files.iter().find(|file| file.id == image.id).unwrap().source_audio.clone();
    assert_eq!(layout.unwrap().layout, Some(SourceAudioLayout::File));

    let reopened = draft_where(&drafts, &saved, |_| true).await;
    assert_eq!(table_rows(&reopened).len(), 1);
    let (sheets, names) = files_of(&reopened);
    assert_eq!(sheets, vec![]);
    assert_eq!(names, ["Test Album.cue"], "the sheet is a file no track plays from");
}

/// A rating on a track the save deletes makes the draft say the save
/// deletes track data; a form that deletes nothing says it does not.
#[tokio::test]
async fn the_draft_says_when_a_save_deletes_a_rated_track() {
    let (library, imported) = imported_cue_image().await;
    library
        .people()
        .set_rating(
            bae_people::collections::RatingTarget::Track(imported.track_ids[1].clone()),
            Some(bae_people::collections::Stars::Four),
        )
        .await
        .unwrap();
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let sheet = sheet_of(&draft_where(&drafts, &form, |_| true).await);

    let unbound = form.clone().with_sheet_file(sheet.sheet_id.clone(), line_of(&sheet), None);
    let draft = draft_where(&drafts, &unbound, |draft| table_rows(draft).len() == 1).await;
    assert!(draft.deletes_track_data);
    let kept = draft_where(&drafts, &form, |draft| table_rows(draft).len() == 3).await;
    assert!(!kept.deletes_track_data);
}

/// A title the person types for the track a save adds is the title the
/// track is saved with.
#[tokio::test]
async fn the_added_track_takes_the_title_the_person_types() {
    let (library, imported) = imported_cue_image().await;
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let sheet = sheet_of(&draft_where(&drafts, &form, |_| true).await);

    let mut ignored = form.with_sheet_file(sheet.sheet_id.clone(), line_of(&sheet), None);
    let draft = draft_where(&drafts, &ignored, |draft| table_rows(draft).len() == 1).await;
    let mut row = draft
        .sides
        .iter()
        .flatten()
        .flat_map(bae_import::TrackTableSide::rows)
        .next()
        .unwrap()
        .track
        .clone();
    row.title = "Whole Album".to_string();
    ignored = ignored.with_track(row);
    let draft =
        draft_where(&drafts, &ignored, |draft| table_rows(draft)[0].0 == "Whole Album").await;
    assert_eq!(table_rows(&draft).len(), 1);

    manager.save_release_edit(ignored, bae_library::GonePick::Refuse).await.unwrap();
    let tracks = library.records().get_tracks_for_release(&imported.release_id).await.unwrap();
    assert_eq!(
        tracks.iter().map(|track| track.title.as_str()).collect::<Vec<_>>(),
        ["Whole Album"]
    );
}
