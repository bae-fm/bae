//! Releases read from several folders, and the actions that make and undo them.
//!
//! A release is a set of folders. The scan reads one folder as one release
//! until a grouping says otherwise: a grouping anchored at a folder reads
//! every release below it as one (the scan proposes these from the folder's
//! structure, and the person can flip them), and a grouping the person makes
//! from releases anywhere reads exactly those as one. Either way the release
//! gives each folder it is made of a run of discs of its own, in order.
//!
//! This module builds the second kind from the releases it takes in; the scan
//! builds the first kind itself, from the disk (see
//! [`super::folder_scanner`]).

use super::folder_scanner::{
    CandidateFile, CandidateFileEdits, CategorizedFiles, FileRole, FolderCandidate,
    ReleaseFileScope, ReleasePart, SheetBinding,
};
use super::ImportError;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Which of the two grouping actions a release offers, if either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupingAction {
    /// Read this release together with others as one.
    Combine,
    /// Read the folders this release is made of as releases of their own.
    Separate,
}

/// The release a grouping with no anchor reads `members` as, in the order
/// given — each with the file decisions stored for it — and the decisions the
/// release takes over from them: its files and parts, before any decision
/// stored for the release itself lands.
///
/// A member's decisions carry across as the release's own starting point: a
/// file taken out of a folder's tracklist stays out of the release's, a sheet
/// bound by hand stays bound, and a sheet's disc keeps its place within the
/// run of discs the release gives that folder. Decisions the release stores
/// later go over them.
///
/// Each file keeps its path on disk; its path in the release is its path in
/// its member, under a prefix naming that member. Within one watched folder
/// the prefix is the member's folder relative to the folder all members share,
/// so two disc folders under one album read `Disc 1/01.flac`, `Disc 2/01.flac`
/// exactly as the album would. Members under different watched folders share
/// no folder, and each takes a prefix of its position and name.
pub fn compose(
    key: &str,
    watched_folder_path: &str,
    members: &[(FolderCandidate, CandidateFileEdits)],
) -> Result<(FolderCandidate, CandidateFileEdits), ImportError> {
    let edits: Vec<&CandidateFileEdits> = members.iter().map(|(_, edits)| edits).collect();
    let members: Vec<FolderCandidate> = members.iter().map(|(member, _)| member.clone()).collect();
    let members = members.as_slice();
    let first = members.first().ok_or_else(|| ImportError::Internal {
        detail: "a release read from several folders needs at least one of them".into(),
    })?;
    let mut seen_keys = HashSet::new();
    for member in members {
        if !seen_keys.insert(member.key()) {
            return Err(ImportError::Internal {
                detail: format!("{} was selected more than once", member.key()),
            });
        }
    }
    let one_root = members
        .iter()
        .all(|member| member.watched_folder_path == first.watched_folder_path);
    let shared = one_root.then(|| shared_folder(members));
    let mut seen_files = HashSet::new();
    let mut files = Vec::new();
    let mut parts = Vec::new();
    let mut inherited = CandidateFileEdits::default();
    let mut next_disc = 1u32;
    for (position, member) in members.iter().enumerate() {
        let prefix = match &shared {
            Some(shared) => {
                let within = member
                    .file_root
                    .strip_prefix(shared)
                    .expect("the shared folder holds every member");
                let within = super::folder_scanner::relative_path_string(within);
                if within.is_empty() {
                    within
                } else {
                    format!("{within}/")
                }
            }
            None => {
                let prefix = format!("{:02} - {}/", position + 1, member.name);
                super::watched_folder::validate_relative_path(prefix.trim_end_matches('/'))?;
                prefix
            }
        };
        if member.files.audio().next().is_none() {
            return Err(ImportError::Internal {
                detail: format!("{} has no playable tracks to combine", member.key()),
            });
        }
        for entry in &member.files.files {
            if !seen_files.insert(entry.file.path.clone()) {
                return Err(ImportError::Internal {
                    detail: format!(
                        "{} belongs to more than one selected folder",
                        entry.file.path.display()
                    ),
                });
            }
            files.push(prefixed(entry, &prefix));
        }
        // The member's own discs, in its own order, become a run of discs
        // starting where the run before it ended.
        let discs: std::collections::BTreeSet<Option<i32>> =
            super::track_slots::direct_entry_track_rows(&member.files)
                .iter()
                .map(|track| track.side)
                .collect();
        let run_start = next_disc;
        let in_run = |disc: u32| {
            let position = discs
                .iter()
                .position(|side| *side == i32::try_from(disc).ok())
                .unwrap_or(0);
            run_start + u32::try_from(position).unwrap_or(0)
        };
        inherited.overlay(&edits[position].under_prefix(&prefix, in_run));
        next_disc += u32::try_from(discs.len().max(1)).map_err(|_| ImportError::Internal {
            detail: "combined disc number exceeds the supported range".into(),
        })?;
        if member.files.parts.is_empty() {
            parts.push(ReleasePart {
                folder: member.path.clone(),
                prefix: prefix.clone(),
            });
        } else {
            parts.extend(member.files.parts.iter().map(|part| ReleasePart {
                folder: part.folder.clone(),
                prefix: format!("{prefix}{}", part.prefix),
            }));
        }
    }
    // Member order is the release's order: each member's files keep their own
    // order, one member after another, so the tracks play folder by folder.
    let folder = shared.unwrap_or_else(|| first.path.clone());
    inherited.revision = 0;
    let release = FolderCandidate {
        path: folder.clone(),
        file_root: folder,
        name: first.name.clone(),
        files: CategorizedFiles { files, parts },
        watched_folder_path: watched_folder_path.to_string(),
        scope: ReleaseFileScope::Recursive,
        file_edit_revision: 0,
        display_path: first.display_path.clone(),
        grouping: Some(key.to_string()),
    };
    Ok((release, inherited))
}

/// The deepest folder every member's files are under.
fn shared_folder(members: &[FolderCandidate]) -> PathBuf {
    let mut shared: PathBuf = members[0].file_root.clone();
    for member in &members[1..] {
        while !member.file_root.starts_with(&shared) {
            if !shared.pop() {
                break;
            }
        }
    }
    let root = Path::new(&members[0].watched_folder_path);
    if !shared.starts_with(root) {
        return root.to_path_buf();
    }
    shared
}

/// One member's file as the release holds it: under `prefix`, with every file
/// a sheet names renamed to match.
fn prefixed(entry: &CandidateFile, prefix: &str) -> CandidateFile {
    let mut entry = entry.clone();
    entry.file.relative_path = format!("{prefix}{}", entry.file.relative_path);
    entry.file.dir_prefix = match entry.file.dir_prefix.take() {
        Some(directory) => Some(format!("{prefix}{directory}")),
        None if prefix.is_empty() => None,
        None => Some(prefix.to_string()),
    };
    if let FileRole::TrackSheet { binding, .. } = &mut entry.role {
        match binding {
            SheetBinding::Resolved { files } | SheetBinding::Unresolved { files } => {
                for audio in files {
                    audio.file_id = format!("{prefix}{}", audio.file_id);
                }
            }
            SheetBinding::RefusedCodec { .. } => {}
        }
    }
    entry
}

#[cfg(test)]
#[path = "grouping_tests.rs"]
mod tests;
