//! Reading one directory of a watched folder: its files and folders in
//! natural name order, hidden entries and noise files left out.

use super::*;

// ── Progressive directory walker ───────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct DirectoryListing {
    pub(super) files: Vec<FileEntry>,
    pub(super) directories: Vec<PathBuf>,
}

pub(crate) trait DirectoryReader: Send + Sync {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError>;

    /// The dates of each of `folders`, directly under `root` and relative to
    /// it, in their order: what a pass walks them newest first by. A folder
    /// whose dates can't be read has none; only a cancelled pass fails this.
    fn folder_times(
        &self,
        root: &Path,
        folders: &[PathBuf],
        cancellation: &ScanCancellation,
    ) -> Result<Vec<FolderTimes>, FolderScanError>;
}

pub(crate) struct OsDirectoryReader;

impl DirectoryReader for OsDirectoryReader {
    fn read(
        &self,
        root: &Path,
        directory: &Path,
        cancellation: &ScanCancellation,
    ) -> Result<DirectoryListing, FolderScanError> {
        let absolute = root.join(directory);
        let entries =
            fs::read_dir(&absolute).map_err(|source| FolderScanError::io(&absolute, source))?;
        let mut files = Vec::new();
        let mut directories = Vec::new();
        for entry in entries {
            cancellation.check()?;
            let entry = entry.map_err(|source| FolderScanError::io(&absolute, source))?;
            let path = entry.path();
            let name = entry.file_name();
            if is_hidden_entry(&name) {
                debug!("ignoring hidden folder-scan entry {}", path.display());
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| FolderScanError::Other(error.to_string()))?
                .to_path_buf();
            let file_type =
                entry.file_type().map_err(|source| FolderScanError::io(&path, source))?;
            // The entry's metadata, or `None` for a folder.
            let metadata = if file_type.is_dir() {
                None
            } else {
                let metadata =
                    entry.metadata().map_err(|source| FolderScanError::io(&path, source))?;
                (!metadata.is_dir() || file_type.is_symlink()).then_some(metadata)
            };
            // A folder is known by its path's bytes, so one whose name isn't
            // valid Unicode is walked like any other.
            let Some(metadata) = metadata else {
                directories.push(relative);
                continue;
            };
            if metadata.is_file() && !is_noise_file(&path) {
                files.push(FileEntry {
                    path: relative,
                    size: metadata.len(),
                    modified_at_ns: file_modified_at_ns(&path, &metadata)?,
                });
            }
        }
        let compare = |left: &PathBuf, right: &PathBuf| {
            let shown = |path: &PathBuf| {
                coven::path_text(Path::new(
                    path.file_name().expect("a directory entry path has a file name"),
                ))
            };
            natord::compare_ignore_case(&shown(left), &shown(right))
        };
        files.sort_by(|left, right| compare(&left.path, &right.path));
        directories.sort_by(compare);
        Ok(DirectoryListing { files, directories })
    }

    fn folder_times(
        &self,
        root: &Path,
        folders: &[PathBuf],
        cancellation: &ScanCancellation,
    ) -> Result<Vec<FolderTimes>, FolderScanError> {
        super::folder_times::times_of_folders(root, folders, cancellation)
    }
}

/// The stamp a file is compared by: its modification time as nanoseconds since
/// the Unix epoch, which is how the scan records it and how SQLite stores it.
pub(crate) fn file_modified_at_ns(
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<i64, FolderScanError> {
    let modified = metadata.modified().map_err(|source| FolderScanError::io(path, source))?;
    let elapsed = modified.duration_since(std::time::UNIX_EPOCH).map_err(|error| {
        FolderScanError::Other(format!(
            "modification time is before the Unix epoch: {}: {error}",
            path.display()
        ))
    })?;
    i64::try_from(elapsed.as_nanos()).map_err(|error| {
        FolderScanError::Other(format!(
            "modification time exceeds SQLite's integer range: {}: {error}",
            path.display()
        ))
    })
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
