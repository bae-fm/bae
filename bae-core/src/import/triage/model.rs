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
/// the tables alone; what is running for it is its [`CandidateLiveState`].
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

    /// The skip command the placement allows; none once an import finished or
    /// failed.
    pub fn skip_action(&self) -> Option<TriageSkipAction> {
        match self {
            Self::Pending => Some(TriageSkipAction::Skip),
            Self::Skipped => Some(TriageSkipAction::Unskip),
            Self::Failed | Self::Done => None,
        }
    }
}

/// The skip-state command a placement allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriageSkipAction {
    Skip,
    Unskip,
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
    FinalizationFailed { failure: crate::import::SaveFailure },
}

/// The candidate's stored draft as the row shows it, independent of what
/// identification matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriageMetadataSummary {
    pub album_title: String,
    /// The draft's album credits, printed as one line.
    pub album_artists: String,
}

impl TriageMetadataSummary {
    /// `None` for a blank draft of a candidate that is `answered` by nothing:
    /// no draft read from anywhere and no release link.
    pub(crate) fn of(draft: &crate::import::RawReleaseEdit, answered: bool) -> Option<Self> {
        if draft.is_blank() && !answered {
            return None;
        }
        Some(Self {
            album_title: draft.album_title.clone(),
            album_artists: crate::import::credited_line(&draft.album_artist_assignments),
        })
    }
}

/// What a row's text column says about its release: nothing yet, a draft, a
/// draft of a candidate linked to a catalog pressing, or one linked to an
/// album whose pressing is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageReading {
    /// No draft, from tags or anywhere: the row leads with its folder.
    Unidentified,
    /// A draft of a candidate linked to no release.
    Prefilled,
    /// A draft of a candidate linked to a catalog pressing, with every
    /// catalog that describes that pressing.
    Identified {
        records: Vec<crate::import::ReleaseRecord>,
    },
    /// A draft of a candidate linked to an album, with every catalog's record
    /// of the album: the row says its pressing is unknown.
    IdentifiedAlbum {
        records: Vec<crate::import::ReleaseRecord>,
    },
}

impl TriageReading {
    /// How a row reads from its draft, its release link, and the catalog
    /// records what it is linked to is described in.
    pub fn of(
        summary: Option<&TriageMetadataSummary>,
        link: Option<&ReleaseLink>,
        records: Vec<crate::import::ReleaseRecord>,
    ) -> Self {
        if summary.is_none() {
            return Self::Unidentified;
        }
        match link {
            Some(ReleaseLink::Pressing(_)) => Self::Identified { records },
            Some(ReleaseLink::Album(_)) => Self::IdentifiedAlbum { records },
            None => Self::Prefilled,
        }
    }

    /// Every catalog record the row names: none unless it is linked.
    pub fn records(self) -> Vec<crate::import::ReleaseRecord> {
        match self {
            Self::Identified { records } | Self::IdentifiedAlbum { records } => records,
            Self::Unidentified | Self::Prefilled => Vec::new(),
        }
    }
}

/// One candidate's row.
#[derive(Debug, Clone, PartialEq)]
pub struct TriageRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    /// The folder's name, which is the row's title while it has no draft.
    pub folder_name: String,
    /// The watched folder this candidate was scanned from
    /// (`WatchedFolder::path`).
    pub watched_folder_path: String,
    pub display_path: String,
    pub actionable: bool,
    pub placement: TriagePlacement,
    /// What the row's commands are decided from in the tables.
    pub action_basis: CandidateActionBasis,
    pub metadata_summary: Option<TriageMetadataSummary>,
    /// The cover the row draws: the chosen one, the matched artwork, or the
    /// folder's default image.
    pub cover: Option<crate::import::CoverImageSource>,
    /// How the candidate's last import ended.
    pub import_status: Option<TriageImportStatus>,
    /// How many files of the release this device imported from the
    /// candidate's files wait in this device's upload queue and how many
    /// are being sent, as the Storage Manager counts them: read only for a
    /// row with an imported release, and `None` while none of its files
    /// waits. How far the bytes have got is the release's upload transfer.
    pub upload: Option<crate::library::UploadProgress>,
    /// Where the candidate's draft was read from, once a source was applied.
    pub metadata_provenance: Option<crate::import::MetadataProvenance>,
    /// What the candidate is linked to in the catalogs.
    pub release_link: Option<ReleaseLink>,
    pub reading: TriageReading,
    /// Whether the person has selected the row.
    pub selected: bool,
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

    /// Count `count` more entries on `tab`.
    pub(crate) fn add(&mut self, tab: TriageTab, count: u32) {
        match tab {
            TriageTab::Pending => self.pending += count,
            TriageTab::Done => self.done += count,
            TriageTab::Skipped => self.skipped += count,
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
        failure: crate::import::ImportFailureReason,
    },
    /// A release read from several folders that cannot be worked on, and why.
    Blocked {
        reason: crate::import::GroupingBlock,
    },
}
