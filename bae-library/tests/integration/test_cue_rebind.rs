//! The release editor's Audio file pop-up: a CUE sheet's `FILE` line bound
//! to another audio file of the release shows in the editor's draft before
//! Save, and Save reads that file and writes the sheet's tracks' audio from
//! it, as the import writes it.

use bae_catalogs::discogs::models::DiscogsRelease;
use bae_import::folder_scanner::SheetBindingOffer;
use bae_records::source_audio::SourceAudioLayout;

/// The 3-track CUE image fixture's release, as Discogs lists it.
fn cue_image_release() -> DiscogsRelease {
    bae_catalogs::test_releases::discogs_test_release(
        "cue-rebind",
        "Test Album",
        &[("Track One", "0:10"), ("Track Two", "0:10"), ("Track Three", "0:10")],
    )
}

/// The 3-track CUE image fixture, imported, with the fixture file
/// `fixture` beside it as `name`: an audio file of the release, on this
/// device only, that no track plays from, with the image's audio facts and
/// `duration_ms` as its length.
async fn imported_with_carried_audio(
    name: &str,
    fixture: &str,
    duration_ms: u64,
) -> (bae_library::test_libraries::TestLibrary, bae_library::test_libraries::ImportedRelease, String)
{
    bae_util::test_logs::install();
    let (library, imported) = bae_library::test_libraries::imported_release_setup(
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
    .expect("import the CUE image");
    let id = add_carried_audio(
        &library,
        &imported.release_id,
        &imported.album_dir,
        name,
        fixture,
        duration_ms,
        coven::Place::DeviceOnly,
    )
    .await;
    (library, imported, id)
}

/// The audio file `name` made from the fixture `fixture`, added to the
/// imported release as [`imported_with_carried_audio`] describes, placed as
/// `place` says.
async fn add_carried_audio(
    library: &bae_library::test_libraries::TestLibrary,
    release_id: &str,
    folder: &bae_util::test_files::TestFolder,
    name: &str,
    fixture: &str,
    duration_ms: u64,
    place: coven::Place,
) -> String {
    let files = library.records().get_files_for_release(release_id).await.unwrap();
    let image = files.iter().find(|file| file.original_filename == "Test Album.flac").unwrap();
    let source_audio = image.source_audio.clone().map(|audio| {
        bae_records::source_audio::SourceAudioFile { layout: None, duration_ms, ..audio }
    });
    let original =
        folder.copy_fixture(bae_audio::test_fixtures::cue_flac_dir().join(fixture), name);
    let id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    let mut row = bae_records::NewReleaseFile::new(
        release_id,
        name,
        image.content_type.clone(),
        bae_records::ReleaseFileRole::Audio,
        id.clone(),
        time::UtcDateTime::now(),
    );
    row.source_audio = source_audio;
    library.records().insert_files_placed_for_test(vec![(row, original, place)]).await.unwrap();
    id
}

/// The only run of the draft's tracks, the sheet over every track: its
/// caption, its one `FILE` line and the file that line plays from.
fn sheet_run(
    draft: &bae_import::ReleaseEditDraft,
) -> (
    &bae_import::SheetGroup,
    &bae_import::folder_scanner::SheetReferenceOptions,
    &bae_import::MappingContainer,
) {
    let sides = draft.sides.as_deref().expect("the release has tracks");
    let [side] = sides else { panic!("the release has one side: {sides:?}") };
    let [bae_import::TrackTableRun::Sheet { sheet, .. }] = side.runs.as_slice() else {
        panic!("one sheet carves every track: {:?}", side.runs);
    };
    let (bae_import::SheetBoundAudio::One(line), bae_import::SheetBound::Describes(container)) =
        (&sheet.bound_audio, &sheet.bound)
    else {
        panic!("the sheet's one line plays one file: {sheet:?}");
    };
    (sheet, line, container)
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

/// The names the draft's Files list shows.
fn listed_files(draft: &bae_import::ReleaseEditDraft) -> Vec<&str> {
    draft
        .files
        .iter()
        .flatten()
        .map(|row| match row {
            bae_import::MappingFileRow::File(file) => file.name.as_str(),
            bae_import::MappingFileRow::Sheet(sheet) => panic!("no sheet is listed: {sheet:?}"),
        })
        .collect()
}

/// A CUE image's sheet moved to another audio file of its release: the
/// editor offers the file, shows the tracks on it before Save, and Save
/// writes every track's audio from it, with nothing left of the image.
#[tokio::test]
async fn a_cue_sheet_moves_to_another_audio_file_of_its_release() {
    let (library, imported, rip) =
        imported_with_carried_audio("Test Album (rip).flac", "Test Album.flac", 30_000).await;
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let opened = draft_where(&drafts, &form, |_| true).await;
    let (sheet, line, source) = sheet_run(&opened);
    assert_eq!(
        line.options
            .iter()
            .map(|option| {
                let bound = line.bound.as_ref().map(|file| &file.file_id) == Some(&option.file_id);
                (option.name.as_str(), bound, option.offer.clone())
            })
            .collect::<Vec<_>>(),
        [
            ("Test Album (rip).flac", false, SheetBindingOffer::Offered),
            ("Test Album.flac", true, SheetBindingOffer::Offered),
        ]
    );
    assert_eq!(listed_files(&opened), ["Test Album (rip).flac"]);
    // The release's detail lists its files by the same rule: the sheet that
    // carves the tracks plays, so only the rip no track plays from is listed.
    let detail = bae_views::LibraryViews::new(library.store().clone())
        .find_db_release_detail(&imported.release_id)
        .await
        .unwrap()
        .expect("the library holds the release");
    assert_eq!(
        detail.files.others.iter().map(|file| file.original_filename.as_str()).collect::<Vec<_>>(),
        ["Test Album (rip).flac"]
    );
    assert_eq!(opened.files_to_read, None);
    let image = source.file_id.clone();

    let picked = form.with_sheet_file(
        sheet.sheet_id.clone(),
        line.file_reference.clone(),
        Some(rip.clone()),
    );
    let draft = draft_where(&drafts, &picked, |draft| sheet_run(draft).2.file_id == rip).await;
    assert_eq!(sheet_run(&draft).2.name, "Test Album (rip).flac");
    assert_eq!(listed_files(&draft), ["Test Album.flac"]);
    assert_eq!(draft.files_to_read, Some(vec!["Test Album (rip).flac".to_string()]));

    let records = library.records();
    let before = records.get_release_audio(&imported.release_id).await.unwrap();
    let album_loudness = |release: Option<bae_records::DbRelease>| {
        let release = release.expect("the library holds the release");
        (release.album_loudness_lufs, release.album_peak_linear)
    };
    let album_before =
        album_loudness(records.find_release_by_id(&imported.release_id).await.unwrap());
    let saved = manager
        .save_release_edit(picked, bae_library::GonePick::Refuse)
        .await
        .expect("save the form");
    assert!(saved.sheet_files.is_empty(), "the saved form opens on what the save wrote");
    let audio = library.records().get_release_audio(&imported.release_id).await.unwrap();
    assert_eq!(audio.formats.len(), 3);
    assert!(audio.segments.iter().all(|segment| segment.file_id == rip), "{:?}", audio.segments);
    assert_eq!(
        audio
            .segments
            .iter()
            .filter(|s| s.role == bae_records::DbAudioSegmentRole::AudioPregap)
            .count(),
        1,
        "track 2's pregap moved with it"
    );
    // The rip holds the image's bytes, so it measures as the image did.
    let loudness = |audio: &bae_records::records::sheet_files::DbReleaseAudio| {
        audio
            .formats
            .iter()
            .map(|format| (format.track_loudness_lufs, format.track_peak_linear))
            .collect::<Vec<_>>()
    };
    assert_eq!(loudness(&audio), loudness(&before));
    assert_eq!(
        album_loudness(records.find_release_by_id(&imported.release_id).await.unwrap()),
        album_before
    );
    let files = library.records().get_files_for_release(&imported.release_id).await.unwrap();
    let layout = |id: &str| {
        files.iter().find(|file| file.id == id).unwrap().source_audio.as_ref().unwrap().layout
    };
    assert_eq!((layout(&rip), layout(&image)), (Some(SourceAudioLayout::Cue), None));
    let reopened = draft_where(&drafts, &saved, |_| true).await;
    assert_eq!(sheet_run(&reopened).2.file_id, rip);
    assert_eq!(listed_files(&reopened), ["Test Album.flac"]);
}

/// An audio file that coven stores, which a read takes from its cache or the
/// cloud storage, is offered like one on this device only.
#[tokio::test]
async fn the_editor_offers_an_audio_file_the_cloud_stores() {
    bae_util::test_logs::install();
    let temp_dir = bae_util::test_files::TestFolder::new();
    let album_dir = temp_dir.path().join("album");
    temp_dir.create_folder(&album_dir);
    let fakes = bae_catalogs::fake_catalogs::FakeCatalogs::offline();
    let (library, cloud) = bae_library::test_libraries::storage_library(
        temp_dir.path(),
        fakes.providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    bae_library::test_libraries::connect_storage(library.manager(), library.store(), &cloud).await;
    let fixtures = bae_audio::test_fixtures::cue_flac_dir();
    temp_dir.copy_fixture(fixtures.join("Test Album.flac"), album_dir.join("Test Album.flac"));
    temp_dir.copy_fixture(fixtures.join("Test Album.cue"), album_dir.join("Test Album.cue"));
    let release_key = fakes.seed_discogs_test_release(cue_image_release());
    let import_handle = library.start_discogs_import().await;
    let import_id = coven::IdSource::new_id(&coven::UuidIds).to_string();
    import_handle
        .send_command(bae_import::ImportCommand {
            destination: bae_import::ImportDestination::Cloud,
            ..bae_import::test_imports::folder_import(
                &import_id,
                album_dir.clone(),
                bae_import::test_imports::discogs_release(release_key),
            )
        })
        .await
        .unwrap();
    let (release_id, _) = bae_import::test_imports::wait_for_import_complete(
        &mut import_handle.subscribe_import(import_id),
    )
    .await;
    add_carried_audio(
        &library,
        &release_id,
        &temp_dir,
        "Test Album (rip).flac",
        "Test Album.flac",
        30_000,
        coven::Place::Cloud,
    )
    .await;
    library.manager().drain_uploads_expecting_work().await.expect("the cloud takes the uploads");
    let manager = library.manager();
    let form = manager.open_release_edit(&release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let opened = draft_where(&drafts, &form, |_| true).await;
    let (_, line, _) = sheet_run(&opened);
    let names: Vec<_> = line.options.iter().map(|option| option.name.as_str()).collect();
    assert_eq!(names, ["Test Album (rip).flac", "Test Album.flac"]);
}

/// An audio file shorter than where the sheet's last track starts can't
/// back the sheet: the editor lists it refused, and a save that binds it
/// anyway is refused and writes nothing.
#[tokio::test]
async fn a_file_shorter_than_the_sheet_is_refused() {
    let (library, imported, short) = imported_with_carried_audio(
        "Short.flac",
        "02 Test Artist - Track Two (White Noise).flac",
        10_000,
    )
    .await;
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let drafts = manager.subscribe_release_edit_draft();
    let opened = draft_where(&drafts, &form, |_| true).await;
    let (sheet, line, source) = sheet_run(&opened);
    let offer = line.options.iter().find(|option| option.file_id == short).unwrap().offer.clone();
    assert_eq!(offer, SheetBindingOffer::RefusedTiming);
    let image = source.file_id.clone();

    let picked =
        form.with_sheet_file(sheet.sheet_id.clone(), line.file_reference.clone(), Some(short));
    let error = manager
        .save_release_edit(picked, bae_library::GonePick::Refuse)
        .await
        .expect_err("the save is refused");
    assert!(
        matches!(&error, bae_library::LibraryError::Import(import)
            if matches!(**import, bae_import::ImportError::SheetBinding { .. })),
        "{error:?}"
    );
    let audio = library.records().get_release_audio(&imported.release_id).await.unwrap();
    assert!(audio.segments.iter().all(|segment| segment.file_id == image));
}
