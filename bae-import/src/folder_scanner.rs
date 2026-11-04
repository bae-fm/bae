//! Folder scanner for import release candidates.
//!
//! Each directory with direct audio approximates one release. A directory
//! whose children hold the audio is read as one release or as separate ones:
//! by the reading stored for it, or, with none, by the scan's own reading of
//! its children (see [`heuristic_folder_release_decision`]). A pass reads several
//! of the folders directly under the watched root at once, newest first by the
//! latest date each has, each a directory at a time, and reports candidates as
//! they become available.
use super::file_validation;
use crate::cue_flac::{parse_cue_sheet, CueSheet};
use bae_util::content_type_hint::ContentTypeHint;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

mod candidates;
mod categorize;
mod file_decisions;
mod file_reads;
mod files;
mod folder_times;
mod release_decision;
mod scan;
mod sheet_bindings;
mod sheet_references;
mod sidecars;
pub use candidates::*;
pub use categorize::is_audio_file;
pub use categorize::scanned_audio;
use categorize::*;
pub(crate) use categorize::{
    categorize_library_files, has_extension, is_cue_file, is_hidden_entry,
};
pub use file_decisions::*;
pub(crate) use file_reads::FileReads;
use file_reads::Playability;
pub use files::*;
pub use folder_times::FolderDate;
pub(crate) use folder_times::FolderTimes;
pub use release_decision::*;
pub(crate) use scan::file_modified_at_ns;
pub use scan::*;
use sheet_bindings::*;
pub use sheet_bindings::{
    BoundTrackSheet, DiscLayout, SheetAudioFile, SheetBinding, SheetBindingOffer,
    SheetBindingOption, SheetReferenceOptions, TrackSheetFile, UnreadableAudio,
};
use sheet_references::resolve_cue_audio_path;
use sidecars::categorize_sidecar;

/// A pre-collected list of files for release candidate detection.
///
/// Built by walking the filesystem once.
pub(crate) struct CandidateFileIndex {
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
    /// One of the threads a pass reads folders on at once panicked; the
    /// others stopped with it. A bug, which the pass's caller reports.
    #[error("{0}")]
    Panicked(bae_util::task_failure::TaskFailed),
}

impl FolderScanError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io { path: path.into(), source }
    }
}

#[derive(Debug, Default)]
struct DirEntry {
    files: Vec<usize>,
    subdirs: BTreeSet<PathBuf>,
}

impl CandidateFileIndex {
    pub(crate) fn new(files: Vec<FileEntry>) -> Self {
        let mut tree = Self { files, dirs: BTreeMap::new() };
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
            self.dirs.entry(parent.clone()).or_default().subdirs.insert(child.clone());
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
#[path = "folder_scanner_tests.rs"]
mod tests;
