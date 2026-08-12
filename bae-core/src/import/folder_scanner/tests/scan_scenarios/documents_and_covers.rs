/// A folder with only a CUE and cover, no audio, yields nothing.
#[test]
fn folder_with_only_cue_and_no_audio() {
    let result = run_scenario(vec![
        FixtureEntry::File {
            rel_path: "Album/Album.cue".into(),
            kind: FileKind::CueFor {
                stem: "Album.flac",
                n_tracks: 5,
            },
        },
        FixtureEntry::File {
            rel_path: "Album/cover.jpg".into(),
            kind: FileKind::Jpeg,
        },
    ]);
    assert!(result.top_level_paths().is_empty());
}

/// A folder with audio plus `.avi` extras still produces an audio candidate.
/// surfaces, `.avi` is ignored.
#[test]
fn folder_with_audio_and_video_mixed() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/bonus.avi".into(),
        kind: FileKind::Avi,
    });
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c.files.documents().all(|d| !d.file_name.ends_with(".avi")));
    assert!(c.files.artwork().all(|a| !a.file_name.ends_with(".avi")));
}

/// A deeply nested release under a chain of single-child wrappers
/// collapses to the leaf.
#[test]
fn deeply_nested_release_scan_root_two_levels_up() {
    let result = run_scenario(flat_audio("A/B/C/Release", 3, FileKind::Flac));
    let top = result.top_level_paths();
    assert_eq!(top, vec!["A/B/C/Release"]);
    assert_eq!(result.candidate(&top[0]).name, "Release");
}

/// A folder with unexpected file types (`.xyz`, `.sh`) alongside
/// audio: candidate surfaces, extras omitted from categorization.
#[test]
fn unexpected_file_types_silently_ignored() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.extend([
        FixtureEntry::File {
            rel_path: "Album/weird.xyz".into(),
            kind: FileKind::UnrecognizedFile("xyz"),
        },
        FixtureEntry::File {
            rel_path: "Album/script.sh".into(),
            kind: FileKind::UnrecognizedFile("sh"),
        },
    ]);
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c
        .files
        .documents()
        .all(|d| !d.file_name.ends_with(".xyz") && !d.file_name.ends_with(".sh")));
    // They are still listed, under the role for what the scan doesn't
    // recognize — and the release carries them like everything else.
    let other: Vec<_> = c
        .files
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Other))
        .map(|entry| entry.file.file_name.as_str())
        .collect();
    assert_eq!(other, vec!["script.sh", "weird.xyz"]);
    assert!(
        c.files.release_files().any(|f| f.file_name == "weird.xyz"),
        "the folder is the release: an unrecognized file is carried, not dropped",
    );
}

/// Zero-byte cover art is an incompleteness signal, so the release is
/// suppressed.
#[test]
fn zero_byte_cover_art_does_not_surface() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(FixtureEntry::File {
        rel_path: "Album/cover.jpg".into(),
        kind: FileKind::ZeroByteJpeg,
    });
    let result = run_scenario(entries);
    assert!(result.top_level_paths().is_empty());
}

// ── Files carry roles ───────────────────────────────────────────────────
//
// Rooted in the folders that were broken, not in the model's own shape.

/// The walkthrough's folder: the sheet was written against a WAV that was later
/// encoded to FLAC. The directive names a file that is not here — a question,
/// not a verdict — so the folder imports, the sheet stays unbound, and the FLAC
/// keeps the audio role.
#[test]
fn sheet_naming_absent_audio_does_not_invalidate_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.wav", "Album Title"),
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    let candidate = match &items[0] {
        ScanItem::Valid(candidate) => candidate,
        ScanItem::Invalid(invalid) => {
            panic!(
                "folder must stay importable, got invalid: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    };

    let sheets: Vec<_> = candidate.files.track_sheets().collect();
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].file.file_name, "Album.cue");
    assert_eq!(
        sheets[0].binding,
        &SheetBinding::Unresolved,
        "the directive names audio that is not here",
    );
    assert_eq!(
        candidate
            .files
            .audio()
            .map(|f| f.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Album.flac"],
    );
    assert_eq!(
        candidate.files.track_count(),
        1,
        "with no sheet bound, the image is one track",
    );
}

/// Audio no sheet references is kept, not dropped: a bound sheet plus two
/// standalone files leaves all three files on the candidate, hashed and listed.
#[test]
fn audio_no_sheet_references_survives() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.flac", "Album Title"),
    )
    .unwrap();
    std::fs::write(album.join("bonus 1.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("bonus 2.flac"), fake_flac()).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(
        files
            .audio()
            .map(|f| f.relative_path.as_str())
            .collect::<Vec<_>>(),
        vec!["Album.flac", "bonus 1.flac", "bonus 2.flac"],
    );
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].audio.file_name, "Album.flac");

    let carried: Vec<_> = files
        .release_files()
        .map(|f| f.relative_path.as_str())
        .collect();
    for expected in ["Album.cue", "Album.flac", "bonus 1.flac", "bonus 2.flac"] {
        assert!(
            carried.contains(&expected),
            "{expected} must survive the scan, got {carried:?}",
        );
    }
}

/// A folder whose only disc-ID source is its rip log becomes a candidate, and
/// the log's TOC still yields the disc ID with the sheet unbound. Before roles
/// the scan refused the folder first, so the log never got the chance.
#[test]
fn folder_identifies_from_its_rip_log_with_the_sheet_unbound() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::copy(fixtures.join("test_album.log"), album.join("rip.log")).unwrap();
    std::fs::copy(
        fixtures.join("flac/01 Test Track 1.flac"),
        album.join("01 Test Track 1.flac"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("flac/02 Test Track 2.flac"),
        album.join("02 Test Track 2.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("Album.cue"),
        make_cue_content("Album.wav", "Album Title"),
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    assert!(
        matches!(&items[0], ScanItem::Valid(_)),
        "folder must be a candidate so its log can identify it",
    );

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert!(files.bound_sheets().is_empty());
    assert!(
        crate::import::discid::compute_discid_from_categorized(&files).is_some(),
        "the rip log's TOC still yields a disc ID with the sheet unbound",
    );
}

/// A sheet that will not parse is not a sheet: it lands as a document, and its
/// audio keeps its role.
#[test]
fn unparseable_sheet_lands_as_a_document() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    // No INDEX: the sheet does not parse.
    std::fs::write(
        album.join("Album.cue"),
        "FILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"Missing Index\"\n",
    )
    .unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(
                candidate
                    .files
                    .documents()
                    .any(|d| d.file_name == "Album.cue"),
                "an unparseable sheet stays a document",
            );
            assert!(candidate.files.track_sheets().next().is_none());
            assert_eq!(candidate.files.audio().count(), 1);
        }
        ScanItem::Invalid(invalid) => {
            panic!(
                "an unparseable sheet must not invalidate: {}",
                invalid.reason
            )
        }
        ScanItem::Discovered(_) | ScanItem::Boundary(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// The hash covers every file the release uploads, including audio no sheet
/// references — which the either/or silently dropped from both. This is the
/// test that fails if that omission is ever reintroduced.
#[test]
fn content_hash_covers_audio_no_sheet_references() {
    let build = |dir: &Path, with_bonus: bool| {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Album.flac"), fake_flac()).unwrap();
        std::fs::write(
            dir.join("Album.cue"),
            make_cue_content("Album.flac", "Album Title"),
        )
        .unwrap();
        if with_bonus {
            std::fs::write(dir.join("bonus.flac"), fake_flac()).unwrap();
        }
        collect_release_candidate_files_with_scope(
            dir,
            crate::import::ReleaseFileScope::Recursive,
            &StoredCandidateEdits::none(),
        )
        .expect("scan should succeed")
        .content_hash()
    };
    let tmp = tempfile::tempdir().unwrap();
    let plain = build(&tmp.path().join("Plain"), false);
    let with_bonus = build(&tmp.path().join("Bonus"), true);
    assert_ne!(
        plain, with_bonus,
        "audio no sheet references must count toward the hash",
    );
}

/// The folder is the release, so an unrecognized sidecar is carried like every
/// other file: it becomes a row the import writes, and it counts toward the
/// hash. The pair that fails if someone narrows either set back — and they must
/// stay one set, or the fingerprint stops describing the payload it identifies.
#[test]
fn an_unrecognized_sidecar_is_carried_and_hashed() {
    const SIDECARS: [&str; 5] = ["rip.accurip", "rip.ffp", "rip.md5", "rip.nfo", "rip.sfv"];

    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("Bare");
    let with_sidecars = tmp.path().join("Sidecars");
    for dir in [&bare, &with_sidecars] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("01.flac"), fake_flac()).unwrap();
    }
    for sidecar in SIDECARS {
        std::fs::write(with_sidecars.join(sidecar), b"scene notes").unwrap();
    }

    let bare_files = collect_release_candidate_files_with_scope(
        &bare,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    let sidecar_files = collect_release_candidate_files_with_scope(
        &with_sidecars,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .unwrap();
    assert_eq!(
        sidecar_files
            .files
            .iter()
            .filter(|entry| matches!(entry.role, FileRole::Other))
            .map(|entry| entry.file.file_name.as_str())
            .collect::<Vec<_>>(),
        SIDECARS.to_vec(),
        "each sidecar is listed under the role for what the scan doesn't recognize",
    );

    // The rows the import writes come from the same iterator the hash covers.
    let carried: Vec<_> = crate::import::handle::flatten_categorized_files(&sidecar_files)
        .into_iter()
        .map(|file| file.file_name)
        .collect();
    for sidecar in SIDECARS {
        assert!(
            carried.contains(&sidecar.to_string()),
            "{sidecar} must become a file row, got {carried:?}",
        );
    }

    assert_ne!(
        bare_files.content_hash(),
        sidecar_files.content_hash(),
        "a file the release carries must count toward the hash",
    );
}

/// The directive binds, not the filename: a sheet and the audio it names pair
/// even when their names have nothing in common.
#[test]
fn a_binding_survives_a_rename_of_the_sheet() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("Audio.flac"), fake_flac()).unwrap();
    std::fs::write(
        album.join("Completely Unrelated.cue"),
        make_cue_content("Audio.flac", "Album Title"),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    let bound = files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].file.file_name, "Completely Unrelated.cue");
    assert_eq!(bound[0].audio.file_name, "Audio.flac");
    assert_eq!(
        bound[0].audio.relative_path, "Audio.flac",
        "`describes` names the audio by its file id",
    );
}

/// Widening the model must not swallow real defects: audio that will not decode
/// and a folder with no audio at all still invalidate.
#[test]
fn corrupt_audio_and_empty_folders_still_invalidate() {
    let tmp = tempfile::tempdir().unwrap();
    let corrupt = tmp.path().join("Corrupt");
    std::fs::create_dir_all(&corrupt).unwrap();
    // Non-empty so the folder is still detected as a leaf, but the bytes are
    // not FLAC — the file will not decode.
    std::fs::write(corrupt.join("01.flac"), b"not a flac at all").unwrap();
    let items = scan_items(corrupt);
    assert_eq!(items.len(), 1);
    assert!(
        matches!(
            &items[0],
            ScanItem::Invalid(InvalidCandidate {
                reason: InvalidReason::CorruptAudioFile { .. },
                ..
            })
        ),
        "corrupt audio is still a real defect",
    );

    let empty = tmp.path().join("Empty");
    std::fs::create_dir_all(&empty).unwrap();
    std::fs::write(empty.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    assert!(
        scan_items(empty.clone()).is_empty(),
        "a folder with no audio has nothing to import",
    );
    assert!(
        matches!(
            collect_release_candidate_files_with_scope(
                &empty,
                crate::import::ReleaseFileScope::Recursive,
                &StoredCandidateEdits::none()
            ),
            Err(crate::import::ImportError::InvalidFolder(
                InvalidReason::NoValidAudio
            ))
        ),
        "categorizing an audio-less folder names NoValidAudio",
    );
}

/// The scan proposes one cover from the conventional filenames; every other
/// image is artwork, and both are the release's images.
#[test]
fn the_scan_proposes_one_cover_from_the_conventional_names() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    for image in ["back.jpg", "cover.jpg", "folder.jpg"] {
        std::fs::write(album.join(image), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    }

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(
        cover_names(&files),
        vec!["cover.jpg"],
        "one image leads the release — the first conventional name in file order",
    );
    assert_eq!(files.artwork().count(), 3, "all three are still images");
}

/// A release-root image outranks a nested one. Sorting by relative path puts
/// `Artwork/front.jpg` ahead of `cover.jpg`, so taking the first conventional
/// name outright would propose the file inside the subfolder.
#[test]
fn a_root_level_cover_outranks_one_in_a_subfolder() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(album.join("Artwork")).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("Artwork/front.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(album.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(cover_names(&files), vec!["cover.jpg"]);

    // With nothing at the root, the nested one leads.
    let nested_only = tmp.path().join("Nested");
    std::fs::create_dir_all(nested_only.join("Artwork")).unwrap();
    std::fs::write(nested_only.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(
        nested_only.join("Artwork/front.jpg"),
        [0xFF, 0xD8, 0xFF, 0xE0],
    )
    .unwrap();
    let files = collect_release_candidate_files_with_scope(
        &nested_only,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan should succeed");
    assert_eq!(cover_names(&files), vec!["front.jpg"]);
}
