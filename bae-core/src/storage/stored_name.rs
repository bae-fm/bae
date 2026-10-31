//! The names bae stores for a release and writes files under: a file's name
//! in its release (`release_files.original_filename`, its names joined by
//! `/`) and the release's folder name (`releases.source_folder_name`, one
//! name).
//!
//! A stored name is a label, not a path. It is synced and shown, and the
//! device that wrote it took it from whatever its own file system holds, so
//! it may hold any character: a name that isn't UTF-8 is stored as its
//! escaped text (`Caf\xE9.flac`, from `coven::path_text`), whose `\` is a
//! separator on Windows, and Linux takes `:` and `?`, which Windows and FAT
//! drives refuse. Wherever bae writes a file under stored names (an export,
//! a save, a local copy) it asks coven for the path
//! ([`coven::RelativePath::local_path`]), which writes each name through the
//! one rule coven uses for its own local copies. Each name is then one valid
//! name inside the folder the person picked, on every platform, and the same
//! names land at the same path on every device: `Caf\xE9.flac` is written as
//! `Caf＼xE9.flac`, and `..` as `.．`.

use coven::{RelativePath, RelativePathError};
use std::path::{Path, PathBuf};

/// A stored name that names no file: it is empty, or one of its names is.
/// Carries the release and column it came from, because the value is data
/// another device may have written and the reader needs to know which row to
/// look at.
#[derive(Debug, thiserror::Error)]
#[error("release {release_id} {label} {value:?} names no file: {source}")]
pub struct StoredNameError {
    pub release_id: String,
    pub label: String,
    pub value: String,
    pub source: RelativePathError,
}

/// The path below a release's folder that a file's name in the release
/// gives: each of its `/`-separated names, whatever else it holds.
pub fn file_path(
    release_id: &str,
    file_id: &str,
    original_filename: &str,
) -> Result<RelativePath, StoredNameError> {
    RelativePath::new(original_filename.split('/')).map_err(|source| StoredNameError {
        release_id: release_id.to_string(),
        label: format!("original_filename for file {file_id}"),
        value: original_filename.to_string(),
        source,
    })
}

/// The path a release's folder name gives: one name, whatever it holds, `/`
/// included.
pub fn folder_path(release_id: &str, folder_name: &str) -> Result<RelativePath, StoredNameError> {
    RelativePath::new([folder_name]).map_err(|source| StoredNameError {
        release_id: release_id.to_string(),
        label: "source_folder_name".to_string(),
        value: folder_name.to_string(),
        source,
    })
}

/// A file of a release's output that couldn't be created new.
#[derive(Debug, thiserror::Error)]
pub enum CreateFileError {
    /// A file is already at its path: another of the release's files, whose
    /// name coven writes as this one's (`Track?.flac` and `Track？.flac`), or
    /// the drive holds as one (names differing only in case on macOS).
    #[error("{subject} is written at {}, where another file of the release already is", .shown.display())]
    Clash { subject: String, shown: PathBuf },
    #[error(transparent)]
    Io(std::io::Error),
}

/// Create a file of a release's output at `path`, never over a file already
/// there, so two of the release's files written at one path fail the output
/// rather than leave one file where the release has two. `subject` names the
/// file and `shown` its path below the output's folder, for the error.
pub fn create_new_file(
    path: &Path,
    shown: &Path,
    subject: &str,
) -> Result<std::fs::File, CreateFileError> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::AlreadyExists => CreateFileError::Clash {
                subject: subject.to_string(),
                shown: shown.to_path_buf(),
            },
            _ => CreateFileError::Io(error),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            written("Disc 1/01 - Song.flac"),
            Path::new("Disc 1").join("01 - Song.flac")
        );
    }

    /// Every name is written as coven writes it, the same on every
    /// platform: an escaped name's backslash is a character of its name,
    /// and `..` an ordinary name, so joined to a folder the path stays in
    /// it.
    #[test]
    fn a_name_is_written_as_coven_writes_it_on_every_platform() {
        assert_eq!(written(r"Caf\xE9.flac"), Path::new("Caf＼xE9.flac"));
        assert_eq!(
            written(r"CD1/Caf\xE9.flac"),
            Path::new("CD1").join("Caf＼xE9.flac")
        );
        assert_eq!(
            written("../escape.flac"),
            Path::new(".．").join("escape.flac")
        );
        assert_eq!(written(r"C:\escape.flac"), Path::new("C：＼escape.flac"));
        for name in [r"Caf\xE9.flac", "../escape.flac", r"C:\escape.flac", "a\0b"] {
            let path = written(name);
            assert!(
                path.components()
                    .all(|component| matches!(component, std::path::Component::Normal(_))),
                "{name:?} is written as names only: {path:?}"
            );
        }
    }

    #[test]
    fn a_folder_name_is_one_name_whatever_it_holds() {
        let written = |name| folder_path(RELEASE, name).unwrap().local_path();
        assert_eq!(
            written("Album Title (2020)"),
            Path::new("Album Title (2020)")
        );
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
}
