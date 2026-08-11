/// File names of every image the scan proposed as the release's cover.
fn cover_names(files: &CategorizedFiles) -> Vec<&str> {
    files
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Cover))
        .map(|entry| entry.file.file_name.as_str())
        .collect()
}

// ── The sheet↔audio binding is a user decision ──────────────────────────
//
// The scan proposes; these pin what happens when the user overrules it.

/// The walkthrough folder, one step on from the roles task: a twelve-track
/// sheet written against a WAV, the FLAC it was actually encoded to, and the
/// rip log. Unbound it imports as one track; bound, the slot count comes from
/// the sheet and the disc ID becomes computable from sheet plus audio.
///
/// This is the task's whole point — the information needed to fix the folder
/// was on screen all along and the app had no way to accept it.
#[test]
fn binding_a_sheet_whose_directive_missed_makes_the_folder_a_twelve_track_disc() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/test_album.log"),
        album.join("rip.log"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 12),
    )
    .unwrap();

    let unbound = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    assert_eq!(
        unbound.track_count(),
        1,
        "the directive names a file that is not here, so the image is one track",
    );
    assert_eq!(unbound.format_label, "FLAC");

    let bound = scan_with_binding(&album, &unbound, "cd.cue", Some("cd.flac"));

    assert_eq!(
        bound.track_count(),
        12,
        "bound, the slot count comes from the sheet rather than the file",
    );
    assert_eq!(
        bound.format_label, "CUE+FLAC",
        "the label follows the probed codec of the audio the user named",
    );
    let sheets: Vec<_> = bound.track_sheets().collect();
    assert_eq!(
        sheets[0].binding,
        &SheetBinding::Describes {
            file_id: "cd.flac".to_string()
        },
    );
    assert_eq!(bound.bound_sheets()[0].audio.file_name, "cd.flac");
    assert!(
        crate::import::discid::compute_discid_from_categorized(&bound).is_some(),
        "a bound sheet plus its audio yields a disc ID",
    );
}

/// A codec the CUE path cannot seek inside is refused where the choice is
/// offered, with the codec named — never handed to the user as a choice that
/// fails at commit. The FLAC beside it stays offerable, so this is the refusal
/// and not an empty picker.
#[test]
fn audio_a_sheet_cannot_use_is_refused_at_offer_time_with_the_codec_named() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(
        fixtures.join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        fixtures.join("test-fixtures/audio-format/placeholder-mp3.mp3"),
        album.join("cd.mp3"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 12),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let options = files.sheet_binding_options("cd.cue");

    assert_eq!(
        options,
        vec![
            SheetBindingOption {
                file_id: "cd.flac".to_string(),
                offer: SheetBindingOffer::Offered,
            },
            SheetBindingOption {
                file_id: "cd.mp3".to_string(),
                offer: SheetBindingOffer::RefusedCodec {
                    codec: "MP3".to_string()
                },
            },
        ],
        "the MP3 is refused with its codec named, not offered and rejected later",
    );
}

/// Clearing a binding leaves the sheet describing nothing. It does **not**
/// restore the scan's proposal: someone who cleared a binding is saying the
/// guess was wrong, and re-guessing it is the one answer that is certainly not
/// what they asked for.
#[test]
fn clearing_a_binding_leaves_it_unbound_rather_than_re_guessed() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.flac", "Album Title", 12),
    )
    .unwrap();

    let proposed = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    assert_eq!(
        proposed.track_count(),
        12,
        "the directive resolves, so the scan proposes the binding on its own",
    );

    let cleared = scan_with_binding(&album, &proposed, "cd.cue", None);

    assert_eq!(
        cleared.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
        "the sheet the user cleared describes nothing, proposal or not",
    );
    assert_eq!(cleared.track_count(), 1);
    assert_eq!(cleared.format_label, "FLAC");
    assert!(cleared.bound_sheets().is_empty());
}

/// A binding whose audio leaves the folder is not silently kept. Removing the
/// file changes the file set, so it changes the hash the decision is stored
/// under, so the decision is unreachable and the candidate derives from what is
/// actually there. The behaviour is what matters; the hash is only how.
#[test]
fn a_binding_whose_audio_disappears_is_not_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        album.join("bonus.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 12),
    )
    .unwrap();

    let scanned = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let stored = stored_binding(&scanned, "cd.cue", Some("cd.flac"));
    assert_eq!(
        collect_release_candidate_files_with_scope(
            &album,
            crate::import::ReleaseFileScope::Recursive,
            &stored
        )
        .expect("scan")
        .track_count(),
        12,
        "the binding applies while the audio it names is here",
    );

    std::fs::remove_file(album.join("cd.flac")).unwrap();

    let after = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .expect("scan");
    assert_eq!(
        after.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
        "the folder derives from what is on disk, with no memory of the removed pairing",
    );
    assert_eq!(
        after.track_count(),
        1,
        "one standalone track is all that is left"
    );
}

/// The stored bindings a fresh scan of `folder` would apply, if the user had
/// made this one decision about `files`.
fn stored_binding(
    files: &CategorizedFiles,
    sheet_file_id: &str,
    audio_file_id: Option<&str>,
) -> StoredCandidateEdits {
    let mut edits = SheetBindingEdits::default();
    edits.set(
        sheet_file_id.to_string(),
        match audio_file_id {
            Some(file_id) => UserSheetBinding::Describes {
                file_id: file_id.to_string(),
            },
            None => UserSheetBinding::Cleared,
        },
    );
    StoredCandidateEdits::new(HashMap::from([(
        files.content_hash(),
        CandidateFileEdits {
            sheet_bindings: edits,
            ..Default::default()
        },
    )]))
}

/// Re-scan `folder` as it reads once the user has made one binding decision.
fn scan_with_binding(
    folder: &Path,
    files: &CategorizedFiles,
    sheet_file_id: &str,
    audio_file_id: Option<&str>,
) -> CategorizedFiles {
    collect_release_candidate_files_with_scope(
        folder,
        crate::import::ReleaseFileScope::Recursive,
        &stored_binding(files, sheet_file_id, audio_file_id),
    )
    .expect("scan")
}

// ── What a role makes of a file, and which files are the release's tracks ────

/// A folder holding a disc image, its sheet, and two loose bonus tracks. The
/// "Becomes" column reads off the folder alone — no release has been picked —
/// and it says which slots each file backs: the sheet carves the first eleven,
/// the bonus files take one each, and the container the sheet speaks for backs
/// none of its own.
#[test]
fn becomes_names_the_slots_each_file_backs() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("CDImage.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("CDImage.cue"),
        make_cue_content_n_tracks("CDImage.flac", "Album Title", 11),
    )
    .unwrap();
    std::fs::write(album.join("bonus-1.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("bonus-2.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("cover.jpg"), fake_jpeg()).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let becomes: Vec<(&str, FileBecomes)> = files
        .files
        .iter()
        .map(|entry| entry.file.relative_path.as_str())
        .zip(files.becomes())
        .collect();

    assert_eq!(
        becomes,
        vec![
            ("CDImage.cue", FileBecomes::Slots { first: 1, last: 11 }),
            ("CDImage.flac", FileBecomes::NoSlots),
            (
                "bonus-1.flac",
                FileBecomes::Slots {
                    first: 12,
                    last: 12
                }
            ),
            (
                "bonus-2.flac",
                FileBecomes::Slots {
                    first: 13,
                    last: 13
                }
            ),
            ("cover.jpg", FileBecomes::NoSlots),
        ],
    );
}

/// A directory of nothing but documents collapses to one row. A directory of
/// images does not — every image belongs to the gallery, which shows it — and
/// neither does one holding two different jobs.
#[test]
fn a_homogeneous_directory_collapses_to_one_row() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(album.join("scans")).unwrap();
    std::fs::create_dir_all(album.join("logs")).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("cover.jpg"), fake_jpeg()).unwrap();
    for index in 1..=4 {
        std::fs::write(album.join(format!("scans/page-{index}.jpg")), fake_jpeg()).unwrap();
    }
    std::fs::write(album.join("logs/rip.log"), b"log").unwrap();
    std::fs::write(album.join("logs/notes.txt"), b"notes").unwrap();
    // A directory holding two different jobs stays expanded.
    std::fs::create_dir_all(album.join("extras")).unwrap();
    std::fs::write(album.join("extras/back.jpg"), fake_jpeg()).unwrap();
    std::fs::write(album.join("extras/info.txt"), b"info").unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let collapsed: Vec<(String, FileRowKind, u32)> = files
        .collapsed_directories()
        .into_iter()
        .map(|dir| (dir.dir_prefix, dir.kind, dir.count))
        .collect();

    assert_eq!(
        collapsed,
        vec![("logs/".to_string(), FileRowKind::Document, 2)],
    );
}

/// Taking a file out of the tracklist stops it producing a slot, and the file
/// stays in the release: the folder is the release, so it still imports. The
/// content hash is what the decision is stored under, so it must not move.
#[test]
fn a_file_taken_out_of_the_tracklist_stops_being_a_slot_and_stays_in_the_release() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    for index in 1..=3 {
        std::fs::write(album.join(format!("{index:02}.flac")), fake_flac()).unwrap();
    }

    let mut files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let hash = files.content_hash();
    assert_eq!(files.track_count(), 3);

    let mut roles = FileRoleEdits::default();
    roles.set("03.flac".to_string(), FileRoleChoice::NotATrack);
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            file_roles: roles.clone(),
            ..Default::default()
        })
        .expect("taking one of three out is fine");

    assert_eq!(files.track_count(), 2, "it stops being one of the tracks");
    assert_eq!(
        files.becomes().last(),
        Some(&FileBecomes::NoSlots),
        "and it backs no slot",
    );
    assert_eq!(
        files.release_files().count(),
        3,
        "the folder is the release: the file is still carried, uploaded and exported",
    );
    assert_eq!(
        files.content_hash(),
        hash,
        "the hash covers files, never role decisions, so the row stays addressable",
    );

    // And a fresh walk with the decision stored reads the same way, which is
    // what makes an exclusion survive re-picking a release and relaunching.
    let stored = StoredCandidateEdits::new(HashMap::from([(
        hash,
        CandidateFileEdits {
            file_roles: roles,
            ..Default::default()
        },
    )]));
    let reopened = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &stored,
    )
    .expect("scan");
    assert_eq!(reopened.track_count(), 2);
    assert_eq!(reopened.release_files().count(), 3);
}

/// Taking out the last audio a folder has is refused, and refused on a copy, so
/// nothing is written and the candidate is left exactly as it was. A release
/// with no tracks is not a state the rest of the import can describe.
#[test]
fn taking_out_the_last_audio_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();

    let mut files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let mut roles = FileRoleEdits::default();
    roles.set("01.flac".to_string(), FileRoleChoice::NotATrack);

    let err = files
        .apply_candidate_file_edits(&CandidateFileEdits {
            file_roles: roles,
            ..Default::default()
        })
        .expect_err("there would be nothing left to import");
    assert_eq!(err, InvalidReason::NoValidAudio);
}

/// A decision only ever moves a file the scan read as audio. A stored decision
/// naming an image is ignored rather than applied to whatever now sits at that
/// path, and an image is never offered the choice in the first place.
#[test]
fn only_audio_carries_a_role_decision() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("cover.jpg"), fake_jpeg()).unwrap();

    let mut files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let alternatives: Vec<(&str, usize)> = files
        .files
        .iter()
        .map(|entry| {
            (
                entry.file.relative_path.as_str(),
                entry.role_alternatives().len(),
            )
        })
        .collect();
    assert_eq!(alternatives, vec![("01.flac", 2), ("cover.jpg", 0)]);

    let mut roles = FileRoleEdits::default();
    roles.set("cover.jpg".to_string(), FileRoleChoice::NotATrack);
    files
        .apply_candidate_file_edits(&CandidateFileEdits {
            file_roles: roles,
            ..Default::default()
        })
        .expect("a decision about a non-audio file changes nothing");

    assert!(matches!(files.files[1].role, FileRole::Cover));
    assert_eq!(files.track_count(), 1);
}
