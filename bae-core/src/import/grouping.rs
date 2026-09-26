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

/// Why a grouping's release cannot be worked on as it stands. The release
/// keeps what it was last built from until this is fixed or the grouping is
/// undone. Each reason is typed so every surface says it in the person's own
/// language; the names it carries are for the log, never for display.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GroupingBlock {
    /// A release it takes in was read again and is no longer valid.
    #[error("Source folder changed: {folder}")]
    SourceChanged { folder: String },
    /// A release it takes in is no longer stored.
    #[error("Source folder changed or disappeared: {folder}")]
    SourceGone { folder: String },
    /// Another grouping already reads the files of the folder its releases
    /// sit in.
    #[error("The files in {folder} already go with the combined release {release}")]
    FolderFilesTaken { folder: String, release: String },
    /// Several groupings sit in a folder that has files, none of them reading
    /// them yet, so no one of them can.
    #[error("The files in {folder} would go with more than one combined release")]
    FolderFilesContested { folder: String },
    /// A download into the folder its releases sit in is still running.
    #[error("The files in {folder} are still downloading")]
    FolderFilesDownloading { folder: String },
    /// The releases it takes in make no release — a fault no person caused,
    /// said only as the diagnostic it is.
    #[error("{detail}")]
    Unbuildable { detail: String },
}

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
/// When every member sits directly in one folder (`shared_parent`), the
/// release is that folder's: `parent_files` are the folder's sidecar files —
/// the cover or booklet beside the disc folders — and they keep their paths
/// below it. Otherwise there is no such folder and `parent_files` is empty;
/// files from anywhere else are refused.
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
    parent_files: &[CandidateFile],
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
    if !parent_files.is_empty() {
        let parent = shared_parent(
            members
                .iter()
                .map(|member| (member.watched_folder_path.as_str(), member.file_root.as_path())),
        )
        .filter(|parent| shared.as_ref() == Some(parent))
        .ok_or_else(|| ImportError::Internal {
            detail: format!("{key} does not sit in one folder, so no folder's files are its own"),
        })?;
        for entry in parent_files {
            if !entry.file.path.starts_with(&parent) {
                return Err(ImportError::Internal {
                    detail: format!(
                        "{} is not under {}, the folder {key} sits in",
                        entry.file.path.display(),
                        parent.display()
                    ),
                });
            }
            seen_files.insert(entry.file.path.clone());
            files.push(entry.clone());
        }
    }
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

/// The folder a grouping's releases all sit directly in, whose sidecar files
/// the grouping reads as its own — or `None` when they sit in no one folder.
///
/// This is the one rule for every grouping. Releases picked together follow
/// it, and so does a folder the scan reads as one release: its releases are
/// the ones it would be read as apart, and it reads its own files only when
/// they all sit directly in it. Releases nested at different depths — a disc
/// folder beside an album folder of discs — sit in no one folder, so neither
/// kind reads the files of the folder above them nor of any folder between;
/// those stay each folder's sidecar.
///
/// Each release is given as its watched folder and the folder its files are
/// read from. They sit in one folder when every one of those is directly in
/// the same folder, below the watched folder they share: `Album/Disc 1` and
/// `Album/Disc 2` sit in `Album`, whether or not `Album` holds other releases
/// too. Releases in different folders, at different depths, or under
/// different watched folders sit in none, and neither does a release that is
/// itself the folder the others are in. The watched folder is never one: it is
/// never a release, so its files are never one's own.
pub(crate) fn shared_parent<'a>(
    members: impl IntoIterator<Item = (&'a str, &'a Path)>,
) -> Option<PathBuf> {
    let mut shared: Option<(&str, &Path)> = None;
    for (watched_folder_path, file_root) in members {
        let parent = file_root.parent()?;
        match shared {
            None => shared = Some((watched_folder_path, parent)),
            Some((root, folder)) if root == watched_folder_path && folder == parent => {}
            Some(_) => return None,
        }
    }
    let (root, folder) = shared?;
    let root = Path::new(root);
    (folder != root && folder.starts_with(root)).then(|| folder.to_path_buf())
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
