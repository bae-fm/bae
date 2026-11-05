use super::*;
use std::path::{Path, PathBuf};

const RELEASE: &str = "af63ef4c-8602-4cd5-82c0-3d334b916305";
const FILE: &str = "0f7d1d5e-55a8-4d49-9a43-5a5c0d4b0f43";

fn written(original_filename: &str) -> PathBuf {
    file_path(RELEASE, FILE, original_filename)
        .unwrap_or_else(|error| panic!("{original_filename:?}: {error}"))
        .local_path()
}

#[test]
fn a_files_names_are_its_folders_and_its_own_name() {
    assert_eq!(written("track.flac"), Path::new("track.flac"));
    assert_eq!(written("Disc 1/01 - Song.flac"), Path::new("Disc 1").join("01 - Song.flac"));
}

/// Every name is written as coven writes it, the same on every
/// platform: an escaped name's backslash is a character of its name,
/// and `..` an ordinary name, so joined to a folder the path stays in
/// it.
#[test]
fn a_name_is_written_as_coven_writes_it_on_every_platform() {
    assert_eq!(written(r"Caf\xE9.flac"), Path::new("Caf＼xE9.flac"));
    assert_eq!(written(r"CD1/Caf\xE9.flac"), Path::new("CD1").join("Caf＼xE9.flac"));
    assert_eq!(written("../escape.flac"), Path::new(".．").join("escape.flac"));
    assert_eq!(written(r"C:\escape.flac"), Path::new("C：＼escape.flac"));
    for name in [r"Caf\xE9.flac", "../escape.flac", r"C:\escape.flac", "a\0b"] {
        let path = written(name);
        assert!(
            path.components().all(|component| matches!(component, std::path::Component::Normal(_))),
            "{name:?} is written as names only: {path:?}"
        );
    }
}

#[test]
fn a_folder_name_is_one_name_whatever_it_holds() {
    let written = |name| folder_path(RELEASE, name).unwrap().local_path();
    assert_eq!(written("Album Title (2020)"), Path::new("Album Title (2020)"));
    assert_eq!(written("../escape"), Path::new("..／escape"));
}

/// A name with no names, or an empty one (which an absolute path's
/// leading `/` gives), names no file.
#[test]
fn an_empty_name_names_no_file() {
    for bad in ["", "/etc/passwd", "CD1//track.flac", "CD1/"] {
        let error = file_path(RELEASE, FILE, bad).expect_err(bad);
        assert_eq!(error.source, RelativePathError::Empty);
    }
    folder_path(RELEASE, "").expect_err("an empty folder name");
}

/// A file's extension is its own name's, whatever dots its folders hold.
#[test]
fn a_file_s_extension_is_its_own_name_s() {
    assert_eq!(file_extension("Disc 1/Track.FLAC").as_deref(), Some("flac"));
    assert_eq!(file_extension("Album.v2/Track.tar.gz").as_deref(), Some("gz"));
    assert_eq!(file_extension("Disc.1/track"), None);
    assert_eq!(file_extension("track"), None);
    assert_eq!(file_extension(".hidden"), None);
    assert_eq!(file_extension("track."), None);
}
