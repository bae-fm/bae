use super::*;

/// Why a candidate folder failed validation. The `Display` text is the terse
/// internal form (used by the import-commit error channel); the UI localizes the
/// typed variant for the Skipped tab.
///
/// Only real defects invalidate. A sheet that disagrees with what is on disk —
/// naming absent audio, failing to parse, or naming a codec bae can't play from
/// a single-file CUE — leaves the sheet unbound and the folder importable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum InvalidReason {
    #[error("corrupt or zero-byte audio file: {path}")]
    CorruptAudioFile { path: String },
    #[error("corrupt or zero-byte image: {path}")]
    CorruptImage { path: String },
    #[error("no valid audio files")]
    NoValidAudio,
}

/// A leaf folder that looks like a release (it has audio) but failed validation.
/// It can't be imported, so it carries no files and no identify state — only
/// enough to surface it under the Skipped tab with its reason.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InvalidCandidate {
    /// Root path of the folder that failed validation.
    pub path: PathBuf,
    /// Display name (derived from folder name).
    pub name: String,
    /// Absolute path of the watched folder this was scanned from — the
    /// candidate-list group it belongs to. Equal to the scan root.
    pub watched_folder_path: String,
    pub display_path: String,
    /// Explicit release-structure decisions that exposed this invalid row.
    pub resolved_boundaries: Vec<ResolvedFolderReleaseBoundary>,
    /// Why the folder failed validation — the UI localizes this typed reason.
    pub reason: InvalidReason,
}

/// One item the scan callback yields per leaf folder: a valid release
/// candidate, or an invalid one (looked like a release but failed validation).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ScanItem {
    /// A release approximation completed before its enclosing folder boundary
    /// was known. It is visible scan progress, but identification must wait for
    /// a later [`Self::Valid`] or [`Self::Boundary`] update.
    Discovered(FolderCandidate),
    Valid(FolderCandidate),
    Invalid(InvalidCandidate),
    Boundary(FolderReleaseBoundary),
}

/// Only the durable folder-scan tables key items this way, and those are
/// desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ScanItem {
    pub(crate) fn persisted_key(&self) -> String {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => {
                candidate.path.to_string_lossy().into_owned()
            }
            Self::Invalid(candidate) => candidate.path.to_string_lossy().into_owned(),
            Self::Boundary(boundary) => Path::new(&boundary.key.watched_folder_path)
                .join(&boundary.key.relative_folder_path)
                .to_string_lossy()
                .into_owned(),
        }
    }
}

/// Which files a folder boundary owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReleaseFileScope {
    /// Files directly in the folder plus descendants below children with no
    /// audio of their own.
    Direct,
    /// Every file below the folder.
    Recursive,
}

/// The stable address of one folder-boundary decision.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct FolderReleaseDecisionKey {
    pub watched_folder_path: String,
    /// `/`-separated path below the watched root. Empty names the root.
    pub relative_folder_path: String,
}

/// The user's explicit interpretation of an ambiguous folder boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FolderReleaseDecision {
    CombineAsOneRelease,
    KeepAsSeparateReleases,
}

/// Which persisted scan entries a set of boundary decisions supersedes. Reads
/// the durable scan entries, so it exists only where scans persist — desktop.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn release_decision_removed_keys(
    persisted_keys: &HashSet<String>,
    decisions: &[(FolderReleaseDecisionKey, FolderReleaseDecision)],
) -> Vec<String> {
    let mut removed = Vec::new();
    for (key, decision) in decisions {
        let boundary_path = Path::new(&key.watched_folder_path).join(&key.relative_folder_path);
        let boundary_key = boundary_path.to_string_lossy();
        removed.extend(persisted_keys.iter().filter_map(|candidate_key| {
            let path = Path::new(candidate_key);
            let superseded = match decision {
                FolderReleaseDecision::CombineAsOneRelease => path.starts_with(&boundary_path),
                FolderReleaseDecision::KeepAsSeparateReleases => {
                    candidate_key == boundary_key.as_ref()
                }
            };
            superseded.then(|| candidate_key.clone())
        }));
    }
    removed.sort();
    removed.dedup();
    removed
}

/// Decisions loaded for one watched root before its scan begins.
#[derive(Debug, Clone, Default)]
pub struct FolderReleaseDecisions(HashMap<String, FolderReleaseDecision>);

impl FolderReleaseDecisions {
    pub fn new(decisions: HashMap<String, FolderReleaseDecision>) -> Self {
        Self(decisions)
    }

    pub(crate) fn get(&self, relative_folder_path: &str) -> Option<FolderReleaseDecision> {
        self.0.get(relative_folder_path).copied()
    }
}

/// A folder row in an unresolved boundary's compact tree.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FolderReleaseTreeRow {
    pub name: String,
    pub display_path: String,
    pub depth: u32,
    pub kind: FolderReleaseTreeRowKind,
    pub decision_key: FolderReleaseDecisionKey,
    /// Enclosing unresolved boundaries that become separate when this row is
    /// resolved. The UI submits only `decision_key`; core persists these with
    /// it in one transaction.
    pub ancestor_decision_keys: Vec<FolderReleaseDecisionKey>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum FolderReleaseTreeRowKind {
    Folder,
    Candidate {
        summary: FolderReleaseCandidateSummary,
    },
    Invalid {
        reason: InvalidReason,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FolderReleaseCandidateSummary {
    pub track_count: u32,
    pub format_label: String,
}

/// A folder whose structure admits both one recursive release and several
/// direct release approximations.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FolderReleaseBoundary {
    pub key: FolderReleaseDecisionKey,
    pub name: String,
    pub display_path: String,
    pub shared_file_count: u32,
    pub tree_rows: Vec<FolderReleaseTreeRow>,
    /// Tentative candidate paths hidden by this boundary.
    pub candidate_keys: Vec<String>,
}

/// A resolved boundary retained on a row so its context menu can set the
/// opposite interpretation without reconstructing a path.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ResolvedFolderReleaseBoundary {
    pub key: FolderReleaseDecisionKey,
    pub decision: FolderReleaseDecision,
    pub name: String,
    pub display_path: String,
}

/// A folder candidate detected during filesystem scanning.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FolderCandidate {
    /// Root path of this release
    pub path: PathBuf,
    /// Root passed to the import revalidation walk. It differs from `path`
    /// when audio-free single-child wrappers were collapsed into this row.
    pub file_root: PathBuf,
    /// Display name (derived from folder name)
    pub name: String,
    /// Pre-categorized files for this release
    pub files: CategorizedFiles,
    /// Absolute path of the watched folder this candidate was scanned from —
    /// the candidate-list group it belongs to. Equal to the scan root. The
    /// group's display name comes from the watched-folder list, not here.
    pub watched_folder_path: String,
    /// The exact file ownership rule used to build `files`.
    pub scope: ReleaseFileScope,
    /// Revision of the stored file decisions applied to `files`.
    pub file_edit_revision: u64,
    /// Root-relative path for the queue subtitle, with `/` separators.
    pub display_path: String,
    /// Present when an explicit decision exposed this candidate.
    pub resolved_boundaries: Vec<ResolvedFolderReleaseBoundary>,
    /// Nearest default-separate ancestor that contains multiple release rows.
    /// The UI uses this core-issued key for "Combine as One Release".
    pub combine_ancestor_key: Option<FolderReleaseDecisionKey>,
}

impl FolderCandidate {
    pub fn track_count(&self) -> u32 {
        self.files.track_count()
    }
}
