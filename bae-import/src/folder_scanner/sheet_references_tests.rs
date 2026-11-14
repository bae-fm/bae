//! The audio each `FILE` reference of a track sheet names: by its path from
//! the sheet's folder or one above it, then by name or stem once case and
//! normalization are set aside, and nothing where several files answer.

use super::*;
use crate::folder_scanner::tests::*;

/// A single-file sheet written against a WAV automatically describes the FLAC
/// it was encoded to when it is the only same-stem audio beside the sheet.
#[test]
fn single_file_cue_uses_the_unique_same_stem_audio_when_its_reference_is_missing() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "cd.flac");
    let cue =
        std::fs::read_to_string(bae_audio::test_fixtures::cue_flac_dir().join("Test Album.cue"))
            .unwrap()
            .replace("Test Album.flac", "cd.wav");
    std::fs::write(album.join("cd.cue"), cue).unwrap();

    let files = scan_files(&album);

    assert_eq!(files.track_count(), 3);
    assert_uniform_source_audio(&files, bae_records::source_audio::SourceAudioLayout::Cue, "FLAC");
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
        crate::rip_reading::read_rip_artifacts(&files, &bae_util::test_files::lease())
            .unwrap()
            .disc_id
            .computed()
            .len()
            == 1,
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
    let cue =
        std::fs::read_to_string(bae_audio::test_fixtures::cue_flac_dir().join("Test Album.cue"))
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
    let cue =
        std::fs::read_to_string(bae_audio::test_fixtures::cue_flac_dir().join("Test Album.cue"))
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

/// A sheet written on a system that ignores case names its audio in another
/// case than the file has here. The loose bonus track beside it keeps the
/// name-order fallback from answering, so only the reference itself can.
#[test]
fn a_reference_in_another_case_resolves_to_its_audio() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "CDImage.flac");
    copy_cue_flac(&album, "bonus.flac");
    let cue =
        std::fs::read_to_string(bae_audio::test_fixtures::cue_flac_dir().join("Test Album.cue"))
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
    let cue =
        std::fs::read_to_string(bae_audio::test_fixtures::cue_flac_dir().join("Test Album.cue"))
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

/// A sheet saved in a Windows code page rather than UTF-8 still names its
/// audio: the sheet's text is decoded before its `FILE` reference is read.
#[test]
fn a_sheet_in_a_windows_code_page_names_its_audio() {
    let (_tmp, album) = album_dir();
    copy_cue_flac(&album, "Café Image.flac");
    copy_cue_flac(&album, "bonus.flac");
    let cue =
        std::fs::read_to_string(bae_audio::test_fixtures::cue_flac_dir().join("Test Album.cue"))
            .unwrap()
            .replace("Test Album.flac", "Café Image.wav");
    let (windows_1252, _, unmappable) = encoding_rs::WINDOWS_1252.encode(&cue);
    assert!(!unmappable);
    std::fs::write(album.join("disc.cue"), windows_1252).unwrap();

    let files = scan_files(&album);

    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "Café Image.wav".to_string(),
                file_id: "Café Image.flac".to_string(),
            }],
        },
    );
}

// ── Files carry roles ───────────────────────────────────────────────────
//
// Rooted in the folders that were broken, not in the model's own shape.

/// A sheet written against a WAV that was later encoded to FLAC still binds
/// when the FLAC is the unique same-stem audio beside it.
#[test]
fn sheet_naming_absent_audio_uses_the_unique_same_stem_file() {
    let (tmp, album) = album_dir();
    std::fs::write(album.join("Album.flac"), fake_flac()).unwrap();
    std::fs::write(album.join("Album.cue"), make_cue_content("Album.wav", "Album Title")).unwrap();

    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    let candidate = match &items[0] {
        ScanItem::Valid(candidate) => candidate,
        ScanItem::Invalid(invalid) => {
            panic!("folder must stay importable, got invalid: {}", invalid.reason())
        }
        ScanItem::Discovered(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    };

    let sheets: Vec<_> = candidate.files.track_sheets().collect();
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].file.file_name, "Album.cue");
    assert_eq!(
        sheets[0].binding,
        &SheetBinding::Resolved {
            files: vec![SheetAudioFile {
                file_reference: "Album.wav".to_string(),
                file_id: "Album.flac".to_string(),
            }],
        },
    );
    assert_eq!(
        candidate.files.audio().map(|f| f.file_name.as_str()).collect::<Vec<_>>(),
        vec!["Album.flac"],
    );
    assert_eq!(
        candidate.files.track_count(),
        2,
        "the bound sheet defines the release's track slots",
    );
}

/// Every FILE entry uses the same path resolution rule. A ripper commonly
/// writes a CUE before its WAV files are encoded, leaving the sheet's stems
/// intact while the audio extension changes.
#[tokio::test]
async fn multi_file_cue_resolves_each_reference_by_unique_stem() {
    let (tmp, album) = album_dir();
    for name in ["01 First.flac", "02 Second.flac", "03 Third.flac"] {
        std::fs::write(album.join(name), bytes_for(FileKind::Flac)).unwrap();
    }
    std::fs::write(
        album.join("Album.cue"),
        "PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n\
             FILE \"01 First.wav\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"02 Second.wav\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n\
             FILE \"03 Third.wav\" WAVE\n  TRACK 03 AUDIO\n    INDEX 01 00:00:00\n",
    )
    .unwrap();

    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    let files = &candidates[0].files;
    assert_eq!(files.bound_sheets().len(), 1);
    assert_eq!(
        files.track_sheets().next().unwrap().binding,
        &SheetBinding::Resolved {
            files: ["01 First", "02 Second", "03 Third"]
                .map(|stem| SheetAudioFile {
                    file_reference: format!("{stem}.wav"),
                    file_id: format!("{stem}.flac"),
                })
                .to_vec(),
        },
        "the binding carries each reference and the audio it resolved to",
    );
    assert!(
        crate::rip_reading::read_rip_artifacts(files, &bae_util::test_files::lease())
            .unwrap()
            .disc_id
            .computed()
            .iter()
            .map(|computed| computed.source_file.as_deref())
            .eq([Some("Album.cue")]),
        "a one-file-per-track rip lays its files end to end into a disc ID",
    );
    let units = crate::audio_layout::audio_units(files);
    assert_eq!(
        units,
        vec![
            crate::types::AudioFile::SheetSlice {
                file_id: "01 First.flac".to_string(),
                sheet_id: "Album.cue".to_string(),
                index: 0,
            },
            crate::types::AudioFile::SheetSlice {
                file_id: "02 Second.flac".to_string(),
                sheet_id: "Album.cue".to_string(),
                index: 1,
            },
            crate::types::AudioFile::SheetSlice {
                file_id: "03 Third.flac".to_string(),
                sheet_id: "Album.cue".to_string(),
                index: 2,
            },
        ],
    );

    let table = crate::mapping::tests::blank_table(files).await;
    let Some(crate::MappingTrackRun::Sheet { sheet, entries }) = table.track_sides[0].runs.first()
    else {
        panic!("the resolved CUE should group its track rows");
    };
    assert_eq!(sheet.bound, crate::SheetBound::DescribesFiles { audio_file_count: 3 });
    assert_eq!(
        entries
            .iter()
            .map(|entry| match &entry.source {
                crate::MappingSource::SheetEntry(entry) => entry.container_id.as_str(),
                _ => panic!("a CUE group should contain sheet entries"),
            })
            .collect::<Vec<_>>(),
        vec!["01 First.flac", "02 Second.flac", "03 Third.flac"],
    );
}

/// Single-FILE CUE whose own stem differs from the audio file it names —
/// pair detection follows the CUE's `FILE` directive, not the CUE's
/// filename stem. The CUE is the source of truth for what it points at.
#[test]
fn single_file_cue_pairs_by_file_directive_not_stem() {
    let (tmp, album) = album_dir();
    std::fs::write(album.join("Audio.flac"), bytes_for(FileKind::Flac)).unwrap();
    std::fs::write(
        album.join("Sheet.cue"),
        "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Audio.flac\" WAVE\n  \
             TRACK 01 AUDIO\n    INDEX 01 00:00:00\n  \
             TRACK 02 AUDIO\n    INDEX 01 00:01:00\n",
    )
    .unwrap();
    let candidates = scan_valid(tmp.path().to_path_buf());
    assert_eq!(candidates.len(), 1);
    let bound = candidates[0].files.bound_sheets();
    assert_eq!(bound.len(), 1);
    assert_eq!(bound[0].audio_files[0].1.file_name, "Audio.flac");
    assert_eq!(bound[0].file.file_name, "Sheet.cue");
}

/// A sheet whose `FILE` directive names missing audio leaves the sheet
/// unbound. The folder still imports: the sheet proposes a layout, it does not
/// dictate one.
#[test]
fn cue_referencing_missing_audio_leaves_the_sheet_unbound() {
    let mut entries = flat_audio("Album", 5, FileKind::Flac);
    entries.push(file(
        "Album/Album.cue",
        FileKind::NonPairingCue { n_tracks: 5, file_reference: "Album.flac" },
    ));
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    build_fixture(&root, &entries);
    let items = scan_items(root);
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(candidate.files.bound_sheets().is_empty());
            assert_eq!(candidate.files.audio().count(), 5);
        }
        ScanItem::Invalid(invalid) => panic!("must stay importable: {}", invalid.reason()),
        ScanItem::Discovered(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

// ── Layer 3: edge / adversarial cases ─────────────────────────────────

/// A multi-FILE CUE referencing a missing audio file describes a layout
/// the folder can't supply, so it doesn't bind at all: a partial binding would
/// carve tracks out of audio that isn't there. The folder still imports, with
/// the present audio as one track.
#[test]
fn multi_file_cue_with_missing_secondary_file_stays_unbound() {
    let (tmp, album) = album_dir();
    std::fs::write(album.join("Album.flac"), bytes_for(FileKind::Flac)).unwrap();
    // Hand-write the CUE because the DSL doesn't model two-FILE sheets.
    std::fs::write(
            album.join("Album.cue"),
            "PERFORMER \"X\"\nTITLE \"Y\"\nFILE \"Album.flac\" WAVE\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\nFILE \"Missing.flac\" WAVE\n  TRACK 02 AUDIO\n    INDEX 01 05:00:00\n",
        )
        .unwrap();
    let items = scan_items(tmp.path().to_path_buf());
    assert_eq!(items.len(), 1);
    match &items[0] {
        ScanItem::Valid(candidate) => {
            assert!(
                candidate.files.bound_sheets().is_empty(),
                "a sheet binds only when every FILE reference resolves",
            );
            assert_eq!(candidate.files.audio().count(), 1);
            assert_eq!(candidate.files.track_count(), 1);
        }
        ScanItem::Invalid(invalid) => panic!("must stay importable: {}", invalid.reason()),
        ScanItem::Discovered(_) => {
            panic!("terminal scan helper returned a progress item")
        }
    }
}

/// A CUE lacking PERFORMER/TITLE still parses. Its `FILE` directive
/// names audio that isn't here, so it stays unbound and the three FLACs import
/// as themselves — the sheet's declared 15 tracks are a claim about audio the
/// folder doesn't have.
#[test]
fn cue_no_header_naming_absent_audio_stays_unbound() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(file(
        "Album/Album.cue",
        FileKind::CueNoHeader { n_tracks: 15, file_reference: "Album.flac" },
    ));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    assert!(c.files.bound_sheets().is_empty());
    assert_eq!(c.files.track_count(), 3);
}

/// A CUE with an unquoted FILE directive still pairs when it
/// shares a stem; when the track count exceeds on-disk audio, the
/// mismatch guard fires.
#[test]
fn cue_file_reference_with_unquoted_filename_still_parses() {
    // Stem-matched variant: pair detected even with unquoted FILE.
    let result = run_scenario(vec![
        file("Paired/Album.flac", FileKind::Flac),
        file("Paired/Album.cue", FileKind::CueUnquoted { stem: "Album.flac", n_tracks: 6 }),
    ]);
    assert_eq!(result.candidate("Paired").files.bound_sheets().len(), 1);

    // Non-pairing variant: the directive names audio that isn't here, so the
    // sheet stays unbound and the folder still imports.
    let mut entries = flat_audio("Mismatch", 3, FileKind::Flac);
    entries.push(file(
        "Mismatch/Album.cue",
        FileKind::CueUnquoted { stem: "Album.flac", n_tracks: 15 },
    ));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Mismatch"]);
    assert!(result.candidate("Mismatch").files.bound_sheets().is_empty());
}

/// A reference matches the unique same-stem audio beside the sheet.
#[test]
fn resolve_cue_audio_path_matches_by_stem() {
    let audio_files =
        vec![PathBuf::from("/rip/Some Other.mp3"), PathBuf::from("/rip/Album Image.flac")];

    assert_eq!(
        resolve_cue_audio_path(Path::new("/rip/Album.cue"), "Album Image.wav", &audio_files),
        Some(&PathBuf::from("/rip/Album Image.flac")),
    );
}

/// No audio file shares the FILE reference's stem → no match.
#[test]
fn resolve_cue_audio_path_no_stem_match_is_none() {
    let audio_files = vec![PathBuf::from("/rip/Different Name.flac")];

    assert!(resolve_cue_audio_path(Path::new("/rip/Album.cue"), "Album Image.flac", &audio_files)
        .is_none());
}

#[test]
fn resolve_cue_audio_path_ambiguous_same_stem_is_none() {
    let audio_files =
        vec![PathBuf::from("/rip/Album Image.flac"), PathBuf::from("/rip/Album Image.ape")];

    assert!(resolve_cue_audio_path(Path::new("/rip/Album.cue"), "Album Image.wav", &audio_files)
        .is_none());
}

#[test]
fn resolve_cue_audio_path_exact_path_wins_over_same_stem_audio() {
    let audio_files =
        vec![PathBuf::from("/rip/Album Image.ape"), PathBuf::from("/rip/Album Image.flac")];

    assert_eq!(
        resolve_cue_audio_path(Path::new("/rip/Album.cue"), "Album Image.flac", &audio_files),
        Some(&PathBuf::from("/rip/Album Image.flac")),
    );
}

#[test]
fn resolve_cue_audio_path_other_directory_is_none() {
    let audio_files = vec![PathBuf::from("/rip/audio/Album Image.flac")];

    assert!(resolve_cue_audio_path(Path::new("/rip/Album.cue"), "Album Image.wav", &audio_files)
        .is_none());
}

/// Same-stem fallback follows the reference's directory, not only the
/// directory containing the CUE.
#[test]
fn resolve_cue_audio_path_matches_by_stem_in_referenced_subdirectory() {
    let audio_files = vec![PathBuf::from("/rip/audio/Album Image.flac")];

    assert_eq!(
        resolve_cue_audio_path(Path::new("/rip/Album.cue"), "audio/Album Image.wav", &audio_files),
        Some(&PathBuf::from("/rip/audio/Album Image.flac")),
    );
}
