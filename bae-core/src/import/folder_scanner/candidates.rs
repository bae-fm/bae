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

/// The part number in a folder's name. An explicit `CD n`, `Disc n`, `Disk n`,
/// `Vol n`, or `Volume n` marker is authoritative wherever it appears, so
/// years and other numbers elsewhere in the name do not compete with it.
/// Without a marker, exactly one numeric run is required.
pub(super) fn folder_part_number(name: &str) -> Option<u32> {
    let labeled_numbers = labeled_part_numbers(name)?;
    if let Some((&number, remaining)) = labeled_numbers.split_first() {
        return remaining
            .iter()
            .all(|candidate| *candidate == number)
            .then_some(number);
    }

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

fn labeled_part_numbers(name: &str) -> Option<Vec<u32>> {
    const LABELS: &[&[u8]] = &[b"volume", b"disc", b"disk", b"vol", b"cd"];

    let bytes = name.as_bytes();
    let mut numbers = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let Some(label) = LABELS.iter().find(|label| {
            bytes
                .get(index..index + label.len())
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(label))
        }) else {
            index += 1;
            continue;
        };
        let starts_outside_word = match name[..index].chars().next_back() {
            Some(character) => !character.is_alphabetic(),
            None => true,
        };
        if !starts_outside_word {
            index += label.len();
            continue;
        }

        let mut digit_start = index + label.len();
        while digit_start < bytes.len()
            && matches!(bytes[digit_start], b' ' | b'\t' | b'.' | b'-' | b'_' | b'#')
        {
            digit_start += 1;
        }
        let mut digit_end = digit_start;
        while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
            digit_end += 1;
        }
        if digit_end > digit_start {
            let number = name[digit_start..digit_end].parse::<u32>().ok()?;
            numbers.push(number);
            index = digit_end;
        } else {
            index += label.len();
        }
    }
    Some(numbers)
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

/// How a folder was settled, retained on every row below it so the control
/// that reads it the other way has the key to rewrite without rebuilding a
/// path.
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
            &["CD1 Volume 2", "CD2 Volume 3"][..],
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

    /// A labeled disc number is authoritative even when other numbers surround
    /// it.
    #[test]
    fn a_labeled_disc_number_wins_over_other_numbers() {
        for names in [
            &["1994 CD1 archive 2001", "1995 CD2 archive 2002"][..],
            &["1994CD1 archive 2001", "1995CD2 archive 2002"][..],
            &["1994 Disc 1 archive 2001", "1995 Disc 2 archive 2002"][..],
            &["1994 Disk 1 archive 2001", "1995 Disk 2 archive 2002"][..],
            &["1994 Vol. 1 archive 2001", "1995 Vol. 2 archive 2002"][..],
            &["1994 Volume 1 archive 2001", "1995 Volume 2 archive 2002"][..],
            &[
                "1994 Volume 1 (CD 1) archive 2001",
                "1995 Volume 2 (CD 2) archive 2002",
            ][..],
        ] {
            assert_eq!(
                decide(names),
                FolderReleaseDecision::CombineAsOneRelease,
                "{names:?}"
            );
        }
    }

    /// Without a label, several numbers leave the part number ambiguous.
    #[test]
    fn unlabeled_multiple_numbers_name_no_part() {
        for names in [
            &["1994 archive 1", "1995 archive 2"][..],
            &["SACD 1994 layer 1", "SACD 1995 layer 2"][..],
            &["éCD1 archive 1994", "éCD2 archive 1995"][..],
        ] {
            assert_eq!(
                decide(names),
                FolderReleaseDecision::KeepAsSeparateReleases,
                "{names:?}"
            );
        }
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
