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
///
/// Comparable, because that is how a pass tells what it changed from what it
/// merely found again: a walk of an untouched folder produces items equal to
/// the ones already stored, and equal items are written and announced to
/// nobody.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ScanItem {
    /// A release approximation completed before its enclosing folder's reading
    /// was known. It is visible scan progress, but identification must wait for
    /// a later [`Self::Valid`] update.
    Discovered(FolderCandidate),
    Valid(FolderCandidate),
    Invalid(InvalidCandidate),
    /// A folder whose parts the scan could not read: it holds several
    /// releases' worth of audio in a shape the naming does not settle, so the
    /// user says how to read it. Reached only where a folder's own children
    /// did not name its parts.
    Boundary(FolderReleaseBoundary),
    /// The scan read a folder its own way, because nothing was stored for it.
    /// The caller stores it, so the flip control on each resulting candidate
    /// has a decision to rewrite.
    Decided {
        key: FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
    },
}

/// Only the durable folder-scan tables key items this way, and those are
/// desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ScanItem {
    /// The durable scan entry this item is, or `None` for an item that is not
    /// one — the folder reading the scan decided, which is stored as a decision
    /// rather than as a scan entry.
    pub(crate) fn persisted_key(&self) -> Option<String> {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => {
                Some(candidate.path.to_string_lossy().into_owned())
            }
            Self::Invalid(candidate) => Some(candidate.path.to_string_lossy().into_owned()),
            Self::Boundary(boundary) => Some(
                Path::new(&boundary.key.watched_folder_path)
                    .join(&boundary.key.relative_folder_path)
                    .to_string_lossy()
                    .into_owned(),
            ),
            Self::Decided { .. } => None,
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

/// How a folder holding several releases' worth of audio is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FolderReleaseDecision {
    CombineAsOneRelease,
    KeepAsSeparateReleases,
}

/// Who decided. The scan decides every such folder for itself so the queue has
/// candidates to work on rather than a card to answer, and stores that as the
/// folder's decision; the user's own answer replaces it and is never decided
/// over again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FolderReleaseDecisionAuthor {
    User,
    Heuristic,
}

/// How the scan reads a folder whose children hold the audio, with nothing
/// stored for it.
///
/// A multi-disc release names its parts: `Disc 1`, `CD2`, `Vol. 3`. So when
/// every part's name carries a number and those numbers are exactly `1..=N`
/// for `N` parts, the parts are one release. Anything else — a part with no
/// number, a gap, a run that doesn't start at one, audio sitting directly in
/// the folder beside its parts — is a folder that happens to hold several
/// releases.
///
/// The parts are the child folders that hold audio, and only those. A `covers`
/// or `Scans` folder alongside `CD1` and `CD2` is a sidecar the release
/// carries, not a third part that failed to number itself, and the caller
/// leaves it out (`audio_bearing_child_names`).
pub fn heuristic_folder_release_decision(
    holds_audio_directly: bool,
    part_folder_names: &[String],
) -> FolderReleaseDecision {
    if holds_audio_directly || part_folder_names.len() < 2 {
        return FolderReleaseDecision::KeepAsSeparateReleases;
    }
    let mut numbers: Vec<u32> = Vec::with_capacity(part_folder_names.len());
    for name in part_folder_names {
        match folder_part_number(name) {
            Some(number) => numbers.push(number),
            None => return FolderReleaseDecision::KeepAsSeparateReleases,
        }
    }
    numbers.sort_unstable();
    let expected: Vec<u32> = (1..=numbers.len() as u32).collect();
    if numbers == expected {
        FolderReleaseDecision::CombineAsOneRelease
    } else {
        FolderReleaseDecision::KeepAsSeparateReleases
    }
}

/// The one number in a folder's name, or `None` when it holds none or several.
/// Several is as unusable as none: `1994 CD2` names a year and a disc, and
/// nothing in the name says which is which.
pub(super) fn folder_part_number(name: &str) -> Option<u32> {
    let mut found: Option<u32> = None;
    let mut digits = String::new();
    for character in name.chars().chain(std::iter::once(' ')) {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if digits.is_empty() {
            continue;
        }
        let number = digits.parse::<u32>().ok()?;
        digits.clear();
        if found.replace(number).is_some() {
            return None;
        }
    }
    found
}

/// Which persisted scan entries a set of folder readings supersedes. Reads the
/// durable scan entries, so it exists only where scans persist — desktop.
///
/// Combining replaces everything below the folder. Keeping separate replaces
/// whatever stood for the whole folder at its own key — the card asking how to
/// read it, or the candidate that read it as one release. A folder that holds
/// tracks of its own also has a candidate under that key, and that one stays:
/// it is one of the separate releases, not something the reading removes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn release_decision_removed_keys(
    persisted: &[crate::import::candidates::StoredEntryKey],
    decisions: &[(FolderReleaseDecisionKey, FolderReleaseDecision)],
) -> Vec<String> {
    let mut removed = Vec::new();
    for (key, decision) in decisions {
        let boundary_path = Path::new(&key.watched_folder_path).join(&key.relative_folder_path);
        let boundary_key = boundary_path.to_string_lossy();
        removed.extend(persisted.iter().filter_map(|entry| {
            let path = Path::new(&entry.key);
            let superseded = match decision {
                FolderReleaseDecision::CombineAsOneRelease => path.starts_with(&boundary_path),
                FolderReleaseDecision::KeepAsSeparateReleases => {
                    entry.covers_whole_folder && entry.key == boundary_key.as_ref()
                }
            };
            superseded.then(|| entry.key.clone())
        }));
    }
    removed.sort();
    removed.dedup();
    removed
}

/// Decisions loaded for one watched root before its scan begins.
#[derive(Debug, Clone, Default)]
pub struct FolderReleaseDecisions(
    HashMap<String, (FolderReleaseDecision, FolderReleaseDecisionAuthor)>,
);

impl FolderReleaseDecisions {
    pub fn new(
        decisions: HashMap<String, (FolderReleaseDecision, FolderReleaseDecisionAuthor)>,
    ) -> Self {
        Self(decisions)
    }

    /// The stored decision for a folder, and who made it.
    pub(crate) fn get(
        &self,
        relative_folder_path: &str,
    ) -> Option<(FolderReleaseDecision, FolderReleaseDecisionAuthor)> {
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

#[cfg(test)]
mod heuristic_tests {
    use super::*;

    fn decide(names: &[&str]) -> FolderReleaseDecision {
        heuristic_folder_release_decision(
            false,
            &names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
        )
    }

    /// The children number themselves 1..=N, so they are the parts of one
    /// release.
    #[test]
    fn a_continuous_run_from_one_is_one_release() {
        for names in [
            &["1", "2"][..],
            &["Disc 1", "Disc 2", "Disc 3", "Disc 4"][..],
            &["CD2", "CD1"][..],
            &["Vol. 3", "Vol. 1", "Vol. 2"][..],
        ] {
            assert_eq!(
                decide(names),
                FolderReleaseDecision::CombineAsOneRelease,
                "{names:?}"
            );
        }
    }

    /// A gap, a run that does not start at one, or a child with no number at
    /// all: the folder holds several releases, not one release's parts.
    #[test]
    fn anything_else_is_several_releases() {
        for names in [
            &["1", "3"][..],
            &["2", "3"][..],
            &["CD1", "CD2", "Bonus"][..],
            &["Live in Tokyo", "Live in Osaka"][..],
            &["1", "1"][..],
        ] {
            assert_eq!(
                decide(names),
                FolderReleaseDecision::KeepAsSeparateReleases,
                "{names:?}"
            );
        }
    }

    /// A name with two numbers says nothing about which is the part number.
    #[test]
    fn a_name_with_two_numbers_names_no_part() {
        assert_eq!(
            decide(&["1994 CD1", "1994 CD2"]),
            FolderReleaseDecision::KeepAsSeparateReleases
        );
    }

    /// Tracks sitting in the folder beside its child folders are their own
    /// release, so the folder is not one release's parts.
    #[test]
    fn a_folder_with_its_own_tracks_holds_several_releases() {
        assert_eq!(
            heuristic_folder_release_decision(true, &["Disc 1".to_string(), "Disc 2".to_string()]),
            FolderReleaseDecision::KeepAsSeparateReleases
        );
    }

    /// One child is not a set.
    #[test]
    fn a_single_child_is_not_a_set() {
        assert_eq!(
            decide(&["Disc 1"]),
            FolderReleaseDecision::KeepAsSeparateReleases
        );
    }
}
