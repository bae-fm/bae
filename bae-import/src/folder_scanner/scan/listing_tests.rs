//! Reading one directory: names that are not UTF-8 listed under their
//! escaped names, hidden entries and bae's own files left out, and files in
//! natural name order.

use crate::folder_scanner::tests::*;
#[cfg(target_os = "linux")]
use crate::folder_scanner::{FileRole, InvalidReason, ProposedRole, ScanItem};
#[cfg(target_os = "linux")]
use std::path::PathBuf;

// ── Names that aren't UTF-8 ─────────────────────────────────────────────────
//
// Only Linux takes such a name: macOS and Windows store names as Unicode.

/// A file whose name isn't UTF-8 is read like any other: the scan lists its
/// release, with the file under its escaped name and its path as it is.
#[cfg(target_os = "linux")]
#[test]
fn a_file_whose_name_is_not_utf8_is_listed_under_its_escaped_name() {
    let (tmp, album) = album_dir();
    let track = album.join(latin1_name(".flac"));
    std::fs::write(&track, fake_flac()).unwrap();

    let candidates = scan_valid(tmp.path());

    let [candidate] = candidates.as_slice() else {
        panic!("one release: {candidates:?}");
    };
    assert_eq!(candidate.name, "Album");
    let [entry] = candidate.files.files.as_slice() else {
        panic!("one file: {:?}", candidate.files.files);
    };
    assert_eq!(entry.role, FileRole::Audio);
    assert_eq!(entry.file.relative_path, r"Caf\xE9.flac");
    assert_eq!(entry.file.file_name, r"Caf\xE9.flac");
    assert_eq!(entry.file.path, track);
}

/// A name spelled `Caf\xE9.flac` and a Latin-1 `Café.flac` show the same, and
/// a release holds each name once: the folder is skipped, naming it.
#[cfg(target_os = "linux")]
#[test]
fn two_files_showing_one_name_skip_their_folder() {
    let (tmp, album) = album_dir();
    std::fs::write(album.join(r"Caf\xE9.flac"), fake_flac()).unwrap();
    std::fs::write(album.join(latin1_name(".flac")), fake_flac()).unwrap();

    let items = scan_items(tmp.path());

    let [ScanItem::Invalid(invalid)] = items.as_slice() else {
        panic!("one skipped folder: {items:?}");
    };
    assert_eq!(invalid.reason(), InvalidReason::SameName { name: r"Caf\xE9.flac".to_string() });
    let mut paths: Vec<PathBuf> = invalid.files.iter().map(|file| file.file.path.clone()).collect();
    paths.sort();
    let mut expected = [album.join(r"Caf\xE9.flac"), album.join(latin1_name(".flac"))];
    expected.sort();
    assert_eq!(paths, expected);
    assert!(invalid
        .files
        .iter()
        .all(|file| file.relative_path == r"Caf\xE9.flac" && file.role == ProposedRole::Audio));
}

/// A folder whose name isn't UTF-8 is read like any other: the scan lists the
/// release in it, keyed by its path as it is, its name and its place below the
/// watched folder shown escaped and stored as the name's bytes.
#[cfg(target_os = "linux")]
#[test]
fn a_folder_whose_name_is_not_utf8_is_listed_under_its_escaped_name() {
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Artist").join(latin1_name(""));
    std::fs::create_dir_all(&album).unwrap();
    std::fs::write(album.join("01.flac"), fake_flac()).unwrap();

    let candidates = scan_valid(tmp.path());

    let [candidate] = candidates.as_slice() else {
        panic!("one release: {candidates:?}");
    };
    assert_eq!(candidate.path, album);
    assert_eq!(candidate.key(), crate::CandidateKey::folder(&album));
    assert_eq!(candidate.name, r"Caf\xE9");
    assert_eq!(candidate.relative_folder.as_bytes(), b"Artist/Caf\xE9");
    assert_eq!(candidate.relative_folder.text(), r"Artist/Caf\xE9");
    let [entry] = candidate.files.files.as_slice() else {
        panic!("one file: {:?}", candidate.files.files);
    };
    assert_eq!(entry.file.path, album.join("01.flac"));
    assert_eq!(entry.file.relative_path, "01.flac");
}

#[test]
fn test_collect_release_candidate_files_skips_hidden_and_bae() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();

    // Create visible files
    std::fs::write(root.join("track.flac"), fake_flac()).unwrap();
    std::fs::write(root.join("cover.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(root.join("back.bmp"), b"BMvalid bmp marker").unwrap();

    // Create hidden file that should be ignored
    std::fs::write(root.join(".DS_Store"), b"mac junk").unwrap();

    // Create .bae directory -- entirely ignored by the scanner
    let bae_dir = root.join(".bae");
    std::fs::create_dir(&bae_dir).unwrap();
    std::fs::write(bae_dir.join("cover-mb.jpg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
    std::fs::write(bae_dir.join("cover-discogs.jpeg"), [0xFF, 0xD8, 0xFF, 0xE0]).unwrap();

    let files = scan_files(root);

    let audio_paths: Vec<_> = files.audio().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(audio_paths, vec!["track.flac"]);

    // Only release artwork, not .bae/ files
    let artwork_paths: Vec<_> = files.artwork().map(|f| f.relative_path.as_str()).collect();
    assert_eq!(artwork_paths, vec!["back.bmp", "cover.jpg"]);

    assert_eq!(files.documents().count(), 0);
}

/// The `.bae/` subdirectory is entirely hidden from the scanner.
#[test]
fn bae_sidecar_hidden_from_scanner() {
    let mut entries = flat_audio("Album", 3, FileKind::Flac);
    entries.push(file("Album/.bae/cover-mb.jpg", FileKind::Jpeg));
    let result = run_scenario(entries);
    assert_eq!(result.top_level_paths(), vec!["Album"]);
    let c = result.candidate("Album");
    // Nothing under .bae/ should leak into artwork or documents.
    assert!(c.files.artwork().all(|a| !a.relative_path.contains(".bae/")));
    assert!(c.files.documents().all(|d| !d.relative_path.contains(".bae/")));
}

/// The folder lists its files the way a person reads them: natural order
/// (`2` before `10`) and case-insensitive, so `cover.jpg` sits among the
/// names starting with `c`, not after every capitalized one; names alike but
/// for spaces fall in byte order, as the library lists them.
#[test]
fn files_list_in_case_insensitive_natural_order() {
    let (_tmp, album) = album_dir();
    for name in [
        "Track 10.flac",
        "cover.jpg",
        "Track1.flac",
        "Track 2.flac",
        "Back.jpg",
        "Track 1.flac",
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
    let names: Vec<&str> =
        files.files.iter().map(|entry| entry.file.relative_path.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "Back.jpg",
            "booklet.pdf",
            "cover.jpg",
            "Track 1.flac",
            "Track1.flac",
            "Track 2.flac",
            "Track 10.flac",
        ]
    );
}
