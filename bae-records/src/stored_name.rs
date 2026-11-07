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

/// The path below a person's folder that a local copy of a release's file
/// goes to: the release's folder name, then the file's names.
pub fn copy_path(
    release_id: &str,
    folder_name: &str,
    file_id: &str,
    original_filename: &str,
) -> Result<RelativePath, StoredNameError> {
    let folder = folder_path(release_id, folder_name)?;
    let file = file_path(release_id, file_id, original_filename)?;
    Ok(RelativePath::new(folder.names().chain(file.names()))
        .expect("two paths of non-empty names join to one"))
}

/// The extension a file's name in its release ends on, lowercased and
/// without its dot: its own name's (not a folder's above it) text after its
/// last dot. `None` for a name with no dot past its first character (`track`,
/// `.hidden`) or with nothing after it.
pub fn file_extension(original_filename: &str) -> Option<String> {
    let name = original_filename.rsplit('/').next().unwrap_or(original_filename);
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty() && !extension.is_empty()).then(|| extension.to_lowercase())
}

#[cfg(test)]
#[path = "stored_name_tests.rs"]
mod tests;
