//! The release editor's commands for starting over and its Unlink, on an
//! imported CUE image: applying the files' tags reads the release's files
//! on this device, clearing empties the metadata and keeps the places and
//! records, and Unlink drops the records and keeps the release in its album.
//! Each changes the form and writes nothing until a save.

/// The 3-track CUE image fixture, imported from a Discogs release whose
/// track titles differ from the sheet's.
async fn imported_cue_image(
) -> (bae_library::test_libraries::TestLibrary, bae_library::test_libraries::ImportedRelease) {
    bae_util::test_logs::install();
    bae_library::test_libraries::imported_release_setup(
        bae_catalogs::test_releases::discogs_test_release(
            "release-edit-commands",
            "Listed Album",
            &[("Listed One", "0:10"), ("Listed Two", "0:10"), ("Listed Three", "0:10")],
        ),
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

fn titles(form: &bae_import::ReleaseEditForm) -> Vec<&str> {
    form.edit.tracks.iter().map(|track| track.title.as_str()).collect()
}

/// Applying the files' tags puts what the CUE sheet says in the form, keeps
/// the records, and a save writes it.
#[tokio::test]
async fn applying_the_files_tags_reads_the_sheet_on_this_device() {
    let (library, imported) = imported_cue_image().await;
    let manager = library.manager();
    let import = library.start_discogs_import().await;
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    assert_eq!(titles(&form), ["Listed One", "Listed Two", "Listed Three"]);

    let values = import.release_file_tag_values(&form).await.expect("read the files' tags");
    let applied = form.applying_values(values);
    assert_eq!(applied.edit.album_title, "Test Album");
    assert_eq!(
        titles(&applied),
        ["Track One (Silence)", "Track Two (White Noise)", "Track Three (Brown Noise)"]
    );
    assert_eq!(applied.records, None, "the release keeps its records");
    let records = library.records().get_release_records(&imported.release_id).await.unwrap();
    assert!(!records.is_empty());

    let saved = manager
        .save_release_edit(applied, bae_library::GonePick::Refuse)
        .await
        .expect("save the form");
    assert_eq!(
        titles(&saved),
        ["Track One (Silence)", "Track Two (White Noise)", "Track Three (Brown Noise)"]
    );
    assert_eq!(library.records().get_release_records(&imported.release_id).await.unwrap(), records);
}

/// Clearing empties the metadata, keeps the tracks' places, and leaves the
/// form unsaveable until it names an album and an artist.
#[tokio::test]
async fn clearing_empties_the_metadata_and_keeps_the_places() {
    let (library, imported) = imported_cue_image().await;
    let manager = library.manager();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let cleared = manager.clear_release_edit(form.clone()).await.expect("clear the form");
    assert_eq!(cleared.edit.album_title, "");
    assert!(cleared.edit.album_artist_assignments.is_empty());
    assert_eq!(titles(&cleared), ["", "", ""]);
    assert_eq!(cleared.records, None);
    let places = |form: &bae_import::ReleaseEditForm| {
        form.opened
            .display
            .tracks
            .iter()
            .map(|track| track.position_text.clone())
            .collect::<Vec<_>>()
    };
    let applied = cleared.applied.as_ref().expect("the cleared values are applied");
    assert_eq!(applied.tracks.len(), form.edit.tracks.len());
    assert!(applied.tracks.iter().all(|track| track.roles.is_empty() && track.works.is_empty()));
    assert!(cleared.changed());

    let error = manager
        .save_release_edit(cleared, bae_library::GonePick::Refuse)
        .await
        .expect_err("a form with no album title can't be saved");
    assert!(matches!(error, bae_library::LibraryError::Edit(_)), "{error:?}");
    let reopened = manager.open_release_edit(&imported.release_id).await.unwrap();
    assert_eq!(titles(&reopened), ["Listed One", "Listed Two", "Listed Three"]);
    assert_eq!(places(&reopened), places(&form));
}

/// Unlink drops the records in the form, the draft shows none, and a save
/// writes the release with none in the album it was in.
#[tokio::test]
async fn unlink_drops_the_records_and_keeps_the_album() {
    let (library, imported) = imported_cue_image().await;
    let manager = library.manager();
    let album_id = manager.get_album_id_for_release(&imported.release_id).await.unwrap();
    let form = manager.open_release_edit(&imported.release_id).await.unwrap();
    let unlinked = form.unlinked();
    assert!(unlinked.changed());

    let drafts = manager.subscribe_release_edit_draft();
    drafts.set(Some(unlinked.clone())).expect("the draft takes the form");
    let draft = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let draft = drafts.next().await.expect("read the draft").expect("the release");
            if !draft.has_records {
                return draft;
            }
        }
    })
    .await
    .expect("the draft shows no records");
    assert!(draft.records.is_empty());
    assert!(
        matches!(&draft.album, bae_import::DraftAlbum::Named { album_id: id, .. } if *id == album_id),
        "{:?}",
        draft.album
    );

    let saved = manager
        .save_release_edit(unlinked, bae_library::GonePick::Refuse)
        .await
        .expect("save the form");
    assert_eq!(titles(&saved), ["Listed One", "Listed Two", "Listed Three"]);
    assert!(library.records().get_release_records(&imported.release_id).await.unwrap().is_empty());
    assert_eq!(manager.get_album_id_for_release(&imported.release_id).await.unwrap(), album_id);
}
