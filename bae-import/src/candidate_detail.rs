//! One candidate as its pane shows it: the folder and its files, the draft
//! and where it came from, what identification and an import say about it,
//! and the stage its live standing puts it at.

use super::cover_art::CoverChoice;
use super::folder_scanner::FolderCandidate;
use super::mapping::MappingTable;
use super::triage::{CandidateLiveState, TriageImportStatus};
use super::types::{MetadataProvenance, RawReleaseEdit, ReleaseLink};
use super::{FileEvidence, ImportFailure, ImportedRelease};
use bae_identify::signals::PaneSignals;
use bae_identify::IdentifyStateView;
use bae_records::LibraryStatus;

/// The candidate's files as its pane lists them. The pane's files statement
/// (`load_pane_files_on`) reads every fact here from the scan's rows.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneFiles {
    /// Every file of the folder, in release-relative path order.
    pub files: Vec<PaneFile>,
    /// The folder's images a cover picker can show, in the same order.
    pub cover_files: Vec<PaneFile>,
    /// Every image of the folder, in the same order: the gallery; `None`
    /// when the folder holds no image.
    pub images: Option<Vec<super::mapping::MappingImage>>,
    /// How many tracks the folder holds: its audio units.
    pub track_count: u32,
    /// What the folder's audio is, summarized over its audio files; `None`
    /// for a folder with none.
    pub source_audio: Option<bae_records::source_audio::SourceAudioSummary>,
}

/// One file of the candidate as its pane lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneFile {
    /// The file's path in the release.
    pub name: String,
    pub size: u64,
    pub file_name: String,
    /// The format of an audio file.
    pub audio_format: Option<bae_types::audio_format::AudioFormat>,
    pub role: PaneFileRole,
}

/// The job the scan proposed for one file, with what the pane states beside
/// it.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneFileRole {
    Audio,
    /// A track sheet, with how many audio tracks it lays out.
    TrackSheet {
        track_count: u32,
    },
    /// An image.
    Artwork,
    DamagedImage,
    Document,
    Other,
}

/// Where the list places one release, as its row in the list's placement
/// (`import_list_placement`) holds it: what its commands are decided from,
/// and where it stands among Found's states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ListedRelease {
    pub basis: crate::CandidateActionBasis,
    /// The commands the release offers by itself, apart from the ones a
    /// selection of several rows offers, from its placement and what is
    /// running for it.
    pub actions: Vec<crate::CandidateAction>,
    /// Where it stands among Found's states, what is running for it
    /// included; `None` off Found.
    pub standing: Option<crate::PendingStanding>,
}

/// Where the queue places the candidate a pane shows, with what the pane
/// states beside it. A Done candidate's pane is the library release it became,
/// so it carries nothing the candidate's draft says: the folder check and the
/// catalogs of the linked release are a queued candidate's alone.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidatePanePlacement {
    /// In Pending.
    Pending {
        /// The check against the folder the candidate's found release did
        /// not pass, stated beside its Import: the one the candidate's
        /// stored verdict decides on.
        folder_check: Option<bae_identify::FolderCheck>,
        /// Every catalog the linked release is described in, in the order
        /// surfaces list catalogs; `None` for a candidate linked to no
        /// release.
        records: Option<Vec<bae_catalogs::records::ReleaseRecord>>,
    },
    /// Skipped, with every catalog the linked release is described in, as
    /// for Pending.
    Skipped { records: Option<Vec<bae_catalogs::records::ReleaseRecord>> },
    /// In the library.
    Done,
}

/// Where a candidate's import stands for the pane that shows the candidate:
/// running now, or the outcome the last one left in the tables. The pane
/// draws a different surface for each, so the three are one value rather
/// than a row's stored outcome beside a live flag.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateImportStatus {
    /// An import runs, at this step.
    Importing {
        step: super::ImportStep,
    },
    /// The release this device imported from the candidate's files.
    Complete {
        release: super::ImportedRelease,
    },
    Error {
        failure: super::ImportFailureReason,
    },
    /// A release read from several folders that cannot be worked on as it
    /// stands, and why.
    Blocked {
        reason: super::GroupingBlock,
    },
}

impl CandidateImportStatus {
    pub(crate) fn of(stored: TriageImportStatus) -> Self {
        match stored {
            TriageImportStatus::Complete { release } => Self::Complete { release },
            TriageImportStatus::Error { failure } => Self::Error { failure },
            TriageImportStatus::Blocked { reason } => Self::Blocked { reason },
        }
    }
}

/// One candidate, whole: the folder with its files, what the queue makes of
/// it, the identify state it resumes, and its draft or the release this
/// device imported from it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateDetail {
    pub candidate: FolderCandidate,
    /// The candidate's files as the pane lists them.
    pub files: PaneFiles,
    /// How many folders the candidate's files come from, as SQL counts
    /// them, when they come from several; 0 when they come from one folder.
    pub part_count: u64,
    pub actionable: bool,
    pub skipped: bool,
    /// The identify state that bae rebuilds from the verdict the tables hold
    /// for the candidate's current file shape, with the cards and the
    /// decision its rows hold: what the pane shows when no run is running.
    /// `Idle` when the tables hold no verdict for that shape.
    pub resumed_identify_state: IdentifyStateView,
    /// Where the queue places the candidate, with what the pane states beside
    /// it.
    pub placement: CandidatePanePlacement,
    /// What is running for the candidate right now, and the commands its row
    /// offers with it.
    pub live: CandidateLiveState,
    /// Where the candidate's import stands for the pane: the one running now,
    /// or what the last one left in the tables.
    pub import_status: Option<CandidateImportStatus>,
    /// The candidate's Disc IDs and barcodes that name a file of it, as its
    /// pane's read selects them. Independent of the selected pressing; which
    /// lookups found a result is the run's ledger's to say.
    pub file_evidence: Vec<FileEvidence>,
    /// The signals identification settled on, or `None` before it has.
    pub signals: Option<PaneSignals>,
    pub stage: CandidateStage,
}

impl ImportCandidateDetail {
    /// Whether this device imported a release from the candidate's files.
    pub fn is_added(&self) -> bool {
        matches!(self.stage, CandidateStage::Imported { .. })
    }

    /// The candidate's draft and what goes with it; `None` once this device
    /// imported its files, when it holds none.
    pub fn draft(&self) -> Option<&CandidateDraftPane> {
        match &self.stage {
            CandidateStage::Drafting(draft) => Some(draft),
            CandidateStage::Imported { .. } => None,
        }
    }
}

/// What a candidate's pane draws besides its folder: the draft an import
/// would write, or, once this device imported the candidate's files, the
/// library release it became, the candidate holding no draft.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateStage {
    Drafting(Box<CandidateDraftPane>),
    /// An imported candidate's pane shows the folder's images,
    /// [`PaneFiles::images`], as its gallery, because the library release's
    /// editor lists the release's own files.
    Imported {
        release: ImportedRelease,
    },
}

/// A candidate's draft and what goes with it, as its pane draws them.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateDraftPane {
    pub picked_library_status: Option<LibraryStatus>,
    /// The library album an import of it files under.
    pub album: crate::DraftAlbum,
    pub metadata_draft: RawReleaseEdit,
    /// The media and Discogs details the draft's pressing Add menus offer.
    pub pressing_offers: crate::PressingOffers,
    /// What the pane's summary states of the draft's release; `None` for a
    /// draft read from the files' own tags.
    pub release_lines: Option<crate::DraftReleaseLines>,
    /// The draft's album artists, each with how it stands to the library, as
    /// the pane's live read found them; each mapping row carries its track's
    /// own. `None` when the draft names no album artist.
    pub album_artists: Option<crate::ArtistField>,
    pub metadata_draft_is_blank: bool,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// What the candidate is linked to in the catalogs, which Unlink clears.
    pub release_link: Option<ReleaseLink>,
    pub mapping: MappingTable,
    pub cover: Option<CoverChoice>,
    /// What this candidate's identification asks about: the signals its runs
    /// leave out and the catalog numbers they look up. A control that changes
    /// one sends the whole value back, computed from this.
    pub lookup_choices: bae_identify::LookupChoices,
    pub failure: Option<ImportFailure>,
    /// The failure the pane states for its last command, when it failed.
    pub pane_failure: Option<crate::PaneFailure>,
    /// Where the pane opens on the candidate.
    pub opens_on: crate::MetadataPresentation,
}
