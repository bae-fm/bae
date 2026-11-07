//! What a change on disk under a watched root asks to have read again: the
//! changes a watch's events report, the roots they reach, and the folders
//! directly under a root they reach, or the whole root.

use crate::volume::changed_directories;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

/// The paths in one batch of events that report a change to the watched tree.
///
/// Linux's inotify backend reports every `open()`, including the scan's own
/// reads, so counting those as changes would make every scan schedule the next.
pub(super) fn changed_paths(events: &[notify::Event]) -> Vec<&Path> {
    events
        .iter()
        .filter(|event| reports_a_change(&event.kind))
        .flat_map(|event| event.paths.iter().map(PathBuf::as_path))
        .collect()
}

/// How many events in a batch count as changes, naming the first few; copying
/// an album is hundreds of events, and the first few name the cause.
pub(super) fn changed_events_summary(events: &[notify::Event]) -> String {
    const NAMED: usize = 6;
    let changes: Vec<&notify::Event> =
        events.iter().filter(|event| reports_a_change(&event.kind)).collect();
    let named: Vec<String> = changes
        .iter()
        .take(NAMED)
        .map(|event| {
            format!(
                "{:?} {}",
                event.kind,
                event
                    .paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect();
    let ignored = events.len() - changes.len();
    let more = changes.len().saturating_sub(named.len());
    format!(
        "{} of {} events count as changes ({ignored} ignored){}{}",
        changes.len(),
        events.len(),
        if named.is_empty() { String::new() } else { format!(": {}", named.join("; ")) },
        if more > 0 { format!(" and {more} more") } else { String::new() }
    )
}

fn reports_a_change(kind: &notify::EventKind) -> bool {
    use notify::event::{AccessKind, AccessMode};
    match kind {
        // The close that ended a write is how a finished copy announces itself.
        notify::EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        notify::EventKind::Access(_) => false,
        _ => true,
    }
}

/// What a set of changed paths under one watched root asks to be read again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RootChange {
    /// The root itself: something only a pass over all of it reads right.
    WholeRoot,
    /// These folders directly under the root, and nothing else. Empty when no
    /// change reaches anything a scan reads.
    Folders(std::collections::BTreeSet<std::ffi::OsString>),
}

/// [`root_change`] on a blocking thread: its stats can take as long as a
/// network mount does to answer, and the coordinator must not wait on that.
/// Stats that panic are reported to `diagnostics` and read the whole root.
pub(super) async fn root_change_of(
    root: &Path,
    changed: &[&Path],
    holds_its_own_release: bool,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> RootChange {
    let root = root.to_path_buf();
    let changed: Vec<PathBuf> = changed.iter().map(|path| path.to_path_buf()).collect();
    match diagnostics
        .blocking("read what changed under a watched folder", move || {
            let changed: Vec<&Path> = changed.iter().map(PathBuf::as_path).collect();
            root_change(&root, &changed, holds_its_own_release)
        })
        .await
    {
        Ok(change) => change,
        // Reported as it failed; the whole root is read again.
        Err(failed) => {
            tracing::debug!("{failed}; reading the whole root again");
            RootChange::WholeRoot
        }
    }
}

/// What `changed` (paths under `root`) asks to be read again.
///
/// A root is read one top-level folder at a time, so a change inside one reads
/// that folder again, and an entry that went away is read as a folder that is
/// now empty. The root is read whole when the root itself changed, an audio
/// file directly in it came or went, or it holds tracks of its own (whose
/// release takes in the audio-free folders beside them). Hidden entries are
/// skipped because the scan never lists them.
pub(super) fn root_change(
    root: &Path,
    changed: &[&Path],
    holds_its_own_release: bool,
) -> RootChange {
    let mut folders = std::collections::BTreeSet::new();
    for path in changed {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let names: Vec<&std::ffi::OsStr> =
            relative.components().map(|component| component.as_os_str()).collect();
        if names.iter().any(|name| crate::folder_scanner::is_hidden_entry(name)) {
            continue;
        }
        let Some(first) = names.first() else {
            return RootChange::WholeRoot;
        };
        if holds_its_own_release {
            return RootChange::WholeRoot;
        }
        let entry = root.join(first);
        let folder = || first.to_os_string();
        match std::fs::symlink_metadata(&entry).map(|_| entry.is_dir()) {
            Ok(true) => {
                folders.insert(folder());
            }
            Ok(false) => {
                if crate::folder_scanner::is_audio_file(&entry) {
                    return RootChange::WholeRoot;
                }
            }
            Err(_) => {
                if names.len() == 1 && crate::folder_scanner::is_audio_file(&entry) {
                    return RootChange::WholeRoot;
                }
                folders.insert(folder());
            }
        }
    }
    RootChange::Folders(folders)
}

/// The paths the directory-mtime check of a network root found changed, in the
/// form a filesystem event gives them, or `None` when the root has to be walked.
///
/// When the root's own mtime moved, the folders that came or went are found by
/// listing it against the record, and a root with audio directly in it names
/// itself, since the record cannot tell which of its files moved.
pub(super) fn network_changes(root: &Path, recorded: &[(PathBuf, i64)]) -> Option<Vec<PathBuf>> {
    let moved = changed_directories(recorded)?;
    let mut changes = Vec::new();
    for directory in moved {
        if directory != root {
            changes.push(directory);
            continue;
        }
        let listing = match std::fs::read_dir(root) {
            Ok(listing) => listing,
            Err(error) => {
                warn!(
                    "can't list {} against what the last scan saw, so it is walked: {error}",
                    root.display()
                );
                return None;
            }
        };
        let mut present = HashSet::new();
        for entry in listing {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) => {
                    warn!(
                        "can't read an entry of {} against what the last scan saw, so it is walked: {error}",
                        root.display()
                    );
                    return None;
                }
            };
            if path.is_dir() {
                present.insert(path);
            } else if crate::folder_scanner::is_audio_file(&path) {
                changes.push(root.to_path_buf());
            }
        }
        let known: HashSet<PathBuf> = recorded
            .iter()
            .map(|(path, _)| path.clone())
            .filter(|path| path.parent() == Some(root))
            .collect();
        changes.extend(present.symmetric_difference(&known).cloned());
    }
    Some(changes)
}

/// The watched roots that contain at least one of the `changed` paths, in
/// `roots` order and without duplicates.
pub(super) fn affected_roots(changed: &[&Path], roots: &[PathBuf]) -> Vec<PathBuf> {
    roots.iter().filter(|root| changed.iter().any(|path| path.starts_with(root))).cloned().collect()
}

pub(super) fn roots_for_watch_error(error_paths: &[PathBuf], roots: &[PathBuf]) -> Vec<PathBuf> {
    let paths: Vec<&Path> = error_paths.iter().map(PathBuf::as_path).collect();
    let affected = affected_roots(&paths, roots);
    if affected.is_empty() {
        roots.to_vec()
    } else {
        affected
    }
}

#[cfg(test)]
#[path = "root_change_tests.rs"]
mod tests;
