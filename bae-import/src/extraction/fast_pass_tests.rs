use super::*;
use crate::test_fixtures::{flac_dir, read_fixture};
use bae_identify::signals::TextOrigin;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

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
                    sheet: crate::cue_sheet::CueSheet {
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
        !inputs.iter().any(|p| p == &cue.version.path),
        "CUE names come from the parsed sheet, not the filename pool; got {inputs:?}",
    );
    assert!(
        !inputs.iter().any(|p| p == &flac.version.path),
        "audio filename stems are almost always track titles — wrong pool \
         for Artist / Album autocomplete; got {inputs:?}",
    );
    assert!(
        inputs.iter().any(|p| p == &cover.version.path),
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
    let disc_id =
        crate::rip_reading::read_rip_artifacts(&categorized, &bae_util::test_files::lease())
            .unwrap()
            .disc_id
            .computed();
    let track_count = categorized.track_count();

    assert_eq!(disc_id.len(), 1, "LOG fixture should produce a Disc ID");
    assert_eq!(track_count, 2, "track_count must equal the number of audio files, not 0");
}

#[test]
fn non_utf8_cue_is_decoded_not_dropped() {
    // A Windows-1252 CUE's curly apostrophe (0x92) is decoded, not the sheet dropped.
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("audio.flac"), fixture_flac()).unwrap();

    let mut cue: Vec<u8> = Vec::new();
    cue.extend_from_slice(b"PERFORMER \"Artist Alpha\"\n");
    cue.extend_from_slice(b"TITLE \"Album Title A\"\n");
    cue.extend_from_slice(b"FILE \"audio.flac\" WAVE\n");
    cue.extend_from_slice(b"  TRACK 01 AUDIO\n");
    cue.extend_from_slice(b"    TITLE \"I Ain");
    cue.push(0x92); // Windows-1252 right single quotation mark
    cue.extend_from_slice(b"t Got No Heart\"\n");
    cue.extend_from_slice(b"    INDEX 01 00:00:00\n");
    fs::write(folder.join("Album.cue"), &cue).unwrap();

    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    let pass = gather_non_ocr_sources(&[folder], &files, &bae_util::test_files::lease())
        .expect("scanned fixture audio has complete timing");
    let texts: Vec<&str> = pass.lines.iter().map(|l| l.text.as_str()).collect();

    assert!(texts.contains(&"Artist Alpha"), "non-UTF-8 CUE dropped entirely; got {texts:?}",);
    assert!(
        texts.iter().any(|t| t.starts_with("I Ain") && t.ends_with("t Got No Heart")),
        "non-ASCII track title not recovered; got {texts:?}",
    );
}

/// A folder whose name isn't UTF-8 still gives its name, as it is shown, and
/// the catalog number in its brackets. Only the folder's name is read from
/// its path; its files are read from the scan.
#[cfg(unix)]
#[test]
fn a_folder_whose_name_is_not_utf8_gives_its_name_and_bracketed_catalog() {
    use std::os::unix::ffi::OsStrExt;

    let tmp = TempDir::new().unwrap();
    let scanned = tmp.path().join("Some Folder");
    fs::create_dir_all(&scanned).unwrap();
    fs::write(scanned.join("audio.flac"), fixture_flac()).unwrap();
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &scanned,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    let folder = tmp.path().join(std::ffi::OsStr::from_bytes(b"Caf\xE9 [LABELA071]"));

    let pass = gather_non_ocr_sources(&[folder], &files, &bae_util::test_files::lease())
        .expect("scanned fixture audio has complete timing");

    let texts: Vec<&str> = pass.lines.iter().map(|l| l.text.as_str()).collect();
    assert!(texts.contains(&r"Caf\xE9 [LABELA071]"), "the folder's name is dropped: {texts:?}");
    assert_eq!(pass.bracket_catalogs, ["LABELA071"]);
}

/// A FLAC carrying these Vorbis comments.
fn tagged_flac(path: &Path, comments: &[(&str, &str)]) {
    use lofty::config::WriteOptions;
    use lofty::tag::TagExt;
    fs::write(path, fixture_flac()).unwrap();
    let mut tag = lofty::ogg::tag::VorbisComments::default();
    for (key, value) in comments {
        tag.push((*key).to_string(), (*value).to_string());
    }
    tag.save_to_path(path, WriteOptions::default()).unwrap();
}

#[test]
fn the_audio_s_tags_feed_the_text_and_carry_their_isrcs() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    for (name, isrc) in [("01.flac", "it-000-00-00001"), ("02.flac", "IT0000000002")] {
        tagged_flac(
            &folder.join(name),
            &[
                ("ALBUM", "Album Title"),
                ("ARTIST", "Artist Name"),
                ("LABEL", "Imprint Name"),
                ("ISRC", isrc),
            ],
        );
    }
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    let pass = gather_non_ocr_sources(&[folder], &files, &bae_util::test_files::lease())
        .expect("the fixture audio times");

    let tagged: Vec<&str> = pass
        .lines
        .iter()
        .filter(|line| line.source.text_origin() == TextOrigin::FileTag)
        .map(|line| line.text.as_str())
        .collect();
    assert_eq!(tagged, vec!["Album Title", "Artist Name", "Imprint Name"]);
    assert_eq!(pass.isrcs, vec!["IT0000000001", "IT0000000002"]);
}

/// Read a folder of two FLACs carrying these comments, with `extra` files
/// beside them.
fn origin_of(comments: &[(&str, &str)], extra: &[(&str, &[u8])]) -> bae_catalogs::AudioOrigin {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    for name in ["01.flac", "02.flac"] {
        tagged_flac(&folder.join(name), comments);
    }
    for (name, bytes) in extra {
        fs::write(folder.join(name), bytes).unwrap();
    }
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    gather_non_ocr_sources(&[folder], &files, &bae_util::test_files::lease())
        .expect("the fixture audio times")
        .origin
}

const DELIVERED: &[(&str, &str)] = &[
    ("ISRC", "IT0000000001"),
    ("LABEL", "Imprint Name"),
    ("COPYRIGHT", "(C) 1999 Imprint Name (P) 1999 Imprint Name"),
];

/// A label's delivery set on every track proves a download; a ℗ line counts
/// as a "(P)" one.
#[test]
fn a_label_s_delivery_set_on_every_track_proves_a_download() {
    use bae_catalogs::{AudioSource, DownloadProof};
    assert_eq!(
        origin_of(DELIVERED, &[]).source,
        Some(AudioSource::Download(DownloadProof::DeliverySet))
    );
    assert_eq!(
        origin_of(
            &[
                ("ISRC", "IT0000000001"),
                ("LABEL", "Imprint Name"),
                ("COPYRIGHT", "℗ 1999 Imprint Name"),
            ],
            &[]
        )
        .source,
        Some(AudioSource::Download(DownloadProof::DeliverySet))
    );
}

/// The set proves nothing with a part missing, nor beside a rip log or a
/// track sheet, which say a disc was read — a ripper puts the disc's ISRCs
/// in the tags too.
#[test]
fn a_delivery_set_with_a_part_missing_or_beside_a_rip_file_proves_nothing() {
    assert_eq!(origin_of(&DELIVERED[..2], &[]).source, None);
    assert_eq!(
        origin_of(
            &[
                ("ISRC", "IT0000000001"),
                ("LABEL", "Imprint Name"),
                ("COPYRIGHT", "(C) 1999 Imprint Name"),
            ],
            &[]
        )
        .source,
        None
    );
    assert_eq!(origin_of(DELIVERED, &[("rip.log", b"not a log")]).source, None);
    assert_eq!(origin_of(DELIVERED, &[("Album.cue", b"FILE \"01.flac\" WAVE\n")]).source, None);
}

/// A store's own marker proves a download, and names the track it is on.
#[test]
fn a_store_s_marker_proves_a_download() {
    use bae_catalogs::{AudioSource, DownloadProof, StoreMarker};
    assert_eq!(
        origin_of(&[("COMMENT", "Visit https://artistname.bandcamp.com")], &[]).source,
        Some(AudioSource::Download(DownloadProof::Store {
            marker: StoreMarker::Bandcamp,
            file: "01.flac".to_string(),
        }))
    );
}

/// A ripper's log and a store's marker in one folder: the log was made by
/// reading a disc, and a tag can be copied, so the rip is what it proves.
#[test]
fn a_rip_log_outweighs_a_store_s_marker() {
    let log = fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/logs/test_album.log"))
        .unwrap();
    assert!(matches!(
        origin_of(&[("COMMENT", "Visit https://artistname.bandcamp.com")], &[("Album.log", &log)])
            .source,
        Some(bae_catalogs::AudioSource::CdRip { proof: bae_catalogs::CdProof::RipLog, .. })
    ));
}

/// Each track is titled by its file's title tag, or failing that by its file
/// name without the track number.
#[test]
fn each_track_is_titled_by_its_tag_or_its_file_name() {
    let tmp = TempDir::new().unwrap();
    let folder = tmp.path().join("Some Folder");
    fs::create_dir_all(&folder).unwrap();
    tagged_flac(&folder.join("01 - First File.flac"), &[("TITLE", "Song One")]);
    tagged_flac(&folder.join("02 - Song Two.flac"), &[]);
    let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
        &folder,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .expect("candidate scan");
    let pass = gather_non_ocr_sources(&[folder], &files, &bae_util::test_files::lease())
        .expect("the fixture audio times");

    assert_eq!(pass.track_titles, vec!["Song One", "Song Two"]);
}

/// Files nothing numbers are laid out in an order nothing states, so their
/// titles are not given in it; nor are files whose numbers repeat. Files the
/// tags number against their names' order are laid out by the tags, and give
/// their titles in that order.
#[test]
fn unnumbered_files_give_no_titles() {
    let titles_of = |files: &[(&str, &[(&str, &str)])]| {
        let tmp = TempDir::new().unwrap();
        let folder = tmp.path().join("Some Folder");
        fs::create_dir_all(&folder).unwrap();
        for (name, comments) in files {
            tagged_flac(&folder.join(name), comments);
        }
        let files = crate::folder_scanner::collect_release_candidate_files_with_scope(
            &folder,
            crate::ReleaseFileScope::Recursive,
            &crate::folder_scanner::StoredCandidateEdits::none(),
        )
        .expect("candidate scan");
        gather_non_ocr_sources(&[folder], &files, &bae_util::test_files::lease())
            .expect("the fixture audio times")
            .track_titles
    };
    assert!(titles_of(&[("Song A.flac", &[]), ("Song B.flac", &[])]).is_empty());
    assert!(titles_of(&[
        ("a.flac", &[("TITLE", "Song One"), ("TRACKNUMBER", "1")]),
        ("b.flac", &[("TITLE", "Song Two"), ("TRACKNUMBER", "1")]),
    ])
    .is_empty());
    assert_eq!(
        titles_of(&[
            ("a.flac", &[("TITLE", "Song Two"), ("TRACKNUMBER", "2")]),
            ("b.flac", &[("TITLE", "Song One"), ("TRACKNUMBER", "1")]),
        ]),
        vec!["Song One", "Song Two"]
    );
    assert_eq!(
        titles_of(&[
            ("a.flac", &[("TITLE", "Song One"), ("TRACKNUMBER", "1")]),
            ("b.flac", &[("TITLE", "Song Two"), ("TRACKNUMBER", "2")]),
        ]),
        vec!["Song One", "Song Two"]
    );
}

/// The first FLAC test track's bytes.
fn fixture_flac() -> Vec<u8> {
    read_fixture(&flac_dir().join("01 Test Track 1.flac"))
}

/// A text file that cannot be read fails the pass: its lines would be clues
/// the pass never saw.
#[test]
fn a_text_file_that_cannot_be_read_fails_the_pass() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path();
    for name in ["01 Test Track 1.flac", "02 Test Track 2.flac"] {
        fs::copy(Path::new("tests/fixtures/flac").join(name), dir.join(name)).unwrap();
    }
    fs::write(dir.join("notes.txt"), "Catalog LBL-1001\n").unwrap();
    let categorized = crate::folder_scanner::collect_release_candidate_files_with_scope(
        dir,
        crate::ReleaseFileScope::Recursive,
        &crate::folder_scanner::StoredCandidateEdits::none(),
    )
    .unwrap();
    let folders = [dir.to_path_buf()];
    gather_non_ocr_sources(&folders, &categorized, &bae_util::test_files::lease())
        .expect("the notes read while they are there");

    fs::remove_file(dir.join("notes.txt")).unwrap();
    let error = gather_non_ocr_sources(&folders, &categorized, &bae_util::test_files::lease())
        .err()
        .expect("a text file that left the folder fails the pass");
    assert!(
        matches!(error, crate::ImportError::DocumentRead { ref file, .. } if file.ends_with("notes.txt")),
        "{error:?}"
    );
}
