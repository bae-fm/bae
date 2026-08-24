//! Folder scanner for import release candidates.
//!
//! Each directory with direct audio approximates one release. When a directory
//! also contains audio-bearing descendants, the scanner reports an unresolved
//! boundary instead of guessing whether the parent or descendants are releases.
//! The walk lists one directory at a time and reports candidates as they become
//! available.
use super::file_validation;
use crate::cue_flac::{parse_cue_sheet, CueSheet};
use crate::util::content_type_hint::ContentTypeHint;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{debug, info};

mod boundary;
mod candidates;
mod categorize;
mod files;
mod scan;
pub use candidates::*;
pub use categorize::is_audio_file;
pub(crate) use categorize::is_cover_name;
use categorize::*;
pub use files::*;
pub use scan::*;

const DOCUMENT_EXTENSIONS: &[&str] = &["cue", "log", "txt", "m3u", "m3u8"];

/// Extensions used by download clients and browsers to mark an
/// in-progress download. Presence of any of these anywhere in a folder means
/// the folder is mid-download and should not surface as an import candidate.
const PARTIAL_MARKER_EXTENSIONS: &[&str] = &["part", "crdownload", "download", "aria2", "partial"];

// ── Candidate file index ────────────────────────────────────────────────────

/// The audio file a CUE pairs with. The literal `FILE` path wins. When that is
/// absent, a single-file sheet may use the unique same-stem audio in the CUE's
/// own directory.
pub(crate) fn find_matching_audio_for_cue<'a>(
    cue_path: &Path,
    sheet: &CueSheet,
    audio_files: &'a [PathBuf],
) -> Option<&'a PathBuf> {
    let file_reference = sheet.single_file()?;
    let cue_dir = cue_path.parent()?;
    let exact_path = cue_dir.join(file_reference);
    if let Some(exact) = audio_files
        .iter()
        .find(|path| path.as_path() == exact_path && ContentTypeHint::path_is_audio(path))
    {
        return Some(exact);
    }
    let file_stem = Path::new(file_reference).file_stem()?.to_str()?;
    let mut matches = audio_files.iter().filter(|path| {
        ContentTypeHint::path_is_audio(path)
            && path.parent() == Some(cue_dir)
            && path.file_stem().and_then(|stem| stem.to_str()) == Some(file_stem)
    });
    let matched = matches.next()?;
    if matches.next().is_some() {
        debug!(
            "CUE {:?} has more than one same-stem audio file beside it",
            cue_path
        );
        return None;
    }
    Some(matched)
}

/// A single file entry in a candidate's selected file set.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// Path relative to the scan root
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
}

/// A pre-collected list of files for release candidate detection.
///
/// Built by walking the filesystem once.
pub struct CandidateFileIndex {
    files: Vec<FileEntry>,
    dirs: BTreeMap<PathBuf, DirEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum FolderScanError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// The scan root exists but is a file, not a directory. A distinct variant
    /// so the message is stable across platforms — `read_dir` on a file reports
    /// "Not a directory" on Unix but "The directory name is invalid" on Windows,
    /// and the import service surfaces this text straight to the user.
    #[error("not a directory: {}", path.display())]
    NotADirectory { path: PathBuf },
    #[error("folder scan cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl FolderScanError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

#[derive(Debug, Default)]
struct DirEntry {
    files: Vec<usize>,
    subdirs: BTreeSet<PathBuf>,
}

impl CandidateFileIndex {
    pub fn new(files: Vec<FileEntry>) -> Self {
        let mut tree = Self {
            files,
            dirs: BTreeMap::new(),
        };
        tree.rebuild_index();
        tree
    }

    /// All files recursively under `dir` (inclusive).
    fn all_files_under<'a>(&'a self, dir: &Path) -> impl Iterator<Item = &'a FileEntry> {
        let mut indices = Vec::new();
        self.collect_file_indices_under(&Self::normalize_dir(dir), &mut indices);
        indices.into_iter().map(|idx| &self.files[idx])
    }

    fn rebuild_index(&mut self) {
        self.dirs.clear();
        self.dirs.entry(PathBuf::new()).or_default();

        for idx in 0..self.files.len() {
            let parent = {
                let file = &self.files[idx];
                Self::normalize_dir(file.path.parent().unwrap_or_else(|| Path::new("")))
            };
            self.dirs.entry(parent.clone()).or_default().files.push(idx);
            self.index_dir_path(&parent);
        }
    }

    fn index_dir_path(&mut self, dir: &Path) {
        let mut parent = PathBuf::new();
        self.dirs.entry(parent.clone()).or_default();

        for component in dir.components() {
            let child = parent.join(component.as_os_str());
            self.dirs
                .entry(parent.clone())
                .or_default()
                .subdirs
                .insert(child.clone());
            self.dirs.entry(child.clone()).or_default();
            parent = child;
        }
    }

    fn collect_file_indices_under(&self, dir: &Path, out: &mut Vec<usize>) {
        let Some(entry) = self.dirs.get(dir) else {
            return;
        };

        out.extend(entry.files.iter().copied());
        for subdir in &entry.subdirs {
            self.collect_file_indices_under(subdir, out);
        }
    }

    fn normalize_dir(dir: &Path) -> PathBuf {
        if dir.as_os_str().is_empty() || dir == Path::new(".") {
            PathBuf::new()
        } else {
            dir.to_path_buf()
        }
    }
}

#[cfg(test)]
mod tests;
