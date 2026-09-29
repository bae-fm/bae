// ── The sheet↔audio binding is a user decision ──────────────────────────
//
// The scan proposes; these pin both its automatic choices and what happens
// when the user overrules them.

/// Copy the CUE/FLAC disc-image fixture into `album` under `name`.
fn copy_cue_flac(album: &Path, name: &str) {
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.flac"),
        album.join(name),
    )
    .unwrap();
}

/// An `Album` folder holding the disc-image FLAC as `audio_name`, beside a
/// sheet named for the same stem that carves `tracks` tracks out of
/// `cue_reference`.
fn cue_flac_album(
    audio_name: &str,
    cue_reference: &str,
    tracks: usize,
) -> (tempfile::TempDir, PathBuf) {
    let (tmp, album) = album_dir();
    copy_cue_flac(&album, audio_name);
    let stem = Path::new(audio_name)
        .file_stem()
        .expect("the audio fixture is named")
        .to_string_lossy()
        .into_owned();
    std::fs::write(
        album.join(format!("{stem}.cue")),
        make_cue_content_n_tracks(cue_reference, "Album Title", tracks),
    )
    .unwrap();
    (tmp, album)
}

/// A single-file sheet written against a WAV automatically describes the FLAC
/// it was encoded to when it is the only same-stem audio beside the sheet.
#[test]
fn single_file_cue_uses_the_unique_same_stem_audio_when_its_reference_is_missing() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "cd.flac");
    let cue = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.cue"),
    )
    .unwrap()
    .replace("Test Album.flac", "cd.wav");
    std::fs::write(album.join("cd.cue"), cue).unwrap();

    let files = scan_files(&album);

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
    assert_eq!(
        files.bound_sheets()[0].audio_files[0].1.file_name,
        "cd.flac"
    );
    assert!(
        crate::import::discid::read_rip_artifacts(&files)
            .disc_id
            .computed()
            .is_some(),
        "the automatically bound sheet and audio yield a disc ID",
    );
}

/// A sheet written on another machine names the path the audio had there —
/// folders this folder does not have, `\\` for a separator, and the WAV the
/// FLAC was encoded from. The reference is tried from the whole path down to
/// the bare file name, and the same-stem audio beside the sheet answers.
#[test]
fn a_reference_with_foreign_folders_resolves_by_its_file_name() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "cd.flac");
    let cue = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.cue"),
    )
    .unwrap()
    .replace("Test Album.flac", "Artist\\Album\\cd.wav");
    std::fs::write(album.join("cd.cue"), cue).unwrap();

    let files = scan_files(&album);

    assert_eq!(files.track_count(), 3);
    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "Artist\\Album\\cd.wav".to_string(),
                file_id: "cd.flac".to_string(),
            }],
        },
    );
}

/// A sheet moved into a subfolder of the release still names the audio beside
/// where it used to be: its own folder is tried first, then each folder above
/// it, so the audio at the release root answers.
#[test]
fn a_sheet_in_a_subfolder_resolves_audio_above_it() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "cd.flac");
    std::fs::create_dir(album.join("extras")).unwrap();
    let cue = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.cue"),
    )
    .unwrap()
    .replace("Test Album.flac", "cd.wav");
    std::fs::write(album.join("extras").join("cd.cue"), cue).unwrap();

    let files = scan_files(&album);

    assert_eq!(files.track_count(), 3);
    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "cd.wav".to_string(),
                file_id: "cd.flac".to_string(),
            }],
        },
    );
}

/// A per-track sheet whose references name nothing here — the tracks were
/// renamed after the rip — takes the audio files beside it in name order when
/// there are exactly as many of them as it has references.
#[test]
fn a_sheet_naming_nothing_takes_the_audio_beside_it_in_name_order() {
    let (_tmp, album) = album_dir();
    for name in ["01. Track One.flac", "02. Track Two.flac"] {
        copy_cue_flac(&album, name);
    }
    std::fs::write(
        album.join("album.cue"),
        "FILE \"01 -First.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
         FILE \"02 -Second.wav\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let files = scan_files(&album);

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![
                SheetAudioFile {
                    file_reference: "01 -First.wav".to_string(),
                    file_id: "01. Track One.flac".to_string(),
                },
                SheetAudioFile {
                    file_reference: "02 -Second.wav".to_string(),
                    file_id: "02. Track Two.flac".to_string(),
                },
            ],
        },
    );
    assert_eq!(files.track_count(), 2);
}

/// The last resort needs the count to match: one reference beside two audio
/// files is not a pairing anyone asked for.
#[test]
fn a_sheet_naming_nothing_beside_a_different_count_stays_unbound() {
    let (_tmp, album) = album_dir();
    for name in ["01. Track One.flac", "02. Track Two.flac"] {
        copy_cue_flac(&album, name);
    }
    std::fs::write(
        album.join("album.cue"),
        "FILE \"Range.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let files = scan_files(&album);

    assert!(files.carving_sheets().is_empty());
    assert_eq!(files.track_count(), 2);
}

#[test]
fn same_stem_audio_is_not_guessed_when_more_than_one_file_matches() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.wav", 3);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_ape/Test Album.ape"),
        album.join("cd.ape"),
    )
    .unwrap();

    let files = scan_files(&album);

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved { files: Vec::new() },
    );
    assert_eq!(files.track_count(), 2);
}

#[test]
fn multi_file_cue_with_a_missing_reference_stays_unresolved() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "track-01.flac");
    std::fs::write(
        album.join("disc.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
         FILE \"track-01.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
         FILE \"track-02.wav\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let files = scan_files(&album);

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved {
            files: vec![SheetAudioFile {
                file_reference: "track-01.flac".into(),
                file_id: "track-01.flac".into()
            }]
        },
    );
    assert_eq!(files.track_count(), 1);
}

#[test]
fn exact_file_reference_wins_over_other_same_stem_audio() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.flac", 3);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_ape/Test Album.ape"),
        album.join("cd.ape"),
    )
    .unwrap();

    let files = scan_files(&album);

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
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.wav", 12);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-fixtures/audio-format/placeholder-mp3.mp3"),
        album.join("cd.mp3"),
    )
    .unwrap();

    let files = scan_files(&album);
    let options = files.sheet_binding_options("cd.cue");

    assert_eq!(
        options[0].options,
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
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.wav", 12);

    let proposed = scan_files(&album);
    assert_eq!(
        proposed.track_count(),
        12,
        "the unique same-stem audio makes the scan propose the binding",
    );

    let cleared = scan_with_binding(&album, &proposed, "cd.cue", None);

    assert_eq!(
        cleared.track_sheets().next().unwrap().binding,
        &SheetBinding::Unresolved { files: Vec::new() },
        "the sheet the user cleared describes nothing, proposal or not",
    );
    assert_eq!(cleared.track_count(), 1);
    assert_uniform_source_audio(
        &cleared,
        crate::album_detail::SourceAudioLayout::File,
        "FLAC",
    );
    assert!(cleared.bound_sheets().is_empty());
}

/// A binding whose audio leaves the folder is not silently kept. Removing the
/// file changes the file set, so it changes the hash the decision is stored
/// under, so the decision is unreachable and the candidate derives from what is
/// actually there. The behaviour is what matters; the hash is only how.
#[test]
fn a_binding_whose_audio_disappears_is_not_kept() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.wav", 12);
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/flac/01 Test Track 1.flac"),
        album.join("bonus.flac"),
    )
    .unwrap();

    let scanned = scan_files(&album);
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
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "cd.wav".to_string(),
                file_id: "bonus.flac".to_string(),
            }],
        },
        "the folder derives from what is on disk, with no memory of the removed pairing: \
         the one audio file left beside the sheet is what it takes",
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
    edits.set_reference(
        sheet_file_id.to_string(),
        files
            .track_sheets()
            .find(|sheet| sheet.file.relative_path == sheet_file_id)
            .unwrap()
            .sheet
            .single_file()
            .unwrap()
            .to_owned(),
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

// ── Which slots each file backs ──────────────────────────────────────────────

/// A folder holding a disc image, its sheet, and two loose bonus tracks. The
/// "Becomes" column reads off the folder alone — no release has been picked —
/// and it says which slots each file backs: the sheet carves the first eleven,
/// the bonus files take one each, and the container the sheet speaks for backs
/// none of its own.
#[test]
fn becomes_names_the_slots_each_file_backs() {
    let (_tmp, album) = cue_flac_album("CDImage.flac", "CDImage.flac", 11);
    std::fs::write(album.join("bonus-1.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("bonus-2.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("cover.jpg"), fake_jpeg()).unwrap();

    let files = scan_files(&album);
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

/// The folder lists its files the way a person reads them: natural order
/// (`2` before `10`) and case-insensitive, so `cover.jpg` sits among the
/// names starting with `c`, not after every capitalized one.
#[test]
fn files_list_in_case_insensitive_natural_order() {
    let (_tmp, album) = album_dir();
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

    let files = scan_files(&album);
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

/// Two sheets for one image is the usual rip leftover, so the first in file
/// order carves it and the other is ignored rather than both being set aside.
#[test]
fn the_first_of_competing_cues_carves_the_audio() {
    let (_tmp, album) = cue_flac_album("cd.flac", "cd.flac", 3);
    std::fs::write(
        album.join("alternative.cue"),
        make_cue_content_n_tracks("cd.flac", "Album Title", 2),
    )
    .unwrap();
    let files = scan_files(&album);
    assert_eq!(
        files.track_count(),
        2,
        "alternative.cue sorts first and carves"
    );
    assert_eq!(files.carving_sheets().len(), 1);
    assert_eq!(files.bound_sheets().len(), 2);
}

#[test]
fn multi_file_cue_offers_bindings_for_each_reference() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "first.flac");
    copy_cue_flac(&album, "second.flac");
    std::fs::write(album.join("disc.cue"),
        "FILE \"first.flac\" WAVE\n TRACK 01 AUDIO\n INDEX 01 00:00:00\nFILE \"missing.wav\" WAVE\n TRACK 02 AUDIO\n INDEX 01 00:00:00\n").unwrap();
    let files = scan_files(&album);
    assert_eq!(files.track_count(), 2);
    assert!(!files.sheet_binding_options("disc.cue").is_empty());
}

/// A sheet written on a system that ignores case names its audio in another
/// case than the file has here. The loose bonus track beside it keeps the
/// name-order fallback from answering, so only the reference itself can.
#[test]
fn a_reference_in_another_case_resolves_to_its_audio() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "CDImage.flac");
    copy_cue_flac(&album, "bonus.flac");
    let cue = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.cue"),
    )
    .unwrap()
    .replace("Test Album.flac", "cdimage.WAV");
    std::fs::write(album.join("CDImage.cue"), cue).unwrap();

    let files = scan_files(&album);

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "cdimage.WAV".to_string(),
                file_id: "CDImage.flac".to_string(),
            }],
        },
    );
    assert_eq!(files.track_count(), 4, "three slices and the bonus track");
}

/// The same name in the other Unicode normalization: a sheet's text is
/// usually composed (NFC) while a file name can be stored decomposed (NFD).
#[test]
fn a_reference_in_another_unicode_normalization_resolves_to_its_audio() {
    use unicode_normalization::UnicodeNormalization;
    let (_tmp, album) = album_dir();
    let decomposed: String = "Café Image.flac".nfd().collect();
    copy_cue_flac(&album, &decomposed);
    copy_cue_flac(&album, "bonus.flac");
    let composed: String = "Café Image.flac".nfc().collect();
    let cue = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cue_flac/Test Album.cue"),
    )
    .unwrap()
    .replace("Test Album.flac", &composed);
    std::fs::write(album.join("disc.cue"), cue).unwrap();

    let files = scan_files(&album);

    let binding = files.track_sheets().next().unwrap().binding;
    let SheetBinding::Resolved { files: audio } = binding else {
        panic!("the sheet describes its audio, got {binding:?}");
    };
    assert_eq!(audio.len(), 1);
    assert_eq!(
        audio[0].file_id.nfc().collect::<String>(),
        composed,
        "the reference names the image, not the bonus track"
    );
}

/// A per-track sheet whose references differ from the files only in case
/// resolves every reference, even with an extra file beside them.
#[test]
fn a_multi_file_sheet_in_another_case_resolves_every_reference() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "01 Track.flac");
    copy_cue_flac(&album, "02 Track.flac");
    copy_cue_flac(&album, "03 Hidden.flac");
    std::fs::write(
        album.join("disc.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
         FILE \"01 TRACK.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
         FILE \"02 track.wav\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let files = scan_files(&album);

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![
                SheetAudioFile {
                    file_reference: "01 TRACK.flac".into(),
                    file_id: "01 Track.flac".into(),
                },
                SheetAudioFile {
                    file_reference: "02 track.wav".into(),
                    file_id: "02 Track.flac".into(),
                },
            ],
        },
    );
}
