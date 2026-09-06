// ── The sheet↔audio binding is a user decision ──────────────────────────
//
// The scan proposes; these pin both its automatic choices and what happens
// when the user overrules them.

/// A single-file sheet written against a WAV automatically describes the FLAC
/// it was encoded to when it is the only same-stem audio beside the sheet.
#[test]
fn single_file_cue_uses_the_unique_same_stem_audio_when_its_reference_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    let cue = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.cue"),
    )
    .unwrap()
    .replace("Test Album.flac", "cd.wav");
    std::fs::write(album.join("cd.cue"), cue).unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");

    assert_eq!(files.track_count(), 3);
    assert_uniform_source_audio(&files, crate::album_detail::SourceAudioLayout::Cue, "FLAC");
    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "cd.wav".to_string(),
                file_id: "cd.flac".to_string(),
            }],
        },
    );
    assert_eq!(files.bound_sheets()[0].audio_files[0].1.file_name, "cd.flac");
    assert!(
        crate::import::discid::compute_discid_from_categorized(&files).is_some(),
        "the automatically bound sheet and audio yield a disc ID",
    );
}

#[test]
fn same_stem_audio_is_not_guessed_when_more_than_one_file_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_ape/Test Album.ape"),
        album.join("cd.ape"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 3),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
    );
    assert_eq!(files.track_count(), 2);
}

#[test]
fn same_stem_audio_outside_the_cue_directory_is_not_guessed() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(album.join("sheets")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("sheets/cd.cue"),
        make_cue_content_n_tracks("cd.wav", "Album Title", 3),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
    );
    assert_eq!(files.track_count(), 1);
}

#[test]
fn multi_file_cue_with_a_missing_reference_stays_unresolved() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("track-01.flac"),
    )
    .unwrap();
    std::fs::write(
        album.join("disc.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
         FILE \"track-01.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
         FILE \"track-02.wav\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
    );
    assert_eq!(files.track_count(), 1);
}

#[test]
fn exact_file_reference_wins_over_other_same_stem_audio() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join("cd.flac"),
    )
    .unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_ape/Test Album.ape"),
        album.join("cd.ape"),
    )
    .unwrap();
    std::fs::write(
        album.join("cd.cue"),
        make_cue_content_n_tracks("cd.flac", "Album Title", 3),
    )
    .unwrap();

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "cd.flac".to_string(),
                file_id: "cd.flac".to_string(),
            }],
        },
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
        make_cue_content_n_tracks("cd.wav", "Album Title", 12),
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
        "the unique same-stem audio makes the scan propose the binding",
    );

    let cleared = scan_with_binding(&album, &proposed, "cd.cue", None);

    assert_eq!(
        cleared.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved,
        "the sheet the user cleared describes nothing, proposal or not",
    );
    assert_eq!(cleared.track_count(), 1);
    assert_uniform_source_audio(&cleared, crate::album_detail::SourceAudioLayout::File, "FLAC");
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
        13,
        "the binding contributes twelve tracks alongside the loose bonus track",
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
            ("bonus-1.flac", FileBecomes::Slots { first: 1, last: 1 }),
            ("bonus-2.flac", FileBecomes::Slots { first: 2, last: 2 }),
            ("CDImage.cue", FileBecomes::Slots { first: 3, last: 13 }),
            ("CDImage.flac", FileBecomes::NoSlots),
            ("cover.jpg", FileBecomes::NoSlots),
        ],
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

    assert!(matches!(files.files[1].role, FileRole::Artwork));
    assert_eq!(files.track_count(), 1);
}

/// The folder lists its files the way a person reads them: natural order
/// (`2` before `10`) and case-insensitive, so `cover.jpg` sits among the
/// names starting with `c`, not after every capitalized one.
#[test]
fn files_list_in_case_insensitive_natural_order() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Album");
    std::fs::create_dir_all(&album).unwrap();
    for name in [
        "Track 10.flac",
        "cover.jpg",
        "Track 2.flac",
        "Back.jpg",
        "booklet.pdf",
    ] {
        let bytes = if name.ends_with(".flac") {
            fake_flac()
        } else if name.ends_with(".jpg") {
            fake_jpeg()
        } else {
            b"%PDF-1.4".to_vec()
        };
        std::fs::write(album.join(name), bytes).unwrap();
    }

    let files = collect_release_candidate_files_with_scope(
        &album,
        crate::import::ReleaseFileScope::Recursive,
        &StoredCandidateEdits::none(),
    )
    .expect("scan");
    let names: Vec<&str> = files
        .files
        .iter()
        .map(|entry| entry.file.relative_path.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "Back.jpg",
            "booklet.pdf",
            "cover.jpg",
            "Track 2.flac",
            "Track 10.flac",
        ]
    );
}
