//! Creating the files of a release's output: an export, a save or a local
//! copy, each written under the release's stored names
//! ([`bae_records::stored_name`]).

use std::path::{Path, PathBuf};

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
    std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|error| match error
        .kind()
    {
        std::io::ErrorKind::AlreadyExists => {
            CreateFileError::Clash { subject: subject.to_string(), shown: shown.to_path_buf() }
        }
        _ => CreateFileError::Io(error),
    })
}
