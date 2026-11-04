//! Whether the folders under a folder are one release or several: the
//! decision, who made it, the readings stored for a root's folders, and the
//! scan's own reading from the folders' names when nothing is stored.

use super::*;
use crate::CandidateKey;
use bae_db::RelativeFolder;

/// The stable address of one folder-boundary decision.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FolderReleaseDecisionKey {
    pub watched_folder_path: PathBuf,
    /// The folder below the watched root; [`RelativeFolder::root`] names the
    /// root.
    pub relative_folder: RelativeFolder,
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

/// How one folder reads, as stored: whether its releases are one, who said
/// so, and the key of the grouping that says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderReading {
    pub decision: FolderReleaseDecision,
    pub author: FolderReleaseDecisionAuthor,
    pub grouping: CandidateKey,
}

/// How each folder under one watched root reads, loaded before its scan
/// begins, by the folder below the root.
#[derive(Debug, Clone, Default)]
pub struct FolderReleaseDecisions(HashMap<RelativeFolder, FolderReading>);

impl FolderReleaseDecisions {
    pub fn new(readings: HashMap<RelativeFolder, FolderReading>) -> Self {
        Self(readings)
    }

    /// How the folder at `relative_folder` reads, if anything is stored.
    pub(crate) fn get(&self, relative_folder: &RelativeFolder) -> Option<&FolderReading> {
        self.0.get(relative_folder)
    }

    /// Every folder's reading, by the folder below the root.
    #[cfg(test)]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&RelativeFolder, &FolderReading)> {
        self.0.iter()
    }

    /// Read `relative_folder` as `reading`, whatever was stored for it — the
    /// reading a decision about to be stored gives.
    pub(crate) fn insert(&mut self, relative_folder: RelativeFolder, reading: FolderReading) {
        self.0.insert(relative_folder, reading);
    }
}

/// How much audio a child folder holds, counted as far as a reading needs:
/// the audio of one folder, or of several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartAudio {
    /// One folder holds audio: the child itself, or one folder below it
    /// reached through folders with none. A disc holds this much.
    OneFolder,
    /// Two folders or more hold audio, so the child is several releases of
    /// its own, not one disc of a set.
    SeveralFolders,
}

/// A child folder that holds audio: one of the releases its parent is read
/// as, or one disc of the one release its parent is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderPart {
    pub name: String,
    pub audio: PartAudio,
}

/// How the scan reads a folder whose children hold the audio, with nothing
/// stored for it.
///
/// A multi-disc release names its parts: `Disc 1`, `CD2`, `Vol. 3`. So when
/// every part's name carries a number, those numbers are exactly `1..=N` for
/// `N` parts, and each part holds one folder's audio, the parts are one
/// release. Anything else — a part with no number, a gap, a run that doesn't
/// start at one, a part that holds several folders of audio, audio sitting
/// directly in the folder beside its parts — is a folder that happens to hold
/// several releases. `01 Originals` and `02 Remasters`, holding three albums
/// and one, number themselves 1 and 2 and are still four releases.
///
/// The parts are the child folders that hold audio, and only those. A `covers`
/// or `Scans` folder alongside `CD1` and `CD2` is a sidecar the release
/// carries, and a numbered `CD3` with no audio in it is no disc; the caller
/// leaves both out.
pub fn heuristic_folder_release_decision(
    holds_audio_directly: bool,
    parts: &[FolderPart],
) -> FolderReleaseDecision {
    if holds_audio_directly || parts.len() < 2 {
        return FolderReleaseDecision::KeepAsSeparateReleases;
    }
    let mut numbers: Vec<u32> = Vec::with_capacity(parts.len());
    for part in parts {
        if part.audio == PartAudio::SeveralFolders {
            return FolderReleaseDecision::KeepAsSeparateReleases;
        }
        match folder_part_number(&part.name) {
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

/// Whether children named `names` could be one release's discs, before
/// looking inside any of them: the folder holds no tracks of its own, and
/// children numbered 1 and 2 are among them. Discs number themselves from
/// one, so when this is false no child's contents can make
/// [`heuristic_folder_release_decision`] combine them.
pub fn names_could_number_one_release(holds_audio_directly: bool, names: &[String]) -> bool {
    if holds_audio_directly {
        return false;
    }
    let numbers: std::collections::HashSet<u32> =
        names.iter().filter_map(|name| folder_part_number(name)).collect();
    numbers.contains(&1) && numbers.contains(&2)
}

/// The part number in a folder's name. An explicit `CD n`, `Disc n`, `Disk n`,
/// `Vol n`, or `Volume n` marker is authoritative wherever it appears, so
/// years and other numbers elsewhere in the name do not compete with it.
/// Without a marker, exactly one numeric run is required.
pub(super) fn folder_part_number(name: &str) -> Option<u32> {
    let labeled_numbers = labeled_part_numbers(name)?;
    if let Some((&number, remaining)) = labeled_numbers.split_first() {
        return remaining.iter().all(|candidate| *candidate == number).then_some(number);
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

#[cfg(test)]
#[path = "release_decision_tests.rs"]
mod tests;
