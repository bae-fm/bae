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

use coven::RelativePath;

/// What every read of a stored name relies on: the tables refuse a name that
/// names no file (`release_files.original_filename` and
/// `releases.source_folder_name` each have a CHECK), so a name a row holds is
/// one.
const VALID_BY_CHECK: &str = "a stored name is non-empty by its table's CHECK";

/// The path below a release's folder that a file's name in the release
/// gives: each of its `/`-separated names, whatever else it holds.
pub fn file_path(original_filename: &str) -> RelativePath {
    RelativePath::new(original_filename.split('/')).expect(VALID_BY_CHECK)
}

/// The path a release's folder name gives: one name, whatever it holds, `/`
/// included.
pub fn folder_path(folder_name: &str) -> RelativePath {
    RelativePath::new([folder_name]).expect(VALID_BY_CHECK)
}

/// The path below a person's folder that a local copy of a release's file
/// goes to: the release's folder name, then the file's names.
pub fn copy_path(folder_name: &str, original_filename: &str) -> RelativePath {
    let folder = folder_path(folder_name);
    let file = file_path(original_filename);
    RelativePath::new(folder.names().chain(file.names())).expect(VALID_BY_CHECK)
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
