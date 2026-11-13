use super::*;
use crate::CandidateKey;
use crate::RelativeFolder;

/// Why a folder that looks like a release (it holds audio) can't be imported,
/// as its skipped files say. The UI localizes the typed variant for the
/// Skipped tab.
///
/// Only real defects invalidate: a name two files show, and audio bae can't
/// play. A sheet that disagrees with what is on disk —
/// naming absent audio, failing to parse, or naming a codec bae can't play from
/// a single-file CUE — leaves the sheet unbound and the folder importable, and
/// an image that will not decode is carried as a damaged one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidReason {
    /// Two or more of its files show as `name`, its path within the folder. A
    /// name that isn't UTF-8 shows escaped (`Caf\xE9.flac`), the same as a
    /// name spelled that way, and a release holds each name once: it is the
    /// file's name in the library and on export.
    #[error("more than one file is named {name}")]
    SameName { name: String },
    /// `count` of its audio files can't be played: never zero.
    #[error("{count} damaged audio files")]
    DamagedAudio { count: u32 },
    #[error("no valid audio files")]
    NoValidAudio,
}

/// The first of `names` that an earlier one already is: what makes a release
/// holding files by these names unimportable (see [`InvalidReason::SameName`]).
pub(crate) fn shared_name<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = std::collections::HashSet::new();
    names.into_iter().find(|name| !seen.insert(*name))
}

impl InvalidReason {
    /// The reason `files` give: a name two of them show, else the audio among
    /// them bae can't play, or, with none of that, that they hold no audio at
    /// all.
    pub fn of(files: &[SkippedFile]) -> Self {
        if let Some(name) = shared_name(files.iter().map(|file| file.relative_path.as_str())) {
            return Self::SameName { name: name.to_string() };
        }
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
    /// Named as a document of `kind`.
    Document {
        kind: bae_records::DocumentKind,
    },
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
            file: entry.file.version.clone(),
            role: match entry.role {
                FileRole::Audio => ProposedRole::Audio,
                FileRole::TrackSheet { .. } => ProposedRole::Cue,
                FileRole::Artwork => ProposedRole::Image,
                FileRole::DamagedImage => ProposedRole::DamagedImage,
                FileRole::Document { kind } => ProposedRole::Document { kind },
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
            file: FileVersion { path: folder.join(relative_path), size: 0, modified_at_ns: 1 },
            role: ProposedRole::DamagedAudio { damage: AudioDamage::Empty },
        }
    }
}

/// What a skipped folder's tags call the release: the album title the first
/// of its playable files to state one gives, with the line of that file's
/// album artists, or of its track artists when it names no album artist.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SkippedTags {
    pub album_title: String,
    pub artist: Option<bae_types::credits::CreditLine>,
}

/// A leaf folder that looks like a release (it has audio) but failed
/// validation. It can't be imported, so it carries no identify state — only
/// what the scan read of its files, to surface it under the Skipped tab with
/// its reason and let the person look in.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidCandidate {
    /// Root path of the folder that failed validation — for a grouping, the
    /// folder the grouping reads as one release.
    pub path: PathBuf,
    /// Display name: the folder's name as `coven::path_text` shows it.
    pub name: String,
    /// Absolute path of the watched folder this was scanned from — the
    /// candidate-list group it belongs to. Equal to the scan root.
    pub watched_folder_path: PathBuf,
    /// The folder below the watched folder the list shows it as.
    pub relative_folder: RelativeFolder,
    /// The grouping this row reads several folders as, when it is one. The
    /// row's key is then the grouping's, not the folder's.
    pub grouping: Option<CandidateKey>,
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
        let cover = crate::cover_art::default_local_cover_file(&images)?;
        self.files.iter().find(|file| file.relative_path == cover.relative_path)
    }

    /// Where its row draws [`Self::cover`] from.
    pub fn cover_image(&self) -> Option<crate::CoverImageSource> {
        self.cover().map(|file| crate::CoverImageSource::Local { file: file.file.clone() })
    }
}

#[cfg(desktop)]
impl InvalidCandidate {
    /// The key this row is stored and addressed under.
    pub fn key(&self) -> CandidateKey {
        self.grouping.clone().unwrap_or_else(|| CandidateKey::folder(&self.path))
    }
}

/// What a walk of a watched folder yields, in the order it read them: the
/// entries the store keeps for its folders, and the readings it decided for
/// folders nothing was stored for.
#[derive(Debug, Clone, PartialEq)]
pub enum WalkItem {
    Entry(FolderEntry),
    /// The scan read a folder its own way, because nothing was stored for it:
    /// the grouping it proposes at that folder, under the key the scan gave
    /// it. It is stored as the folder's decision, not as a scan entry, before
    /// any entry the walk yields after it, so the candidates that name it name
    /// a stored grouping.
    Reading {
        key: FolderReleaseDecisionKey,
        decision: FolderReleaseDecision,
        grouping: CandidateKey,
    },
}

/// What the store keeps for a folder the scan read: a release entry, or the
/// files no release there owns. A scan writes them in the order the walk
/// yielded them, since a release takes its files from a sidecar holding them.
#[derive(Debug, Clone, PartialEq)]
pub enum FolderEntry {
    Release(ScanItem),
    /// The files under a folder that none of the releases read there owns.
    Sidecar(FolderSidecar),
}

/// One release entry the store keeps for a folder the scan read: a release
/// candidate, or one that looked like a release but failed validation.
///
/// Comparable, because that is how a pass tells what it changed from what it
/// merely found again: a walk of an untouched folder produces items equal to
/// the ones already stored, and equal items are written and announced to
/// nobody.
#[derive(Debug, Clone, PartialEq)]
pub enum ScanItem {
    /// A release approximation completed before its enclosing folder's reading
    /// was known. It is visible scan progress, but identification must wait for
    /// a later [`Self::Valid`] update.
    Discovered(FolderCandidate),
    Valid(FolderCandidate),
    Invalid(InvalidCandidate),
}

/// The files under a folder that none of the releases the scan read there
/// owns: a cover, a booklet, a folder of scans beside disc folders kept as
/// releases of their own. The folder holds no audio of its own and is not read
/// as one release, so no release below it takes them; a grouping whose
/// releases all sit directly in the folder does (see
/// `crate::grouping::shared_parent`).
///
/// Only a folder below the watched root has one: the root is never a release,
/// so nothing is ever read as its own. Which folder's files a grouping reads
/// is one rule for every grouping (see `shared_parent`): a folder read as one
/// release by the scan and releases picked together read the same files.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderSidecar {
    pub watched_folder_path: PathBuf,
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
    /// A file among them is broken — a zero-byte track — or two of them show
    /// one name, so a release that takes them in cannot be imported. Every
    /// file, as the scan read it.
    Invalid(Vec<SkippedFile>),
    /// A download into the folder is still running, so what it will hold is
    /// not known yet; a release that takes them in waits for it, as a
    /// release with a download running in it is not offered at all.
    Downloading,
}

/// Only the durable folder-scan tables key items this way, and those are
/// desktop-only.
#[cfg(desktop)]
impl ScanItem {
    /// The key this entry is stored and addressed under.
    pub(crate) fn key(&self) -> CandidateKey {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => candidate.key(),
            Self::Invalid(candidate) => candidate.key(),
        }
    }

    /// The folders this entry's files are read from, and whether it reads
    /// everything below its folder: a grouping, or a release lent the files
    /// of a wrapper folder around it. A release that reads only its own
    /// folder covers that folder and nothing below it that holds audio.
    pub(crate) fn coverage(&self) -> Coverage {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => Coverage {
                folder: candidate.file_root.clone(),
                whole_subtree: candidate.scope == ReleaseFileScope::Recursive,
            },
            Self::Invalid(candidate) => Coverage {
                folder: candidate.path.clone(),
                whole_subtree: candidate.grouping.is_some(),
            },
        }
    }

    /// The folder below the watched root the list shows this entry as.
    pub(crate) fn relative_folder(&self) -> &RelativeFolder {
        match self {
            Self::Discovered(candidate) | Self::Valid(candidate) => &candidate.relative_folder,
            Self::Invalid(candidate) => &candidate.relative_folder,
        }
    }
}

#[cfg(desktop)]
impl FolderSidecar {
    /// The folder whose files it holds, and nothing below it: what a reading
    /// of one folder checks it stays inside. Which release holds a file is
    /// settled file by file, not by coverage (see the store's sidecar writes).
    pub(crate) fn coverage(&self) -> Coverage {
        Coverage { folder: self.folder.clone(), whole_subtree: false }
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

/// The part of a watched root one stored entry reads its files from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coverage {
    pub folder: PathBuf,
    /// Every file below `folder`, rather than `folder`'s own.
    pub whole_subtree: bool,
}

/// A folder candidate detected during filesystem scanning.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderCandidate {
    /// Root path of this release
    pub path: PathBuf,
    /// Root passed to the import revalidation walk. It differs from `path`
    /// when audio-free single-child wrappers were collapsed into this row.
    pub file_root: PathBuf,
    /// Display name: the folder's name as `coven::path_text` shows it.
    pub name: String,
    /// Pre-categorized files for this release
    pub files: CategorizedFiles,
    /// Absolute path of the watched folder this candidate was scanned from —
    /// the candidate-list group it belongs to. Equal to the scan root. The
    /// group's display name comes from the watched-folder list, not here.
    pub watched_folder_path: PathBuf,
    /// The exact file ownership rule used to build `files`.
    pub scope: ReleaseFileScope,
    /// Revision of the stored file decisions applied to `files`.
    pub file_edit_revision: u64,
    /// The folder below the watched root the queue shows it as.
    pub relative_folder: RelativeFolder,
    /// The grouping this release reads several folders as, when it is one —
    /// the folders themselves are [`CategorizedFiles::parts`]. The release's
    /// key is then the grouping's, not the folder's.
    pub grouping: Option<CandidateKey>,
}

impl FolderCandidate {
    pub fn track_count(&self) -> u32 {
        self.files.track_count()
    }

    /// The key this release is stored and addressed under: its grouping's, or
    /// its folder's.
    pub fn key(&self) -> CandidateKey {
        self.grouping.clone().unwrap_or_else(|| CandidateKey::folder(&self.path))
    }

    /// The folders on disk this release is read from.
    pub fn source_folders(&self) -> Vec<PathBuf> {
        if self.files.parts.is_empty() {
            vec![self.path.clone()]
        } else {
            self.files.parts.iter().map(|part| part.folder.clone()).collect()
        }
    }
}
