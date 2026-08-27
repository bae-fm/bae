// The pane's own controls, from the handle down to the next read.
//
// Every one of these writes a row and returns; nothing is handed back to the
// caller. So what each test asserts is what the pane would draw next — the
// `candidate_pane` read — because that is the only path the UI has to what it
// just wrote.

/// A watched root holding one folder of real audio and one image, scanned into
/// the tables and picked as its own tags.
///
/// The folder is stored rather than scanned for: the pane's writes are what is
/// under test, and a real scan would only make when they run less certain. The
/// files themselves are real — `probe_candidate_durations` opens them.
async fn picked_candidate(
    manager: &LibraryManager,
    tmp: &TempDir,
) -> (FolderCandidate, String, String) {
    use crate::import::folder_scanner::{
        CandidateFile, CategorizedFiles, FileRole, ReleaseFileScope, ScannedFile,
    };

    let root = tmp.path().join("watched");
    let folder = root.join("Album");
    std::fs::create_dir_all(&folder).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac");
    let mut files = Vec::new();
    for (index, name) in ["01 Test Track 1.flac", "02 Test Track 2.flac"]
        .into_iter()
        .enumerate()
    {
        let relative_path = format!("{:02} Track.flac", index + 1);
        let path = folder.join(&relative_path);
        std::fs::copy(fixtures.join(name), &path).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();
        files.push(CandidateFile {
            proposed_audio: true,
            file: ScannedFile::new(path, relative_path, size),
            role: FileRole::Audio,
        });
    }
    let cover_path = folder.join("cover.jpg");
    std::fs::write(&cover_path, [0xFF, 0xD8, 0xFF, 0xE0, 0x00]).unwrap();
    files.push(CandidateFile {
        proposed_audio: false,
        file: ScannedFile::new(cover_path, "cover.jpg".to_string(), 5),
        role: FileRole::Artwork,
    });

    let candidate = FolderCandidate {
        path: folder.clone(),
        file_root: folder.clone(),
        name: "Album".to_string(),
        files: CategorizedFiles {
            files,
            format_label: "FLAC".to_string(),
        },
        watched_folder_path: root.to_string_lossy().into_owned(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: "Album".to_string(),
        resolved_boundaries: Vec::new(),
        combine_ancestor_key: None,
    };

    let root = root.to_string_lossy().into_owned();
    manager.add_watched_import_folder(&root).await.unwrap();
    let generation = manager.begin_folder_scan(&root).await.unwrap();
    manager
        .save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate.clone()),
        )
        .await
        .unwrap();
    manager
        .finish_folder_scan(&root, generation, None)
        .await
        .unwrap();

    let key = folder.to_string_lossy().into_owned();
    let hash = candidate.files.content_hash();
    (candidate, key, hash)
}

/// The handle, the key its controls address, and the hash its rows are stored
/// under — with the folder's own tags already picked, which is what draws the
/// edit form and the mapping table.
async fn pane_fixture() -> (ImportServiceHandle, TempDir, String, String) {
    let (manager, tmp) = setup_test_manager().await;
    let (candidate, key, hash) = picked_candidate(&manager, &tmp).await;
    manager
        .save_candidate_metadata_seed(
            &hash,
            &candidate.path.to_string_lossy(),
            &crate::import::MetadataSeed::FileTags,
        )
        .await
        .unwrap();
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();
    (handle, tmp, key, hash)
}

async fn pane(handle: &ImportServiceHandle, key: &str) -> crate::import::ImportCandidateDetail {
    handle
        .candidate_pane(key)
        .await
        .unwrap()
        .expect("the picked candidate reads back")
}

async fn shut_down(handle: ImportServiceHandle) {
    tokio::task::spawn_blocking(move || handle.stop_and_join())
        .await
        .unwrap();
}

/// The rows of the table that become tracks, in order — what a person edits.
fn track_rows(table: &crate::import::MappingTable) -> Vec<crate::import::RawTrackEdit> {
    use crate::import::mapping::{MappingBecomes, MappingRow};
    table
        .rows
        .iter()
        .flat_map(MappingRow::units)
        .filter_map(|unit| match &unit.becomes {
            MappingBecomes::Track { track, .. } => Some(track.clone()),
            _ => None,
        })
        .collect()
}

/// What every audio row of the table says its file plays for.
fn probed_lengths(table: &crate::import::MappingTable) -> Vec<Option<u64>> {
    use crate::import::mapping::{MappingRow, MappingSource};
    table
        .rows
        .iter()
        .flat_map(MappingRow::units)
        .filter_map(|unit| match &unit.source {
            MappingSource::File(file) => Some(file.probed_duration_ms),
            _ => None,
        })
        .collect()
}

/// A typed field replaces that one field of the form and leaves the rest to
/// the pick. Committing it empty is the person clearing the field, not undoing
/// their edit: the blank is stored and the form comes back blank.
#[tokio::test(flavor = "multi_thread")]
async fn a_typed_field_lands_in_the_next_form_empty_included() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let seeded = pane(&handle, &key).await;
    let seeded_artists = seeded
        .edit
        .expect("a pick draws the form")
        .album_artist_assignments;

    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            "Typed Title".to_string(),
        )
        .await
        .unwrap();

    let edited = pane(&handle, &key).await.edit.unwrap();
    assert_eq!(edited.album_title, "Typed Title");
    assert_eq!(
        edited.album_artist_assignments, seeded_artists,
        "nothing else moved with it"
    );

    handle
        .set_candidate_edit_field(
            &key,
            crate::import::CandidateEditField::AlbumTitle,
            String::new(),
        )
        .await
        .unwrap();

    assert_eq!(pane(&handle, &key).await.edit.unwrap().album_title, "");
    let stored = handle
        .library_manager
        .load_import_candidate_pane_rows(&hash)
        .await
        .unwrap();
    assert_eq!(
        stored.edit.album_title,
        Some(String::new()),
        "a cleared field is a value the person set, not an absent edit"
    );

    shut_down(handle).await;
}

/// The tables the pane writes hang off the candidate's state row, and a pick
/// is what writes that row. An edit typed before anything is picked is refused
/// rather than stored where nothing would read it.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_with_nothing_picked_is_refused() {
    let (manager, tmp) = setup_test_manager().await;
    let (_candidate, key, _hash) = picked_candidate(&manager, &tmp).await;
    let handle = manager
        .start_import_service(tokio::runtime::Handle::current())
        .await
        .unwrap();

    let error = handle
        .set_candidate_edit_field(&key, crate::import::CandidateEditField::Year, "1991".into())
        .await
        .expect_err("no pick, no row to hang the edit off");
    assert!(
        error.to_string().contains("no candidate state row"),
        "the refusal names what is missing: {error}"
    );

    shut_down(handle).await;
}

/// An edited row comes back edited and its neighbours come back untouched.
#[tokio::test(flavor = "multi_thread")]
async fn an_edited_track_row_redraws_alone() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.mapping);
    let first = before.first().expect("the folder names tracks").clone();
    let untouched = before[1].title.clone();

    handle
        .set_candidate_track_edit(
            &key,
            crate::import::RawTrackEdit {
                title: "Renamed".to_string(),
                artist_assignments: crate::import::TrackArtistAssignments::Explicit(vec![
                    crate::import::ArtistAssignment::new("Someone"),
                ]),
                ..first.clone()
            },
        )
        .await
        .unwrap();

    let after = track_rows(&pane(&handle, &key).await.mapping);
    assert_eq!(after.len(), before.len());
    assert_eq!(after[0].title, "Renamed");
    assert_eq!(
        after[0].artist_assignments,
        crate::import::TrackArtistAssignments::Explicit(vec![
            crate::import::ArtistAssignment::new("Someone")
        ])
    );
    assert_eq!(
        after[0].file, first.file,
        "the audio the row was bound to rides through the edit"
    );
    assert_eq!(after[1].title, untouched);

    shut_down(handle).await;
}

/// A dropped row leaves the table: the release commits without that track.
#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_track_leaves_the_table() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let before = track_rows(&pane(&handle, &key).await.mapping);
    let dropped = before[0].id.clone();
    let kept = before[1].id.clone();

    handle
        .drop_candidate_track(&key, dropped.clone())
        .await
        .unwrap();

    let after = track_rows(&pane(&handle, &key).await.mapping);
    assert!(
        !after.iter().any(|track| track.id == dropped),
        "the dropped row is gone"
    );
    assert!(after.iter().any(|track| track.id == kept));

    shut_down(handle).await;
}

/// A chosen cover comes back as the image on disk it names.
#[tokio::test(flavor = "multi_thread")]
async fn a_chosen_cover_comes_back_pointed_at_the_file() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert!(
        pane(&handle, &key).await.cover.is_none(),
        "the folder's own tags offer no cover until one is chosen"
    );

    handle
        .set_candidate_cover(
            &key,
            crate::import::CoverSelection::Local("cover.jpg".to_string()),
        )
        .await
        .unwrap();

    let cover = pane(&handle, &key)
        .await
        .cover
        .expect("the choice is what the pane draws");
    assert_eq!(
        cover.selection,
        crate::import::CoverSelection::Local("cover.jpg".to_string())
    );
    let crate::import::cover_art::CoverImageSource::Local { path } = cover.preview else {
        panic!("a folder image is drawn from disk, not fetched");
    };
    assert!(path.ends_with("cover.jpg"));

    shut_down(handle).await;
}

/// The pane asks for the units nothing has measured, and asks once: probing
/// writes a row per unit, the next read wants nothing, and a second probe of
/// the same units replaces those rows rather than adding to them.
#[tokio::test(flavor = "multi_thread")]
async fn probing_answers_the_pane_and_stops_it_asking() {
    let (handle, _tmp, key, hash) = pane_fixture().await;
    let wanted = pane(&handle, &key).await.unprobed;
    assert_eq!(
        wanted.len(),
        2,
        "nothing has opened either file, so the pane asks for both"
    );

    handle
        .probe_candidate_durations(&key, wanted.clone())
        .await
        .unwrap();

    let after = pane(&handle, &key).await;
    assert!(
        after.unprobed.is_empty(),
        "the rows are what ends the asking"
    );
    assert!(
        probed_lengths(&after.mapping)
            .iter()
            .all(Option::is_some),
        "and the table draws what they say"
    );

    let stored = handle
        .library_manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .expect("the pick wrote the state row");
    assert_eq!(stored.durations.units.len(), 2);

    handle.probe_candidate_durations(&key, wanted).await.unwrap();
    let again = handle
        .library_manager
        .load_import_candidate_state(&hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        again.durations.units, stored.durations.units,
        "probing the same units again replaces the rows, it does not stack them"
    );

    shut_down(handle).await;
}
