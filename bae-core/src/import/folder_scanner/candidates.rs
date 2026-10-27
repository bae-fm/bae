use super::*;

/// Why a folder that looks like a release (it holds audio) can't be imported,
/// as its skipped files say. The UI localizes the typed variant for the
/// Skipped tab.
///
/// Only real defects invalidate. A sheet that disagrees with what is on disk —
/// naming absent audio, failing to parse, or naming a codec bae can't play from
/// a single-file CUE — leaves the sheet unbound and the folder importable, and
/// an image that will not decode is carried as a damaged one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidReason {
    /// `count` of its audio files can't be played: never zero.
    #[error("{count} damaged audio files")]
    DamagedAudio { count: u32 },
    #[error("no valid audio files")]
    NoValidAudio,
}

impl InvalidReason {
    /// The reason `files` give: the audio among them bae can't play, or, with
    /// none of that, that they hold no audio at all.
    pub fn of(files: &[SkippedFile]) -> Self {
        let damaged = files
            .iter()
            .filter(|file| matches!(file.role, ProposedRole::DamagedAudio { .. }))
            .count();
        match u32::try_from(damaged).expect("a folder's file count fits u32") {
            0 => Self::NoValidAudio,
            count => Self::DamagedAudio { count },
        }
    }
}

/// A track sheet bound to audio that holds nothing bae can read: the error
/// settling a release's sheet decisions over its files gives.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a track sheet is bound to {path}, which holds no audio bae can read")]
pub struct UnreadableAudio {
    pub path: String,
}

/// Why one audio file can't be played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AudioDamage {
    /// It holds no bytes.
    Empty,
    /// FFmpeg finds no stream in it that bae plays: a download cut off after
    /// its header, or bytes overwritten with zeros.
    Unreadable,
}

/// What the scan reads one file as before a release's roles settle: what its
/// name says it is, and, for audio and images, what its bytes say. The files
/// of a folder that can't be imported stop here, so this is also how the
/// Skipped tab lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProposedRole {
    /// Audio bae plays.
    Audio,
    /// Named as audio, and bae can't play it.
    DamagedAudio {
        damage: AudioDamage,
    },
    /// A `.cue`, before it is parsed.
    Cue,
    Image,
    /// Named as an image, and its bytes do not decode as one.
    DamagedImage,
    Document,
    Other,
}

/// One file of a folder that can't be imported, as the scan read it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SkippedFile {
    /// The file's path within the folder, `/`-separated.
    pub relative_path: String,
    /// The version of the file the scan read.
    pub file: FileVersion,
    pub role: ProposedRole,
}

impl SkippedFile {
    /// `entry` of a release that can't be imported, read as the role its
    /// scan settled on says.
    pub(crate) fn of_release_file(entry: &CandidateFile) -> Self {
        Self {
            relative_path: entry.file.relative_path.clone(),
            file: entry.file.version(),
            role: match entry.role {
                FileRole::Audio => ProposedRole::Audio,
                FileRole::TrackSheet { .. } => ProposedRole::Cue,
                FileRole::Artwork => ProposedRole::Image,
                FileRole::DamagedImage => ProposedRole::DamagedImage,
                FileRole::Document => ProposedRole::Document,
                FileRole::Other => ProposedRole::Other,
            },
        }
    }

    /// The file as the scan found it, with none of its audio facts.
    pub(crate) fn scanned(&self) -> ScannedFile {
        ScannedFile::new(
            self.file.path.clone(),
            self.relative_path.clone(),
            self.file.size,
            self.file.modified_at_ns,
        )
    }
}

#[cfg(test)]
impl SkippedFile {
    /// A zero-byte track at `relative_path` under `folder`.
    pub(crate) fn empty_track(folder: &Path, relative_path: &str) -> Self {
        Self {
            relative_path: relative_path.to_string(),
            file: FileVersion {
                path: folder.join(relative_path),
                size: 0,
                modified_at_ns: 1,
            },
            role: ProposedRole::DamagedAudio {
                damage: AudioDamage::Empty,
            },
        }
    }
}

/// What a skipped folder's tags call the release: the album title the first
/// of its playable files to state one gives, with the line of that file's
/// album artists, or of its track artists when it names no album artist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkippedTags {
    pub album_title: String,
    pub artist: Option<String>,
}

/// A leaf folder that looks like a release (it has audio) but failed
/// validation. It can't be imported, so it carries no identify state — only
/// what the scan read of its files, to surface it under the Skipped tab with
/// its reason and let the person look in.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InvalidCandidate {
    /// Root path of the folder that failed validation — for a grouping, the
    /// folder the grouping reads as one release.
    pub path: PathBuf,
    /// Display name (derived from folder name).
    pub name: String,
    /// Absolute path of the watched folder this was scanned from — the
    /// candidate-list group it belongs to. Equal to the scan root.
    pub watched_folder_path: String,
    pub display_path: String,
    /// The grouping this row reads several folders as, when it is one. The
    /// row's key is then the grouping's, not the folder's.
    pub grouping: Option<String>,
    /// Every file it holds, in name order, as the scan read it; the damaged
    /// ones are why it failed.
    pub files: Vec<SkippedFile>,
    /// What its tags call the release. `None` when none of its playable files
    /// states an album title, and for a grouping, which is built from stored
    /// rows without reading its files' tags.
    pub tags: Option<SkippedTags>,
}

impl InvalidCandidate {
    /// Why it failed validation.
    pub fn reason(&self) -> InvalidReason {
        InvalidReason::of(&self.files)
    }

    /// The image it would lead with if it could be imported: the one the
    /// folder image ranking picks among its images that decode.
    pub fn cover(&self) -> Option<&SkippedFile> {
        let images: Vec<ScannedFile> = self
            .files
            .iter()
            .filter(|file| file.role == ProposedRole::Image)
            .map(SkippedFile::scanned)
            .collect();
        let cover = crate::import::local_artwork::default_local_cover_file(&images)?;
        self.files
            .iter()
            .find(|file| file.relative_path == cover.relative_path)
    }

    /// Where its row draws [`Self::cover`] from.
    pub fn cover_image(&self) -> Option<crate::import::CoverImageSource> {
        self.cover()
            .map(|file| crate::import::CoverImageSource::Local {
                file: file.file.clone(),
            })
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl InvalidCandidate {
    /// The key this row is stored and addressed under.
    pub fn key(&self) -> String {
        self.grouping
            .clone()
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
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
    /// The scan read a folder its own way, because nothing was stored for it:
    /// the grouping it proposes at that folder, under the key the scan gave
    /// it. The caller stores it before any candidate it produced, so the
    /// candidates that name it name a stored grouping.
    Decided {
        key: FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
        grouping: String,
    },
    /// The files under a folder that none of the releases read there owns.
    Sidecar(FolderSidecar),
}

/// The files under a folder that none of the releases the scan read there
/// owns: a cover, a booklet, a folder of scans beside disc folders kept as
/// releases of their own. The folder holds no audio of its own and is not read
/// as one release, so no release below it takes them; a grouping whose
/// releases all sit directly in the folder does (see
/// `crate::import::grouping::shared_parent`).
///
/// Only a folder below the watched root has one: the root is never a release,
/// so nothing is ever read as its own. Which folder's files a grouping reads
/// is one rule for every grouping (see `shared_parent`): a folder read as one
/// release by the scan and releases picked together read the same files.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FolderSidecar {
    pub watched_folder_path: String,
    /// The folder, absolute. Each file's relative path is below it.
    pub folder: PathBuf,
    pub files: SidecarFiles,
}

/// A folder's sidecar files as the scan read them.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SidecarFiles {
    /// Every file, in the release file order, each an artwork, a document, or
    /// another file. A track sheet with no audio beside it describes nothing
    /// here to play, so it is carried as the document it is.
    Valid(Vec<CandidateFile>),
    /// A file among them is broken — a zero-byte track — so a release that
    /// takes them in cannot be imported. Every file, as the scan read it.
    Invalid(Vec<SkippedFile>),
    /// A download into the folder is still running, so what it will hold is
    /// not known yet; a release that takes them in waits for it, as a
    /// release with a download running in it is not offered at all.
    Downloading,
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
            Self::Discovered(candidate) | Self::Valid(candidate) => Some(candidate.key()),
            Self::Invalid(candidate) => Some(candidate.key()),
            Self::Decided { .. } | Self::Sidecar(_) => None,
        }
    }

    /// The folders this entry's files are read from, and whether it reads
    /// everything below its folder: a grouping, or a release lent the files
    /// of a wrapper folder around it. A release that reads only its own
    /// folder covers that folder and nothing below it that holds audio.
    pub(crate) fn coverage(&self) -> Option<Coverage> {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => Some(Coverage {
                folder: candidate.file_root.clone(),
                whole_subtree: candidate.scope == ReleaseFileScope::Recursive,
            }),
            Self::Invalid(candidate) => Some(Coverage {
                folder: candidate.path.clone(),
                whole_subtree: candidate.grouping.is_some(),
            }),
            // The folder whose files it holds: what a reading of one folder
            // checks it stays inside. Which release holds a file is settled
            // file by file, not by coverage (see the store's sidecar writes).
            Self::Sidecar(sidecar) => Some(Coverage {
                folder: sidecar.folder.clone(),
                whole_subtree: false,
            }),
            Self::Decided { .. } => None,
        }
    }

    /// The root-relative path the list shows for this entry, or `None` for a
    /// folder reading, which the list shows on the entries it settles.
    pub(crate) fn display_path(&self) -> Option<&str> {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => Some(&candidate.display_path),
            Self::Invalid(candidate) => Some(&candidate.display_path),
            Self::Decided { .. } | Self::Sidecar(_) => None,
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
    let numbers: std::collections::HashSet<u32> = names
        .iter()
        .filter_map(|name| folder_part_number(name))
        .collect();
    numbers.contains(&1) && numbers.contains(&2)
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

/// The part of a watched root one stored entry reads its files from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coverage {
    pub folder: PathBuf,
    /// Every file below `folder`, rather than `folder`'s own.
    pub whole_subtree: bool,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl Coverage {
    /// Whether two entries read any of the same files, and so cannot both
    /// stand: one reading of a folder replaces the other.
    pub(crate) fn overlaps(&self, other: &Coverage) -> bool {
        (self.whole_subtree && other.folder.starts_with(&self.folder))
            || (other.whole_subtree && self.folder.starts_with(&other.folder))
            || self.folder == other.folder
    }
}

/// How one folder reads, as stored: whether its releases are one, who said
/// so, and the key of the grouping that says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderReading {
    pub decision: FolderReleaseDecision,
    pub author: FolderReleaseDecisionAuthor,
    pub grouping: String,
}

/// How each folder under one watched root reads, loaded before its scan
/// begins, by the folder's root-relative path.
#[derive(Debug, Clone, Default)]
pub struct FolderReleaseDecisions(HashMap<String, FolderReading>);

impl FolderReleaseDecisions {
    pub fn new(readings: HashMap<String, FolderReading>) -> Self {
        Self(readings)
    }

    /// How the folder at `relative_folder_path` reads, if anything is stored.
    pub(crate) fn get(&self, relative_folder_path: &str) -> Option<&FolderReading> {
        self.0.get(relative_folder_path)
    }

    /// Read `relative_folder_path` as `reading`, whatever was stored for it —
    /// the reading a decision about to be stored gives.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn insert(&mut self, relative_folder_path: String, reading: FolderReading) {
        self.0.insert(relative_folder_path, reading);
    }
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
    /// The grouping this release reads several folders as, when it is one —
    /// the folders themselves are [`CategorizedFiles::parts`]. The release's
    /// key is then the grouping's, not the folder's.
    pub grouping: Option<String>,
}

impl FolderCandidate {
    pub fn track_count(&self) -> u32 {
        self.files.track_count()
    }

    /// The key this release is stored and addressed under: its grouping's, or
    /// its folder's.
    pub fn key(&self) -> String {
        self.grouping
            .clone()
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }

    /// The folders on disk this release is read from.
    pub fn source_folders(&self) -> Vec<PathBuf> {
        if self.files.parts.is_empty() {
            vec![self.path.clone()]
        } else {
            self.files
                .parts
                .iter()
                .map(|part| part.folder.clone())
                .collect()
        }
    }
}

#[cfg(test)]
mod heuristic_tests {
    use super::*;

    /// Parts named `names`, each holding one folder's audio.
    fn discs(names: &[&str]) -> Vec<FolderPart> {
        names
            .iter()
            .map(|name| FolderPart {
                name: name.to_string(),
                audio: PartAudio::OneFolder,
            })
            .collect()
    }

    fn decide(names: &[&str]) -> FolderReleaseDecision {
        heuristic_folder_release_decision(false, &discs(names))
    }

    /// Twelve discs numbered `disc 01` to `disc 12`, each one folder of
    /// tracks, are one release.
    #[test]
    fn a_zero_padded_run_of_twelve_discs_is_one_release() {
        let names: Vec<String> = (1..=12).map(|disc| format!("disc {disc:02}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(decide(&names), FolderReleaseDecision::CombineAsOneRelease);
    }

    /// Two numbered folders that each wrap whole albums — three in the
    /// first, one in the second — are four releases however they number
    /// themselves.
    #[test]
    fn a_numbered_part_holding_several_albums_is_no_disc() {
        let parts = [
            FolderPart {
                name: "01_Artist_Originals".to_string(),
                audio: PartAudio::SeveralFolders,
            },
            FolderPart {
                name: "02_Artist_Remasters".to_string(),
                audio: PartAudio::OneFolder,
            },
        ];
        assert_eq!(
            decide(&["01_Artist_Originals", "02_Artist_Remasters"]),
            FolderReleaseDecision::CombineAsOneRelease,
            "the names alone do number 1 and 2"
        );
        assert_eq!(
            heuristic_folder_release_decision(false, &parts),
            FolderReleaseDecision::KeepAsSeparateReleases
        );
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
            heuristic_folder_release_decision(true, &discs(&["Disc 1", "Disc 2"])),
            FolderReleaseDecision::KeepAsSeparateReleases
        );
    }

    /// Names with no part 1 or no part 2, or a folder with tracks of its own,
    /// can't be one release's discs whatever the folders hold.
    #[test]
    fn names_without_parts_one_and_two_cannot_number_a_set() {
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        assert!(names_could_number_one_release(
            false,
            &names(&["CD1", "CD2", "Bonus"])
        ));
        assert!(names_could_number_one_release(
            false,
            &names(&["Part 01", "Part 02", "Part 99"])
        ));
        assert!(!names_could_number_one_release(
            false,
            &names(&["1990 - First", "1992 - Second"])
        ));
        assert!(!names_could_number_one_release(
            false,
            &names(&["Release 01", "Release 99"])
        ));
        assert!(!names_could_number_one_release(
            false,
            &names(&["Live in Tokyo", "Live in Osaka"])
        ));
        assert!(!names_could_number_one_release(
            true,
            &names(&["Disc 1", "Disc 2"])
        ));
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
