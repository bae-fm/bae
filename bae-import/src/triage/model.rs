use super::*;

/// The sidebar's three lifecycle tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TriageTab {
    #[default]
    Pending,
    Done,
    Skipped,
}

/// Which tab a row belongs to, and whether its last import failed, read from
/// the tables alone; what is running for it is its [`TriageListLiveState`].
///
/// The list's placement decides it (`import_list_entry`, in the schema), in
/// one order: **Done first**, a folder whose files this device imported, in
/// this session or an earlier one, wherever the folder sits now and whether or
/// not it was ever skipped; **then Skipped**, a decision the person already
/// made; **then a failed attempt**, Pending work with its failure stated;
/// **otherwise Pending**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriagePlacement {
    /// Not imported here and not skipped.
    Pending,
    /// The last import attempt failed; why is the row's [`TriageImportStatus`].
    Failed,
    /// This device imported a release from the folder's files.
    Done,
    Skipped,
}

impl TriagePlacement {
    pub fn tab(&self) -> TriageTab {
        match self {
            Self::Pending | Self::Failed => TriageTab::Pending,
            Self::Done => TriageTab::Done,
            Self::Skipped => TriageTab::Skipped,
        }
    }
}

/// What identification is doing for a candidate right now, whatever its
/// placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentificationStatus {
    /// The candidate has been admitted but its driver has not started.
    Queued,
    /// Signals are being gathered or a provider lookup is in flight.
    Running,
    /// The run has a result that picks no release unattended, and is
    /// committing it.
    Finalizing,
    /// The run picked its release unattended, and the release is being read
    /// in full before the answer is committed. `import_asked`: a person asked
    /// for the candidate to be imported once the answer is.
    ReadingPick { import_asked: bool },
    /// The result could not be committed; `failure` says why.
    FinalizationFailed { failure: crate::SaveFailure },
}

impl IdentificationStatus {
    pub(crate) fn in_progress(&self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::Finalizing | Self::ReadingPick { .. })
    }
}

/// What a row's text column says about its release: nothing yet, a draft, a
/// draft of a candidate linked to a catalog pressing, or one linked to an
/// album whose pressing is unknown; or, for a release this device imported,
/// what the library release's catalog records describe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageReading {
    /// No draft, from tags or anywhere: the row leads with its folder.
    Unidentified,
    /// A draft of a candidate linked to no release, or an imported release
    /// no catalog record describes.
    Prefilled,
    /// A draft of a candidate linked to a catalog pressing, or an imported
    /// release a catalog's record of its pressing describes, with every
    /// catalog that describes it.
    Identified { records: Vec<bae_catalogs::records::ReleaseRecord> },
    /// A draft of a candidate linked to an album, or an imported release only
    /// catalogs' records of its album describe, with every catalog's record
    /// of the album: the row says its pressing is unknown.
    IdentifiedAlbum { records: Vec<bae_catalogs::records::ReleaseRecord> },
}

impl TriageReading {
    /// How a row reads from its draft, its release link, and the catalog
    /// records what it is linked to is described in.
    pub fn of(
        has_summary: bool,
        link: Option<&ReleaseLink>,
        records: Vec<bae_catalogs::records::ReleaseRecord>,
    ) -> Self {
        if !has_summary {
            return Self::Unidentified;
        }
        match link {
            Some(ReleaseLink::Pressing(_)) => Self::Identified { records },
            Some(ReleaseLink::Album(_)) => Self::IdentifiedAlbum { records },
            None => Self::Prefilled,
        }
    }
}

/// One candidate's row.
#[derive(Debug, Clone, PartialEq)]
pub struct TriageRow {
    /// The candidate's key — the key every other import call takes.
    pub candidate_key: crate::CandidateKey,
    /// The folder's name, which is the row's title while it has no draft.
    pub folder_name: String,
    /// The watched folder this candidate was scanned from
    /// (`WatchedFolder::path`).
    pub watched_folder_path: std::path::PathBuf,
    pub actionable: bool,
    pub placement: TriagePlacement,
    /// The album title and credits, when the row shows them.
    pub metadata_summary: Option<crate::AlbumLine>,
    /// The cover the row draws: the candidate's chosen one, or for a release
    /// this device imported, the library release's.
    pub cover: Option<crate::CoverImageSource>,
    /// How the candidate's last import ended.
    pub import_status: Option<TriageImportStatus>,
    /// Where the candidate's draft was read from, once a source was applied;
    /// none for a release this device imported, which reads the library.
    pub metadata_provenance: Option<crate::MetadataProvenance>,
    /// What the candidate is linked to in the catalogs; none for a release
    /// this device imported, whose reading names the library release's
    /// records.
    pub release_link: Option<ReleaseLink>,
    pub reading: TriageReading,
}

/// What a candidate row says below its title and credits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageStateLine {
    Importing { step: crate::ImportStep },
    Uploading { release_id: String },
    QueuedToUpload,
    Error { failure: bae_catalogs::InternalFailure },
}

/// The status a candidate row adds to its title and credits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageRowStatus {
    QueuedToImport,
    Line { line: TriageStateLine },
}

impl TriageRowStatus {
    pub(crate) fn of(
        import_status: Option<&TriageImportStatus>,
        upload: Option<&bae_records::UploadProgress>,
        import: Option<crate::ImportStep>,
        standing: Option<&PendingStanding>,
    ) -> Option<Self> {
        match import {
            Some(crate::ImportStep::Preparing(crate::PrepareStep::Queued)) => {
                return Some(Self::QueuedToImport);
            }
            Some(step) => return Some(Self::Line { line: TriageStateLine::Importing { step } }),
            None => {}
        }
        let line = if let Some(TriageImportStatus::Complete { release }) = import_status {
            match upload?.activity() {
                bae_records::UploadActivity::Uploading => {
                    TriageStateLine::Uploading { release_id: release.release_id.clone() }
                }
                bae_records::UploadActivity::Queued => TriageStateLine::QueuedToUpload,
            }
        } else if let Some(PendingStanding::Error { failure }) = standing {
            TriageStateLine::Error { failure: failure.clone() }
        } else {
            return None;
        };
        Some(Self::Line { line })
    }
}

/// A folder that failed validation, as its row under Skipped and the pane
/// that shows its details show it: what its scan row stores, which the
/// screen's statement reads.
#[derive(Debug, Clone, PartialEq)]
pub struct InvalidRow {
    /// The key the scan stores the row under and every import call takes: its
    /// folder's path, or its grouping's key.
    pub candidate_key: crate::CandidateKey,
    /// Its folder; for a grouping, the folder the grouping reads as one
    /// release.
    pub folder: std::path::PathBuf,
    /// The folder's name as `coven::path_text` shows it.
    pub folder_name: String,
    /// The watched folder the scan found it in (`WatchedFolder::path`).
    pub watched_folder_path: std::path::PathBuf,
    /// Whether the row is several folders that a grouping reads as one
    /// release, which the row offers to separate.
    pub separable: bool,
    /// Why it failed validation, as its scan stored it.
    pub reason: crate::InvalidReason,
    /// What its tags call the release, when they name an album.
    pub tags: Option<crate::folder_scanner::SkippedTags>,
    /// The cover the row draws: the image the folder's cover rules pick among
    /// its images that decode.
    pub cover: Option<crate::CoverImageSource>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriageGroup {
    pub key: FolderReleaseDecisionKey,
    pub name: String,
    /// Whether the rows under this header are this folder read as several
    /// releases, which the header offers to read as one.
    pub combinable: bool,
}

/// How many rows each tab holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TriageTabCounts {
    pub pending: u32,
    pub done: u32,
    pub skipped: u32,
}

impl TriageTabCounts {
    /// How many entries `tab` holds.
    pub(crate) fn of(&self, tab: TriageTab) -> u32 {
        match tab {
            TriageTab::Pending => self.pending,
            TriageTab::Done => self.done,
            TriageTab::Skipped => self.skipped,
        }
    }
}

/// The release this device imported from the folder's files, or its stored
/// import failure.
#[derive(Debug, Clone, PartialEq)]
pub enum TriageImportStatus {
    Complete {
        release: ImportedRelease,
    },
    /// The last import failed, and why.
    Error {
        failure: crate::ImportFailureReason,
    },
    /// A release read from several folders that cannot be worked on, and why.
    Blocked {
        reason: crate::GroupingBlock,
    },
}

/// The release this device imported from one candidate's files, found by the
/// folder hash it recorded (`local_release_origin`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRelease {
    pub release_id: String,
    pub album_id: String,
}
