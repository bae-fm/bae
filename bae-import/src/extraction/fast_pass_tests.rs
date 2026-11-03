use super::*;

#[test]
fn enumerate_filename_inputs_skips_all_audio_and_cue() {
    use crate::folder_scanner::{CandidateFile, FileRole, ScannedFile};

    let cue = ScannedFile::new(PathBuf::from("/rel/Album.cue"), "Album.cue".to_string(), 100, 1);
    let flac =
        ScannedFile::new(PathBuf::from("/rel/Album.flac"), "Album.flac".to_string(), 5_000_000, 1)
            .with_test_flac_audio();
    let cover = ScannedFile::new(
        PathBuf::from("/rel/Artist Name - Album.png"),
        "Artist Name - Album.png".to_string(),
        10_000,
        1,
    );
    let categorized = CategorizedFiles {
        files: vec![
            CandidateFile {
                file: cue.clone(),
                role: FileRole::TrackSheet {
                    sheet: crate::cue_flac::CueSheet {
                        title: None,
                        performer: None,
                        catalog: None,
                        date: None,
                        ripper: None,
                        tracks: Vec::new(),
                    },
                    binding: crate::folder_scanner::SheetBinding::Resolved {
                        files: vec![crate::folder_scanner::SheetAudioFile {
                            file_reference: "Album.wav".to_string(),
                            file_id: "Album.flac".to_string(),
                        }],
                    },
                    disc: crate::folder_scanner::SheetDisc::Disc { number: 1 },
                },
            },
            CandidateFile { file: flac.clone(), role: FileRole::Audio },
            CandidateFile { file: cover.clone(), role: FileRole::Artwork },
        ],
        parts: Vec::new(),
    };

    let inputs: Vec<PathBuf> = enumerate_filename_inputs(&categorized);
    assert!(
        !inputs.iter().any(|p| p == &cue.path),
        "CUE names come from the parsed sheet, not the filename pool; got {inputs:?}",
    );
    assert!(
        !inputs.iter().any(|p| p == &flac.path),
        "audio filename stems are almost always track titles — wrong pool \
         for Artist / Album autocomplete; got {inputs:?}",
    );
    assert!(
        inputs.iter().any(|p| p == &cover.path),
        "artwork filenames still contribute — `Artist Name - Album.png` \
         and similar carry real signal; got {inputs:?}",
    );
}

/// One categorize yields both the disc ID (from the LOG) and the real track
/// count — the pair the folder fast pass reads.
#[test]
fn test_categorized_yields_discid_and_track_count() {
    use tempfile::TempDir;

    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();

    let fixture_log = std::path::Path::new("tests/fixtures/logs/test_album.log");
    std::fs::copy(fixture_log, dir.join("test_album.log")).unwrap();

    let fixture_dir = std::path::Path::new("tests/fixtures/flac");
    std::fs::copy(fixture_dir.join("01 Test Track 1.flac"), dir.join("01 Test Track 1.flac"))
        .unwrap();
    std::fs::copy(fixture_dir.join("02 Test Track 2.flac"), dir.join("02 Test Track 2.flac"))
        .unwrap();

    let categorized = crate::folder_scanner::collect_release_candidate_files_with_scope(
        dir,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let disc_id = crate::discid::read_rip_artifacts(&categorized).disc_id.computed();
    let track_count = categorized.track_count();

    assert_eq!(disc_id.len(), 1, "LOG fixture should produce a Disc ID");
    assert_eq!(track_count, 2, "track_count must equal the number of audio files, not 0");
}
