//! What a folder someone chose to import is to the library, once its watched
//! root has been read.
//!
//! Choosing a folder — from a picker, a drop, Finder, or the empty library —
//! is one request: "take this in". What answers it depends on what the folder
//! already is. Its releases may all be in the library, some may still wait in
//! the queue, or the read may have found none. The same placement the list
//! uses decides which, so where the person is taken always agrees with where
//! the list shows the folder's releases.

use super::flatten::{natural_path, place_row};
use crate::db::{ImportQueueRows, ScanCandidateKind, ScanCandidateListRow};
use crate::import::triage::{TriageImportStatus, TriageTab};
use crate::library::LibraryError;
use std::path::Path;

/// Where a chosen folder's releases stand, which is where the person is taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChosenFolder {
    /// Every release the folder holds is already in the library. `album_id` is
    /// the album of the first of them in path order — the one to show.
    InLibrary { album_id: String },
    /// Releases the folder holds are not in the library yet. The keys are the
    /// ones still waiting in the queue — or, when every one of those was set
    /// aside, the skipped ones — in path order.
    InImportQueue { candidate_keys: Vec<String> },
    /// The read found no release in the folder.
    NoReleases,
}

/// One read of a chosen folder: what its releases are, or why its watched
/// root's read failed — in which case what is stored for it is the previous
/// read's, and nothing may be concluded from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChosenFolderRead {
    Read(ChosenFolder),
    ScanFailed(String),
}

/// What `chosen`, under the watched root `root`, is to the library.
///
/// The releases it names are the ones stored at or below it. A folder that
/// holds none of its own is part of a release instead — a disc folder of a
/// release read from its parent, an artwork folder beside the audio — and
/// names the one stored at its nearest enclosing folder.
pub(crate) fn chosen_folder(
    rows: &ImportQueueRows,
    root: &str,
    chosen: &Path,
) -> Result<ChosenFolder, LibraryError> {
    let releases: Vec<&ScanCandidateListRow> = rows
        .candidates
        .iter()
        .filter(|row| row.watched_folder_path == root && row.kind == ScanCandidateKind::Valid)
        .collect();
    let mut named: Vec<&ScanCandidateListRow> = releases
        .iter()
        .copied()
        .filter(|row| Path::new(&row.folder).starts_with(chosen))
        .collect();
    if named.is_empty() {
        named.extend(
            releases
                .iter()
                .copied()
                .filter(|row| chosen.starts_with(&row.folder))
                .max_by_key(|row| Path::new(&row.folder).components().count()),
        );
    }
    named.sort_by(|left, right| natural_path(&left.display_path, &right.display_path));

    let mut waiting = Vec::new();
    let mut skipped = Vec::new();
    let mut first_album = None;
    for row in named {
        let placed = place_row(rows, row)?;
        match placed.placement.tab() {
            TriageTab::Pending => waiting.push(placed.candidate_key),
            TriageTab::Skipped => skipped.push(placed.candidate_key),
            TriageTab::Done => {
                let Some(TriageImportStatus::Complete { release }) = placed.import_status else {
                    return Err(LibraryError::Internal(format!(
                        "release {} is placed Done without the library release it became",
                        placed.candidate_key
                    )));
                };
                first_album.get_or_insert(release.album_id);
            }
        }
    }
    Ok(if !waiting.is_empty() {
        ChosenFolder::InImportQueue {
            candidate_keys: waiting,
        }
    } else if !skipped.is_empty() {
        ChosenFolder::InImportQueue {
            candidate_keys: skipped,
        }
    } else if let Some(album_id) = first_album {
        ChosenFolder::InLibrary { album_id }
    } else {
        ChosenFolder::NoReleases
    })
}
