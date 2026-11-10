//! One candidate as its pane shows it: the folder and its files, the draft
//! and where it came from, what identification and an import say about it,
//! and the stage its live standing puts it at.

use super::cover_art::CoverChoice;
use super::folder_scanner::FolderCandidate;
use super::mapping::MappingTable;
use super::triage::{
    import_status_of, stated_folder_check, CandidateLiveState, TriageImportStatus,
    TriageMetadataSummary, TriagePlacement, TriageRuntimeFacts,
};
use super::types::{MetadataProvenance, RawReleaseEdit, ReleaseLink};
use super::{FileEvidence, ImportFailure, ImportedRelease};
use crate::CandidateSession;
use crate::TriageTab;
use bae_catalogs::remote_image::RemoteCover;
use bae_identify::signals::Signals;
use bae_identify::{IdentifyState, VerdictSummary};
use bae_records::LibraryStatus;

/// One candidate as the pane reads it, before its commands are decided.
///
/// The row is built here from what the tables say, its standing among them;
/// [`Self::resolve`] decides the detail's live state from the two.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateDetailProjection {
    pub candidate: FolderCandidate,
    pub source_error: Option<super::GroupingBlock>,
    pub actionable: bool,
    pub skipped: bool,
    /// The identify state the stored verdict stands back up as — the answer a
    /// row shows when no run is in flight.
    pub resumed_identify_state: IdentifyState,
    /// The stored verdict's columns, as the list reads them. `None` with no
    /// stored verdict for the candidate's current file shape.
    pub verdict: Option<VerdictSummary>,
    /// The signals identification settled on, or `None` before it has.
    pub signals: Option<Signals>,
    /// What is running for the candidate, from its standing.
    pub standing: TriageRuntimeFacts,
    /// Where the list places the candidate, as its row in the list's
    /// placement says.
    pub listed: ListedRelease,
    /// The draft the candidate holds, or the release this device imported
    /// from its files.
    pub stage: ProjectedStage,
}

/// What a candidate's pane draws besides its folder, as the tables hold it:
/// the draft an import would write, or, once this device imported the
/// candidate's files, the library release it became, the candidate holding
/// no draft.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectedStage {
    Drafting(Box<DraftProjection>),
    Imported {
        release: ImportedRelease,
        /// The folder's images, read from its scan: the gallery beside the
        /// library release, whose editor lists the release's files.
        images: Vec<super::mapping::MappingImage>,
    },
}

/// A candidate's draft and what goes with it, as the pane reads them.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftProjection {
    pub metadata_provenance: Option<MetadataProvenance>,
    /// What the candidate is linked to in the catalogs.
    pub release_link: Option<ReleaseLink>,
    /// Who wrote the draft, which decides whether a valid one is the answer.
    pub metadata_author: crate::MetadataAuthor,
    /// Every catalog the linked releases describe the release in, in the
    /// order surfaces list catalogs. Empty with no link.
    pub records: Vec<bae_catalogs::records::ReleaseRecord>,
    /// Whether the linked release is already in the library.
    pub picked_library_status: Option<LibraryStatus>,
    /// The library album an import of it files under.
    pub album: crate::DraftAlbum,
    /// The candidate's one editable metadata draft.
    pub metadata_draft: RawReleaseEdit,
    /// What the library holds, as this read found it, for every artist credit
    /// the draft carries.
    pub artist_resolutions: Vec<bae_records::ResolvedCredit>,
    /// Every source unit the folder offers, with the track committing makes of
    /// it. Every audio row awaits a pick until there is one.
    pub mapping: MappingTable,
    /// The cover this candidate commits with: its selection, the picked
    /// release's default, or the folder's default image.
    pub cover: Option<CoverChoice>,
    /// The covers the picker offers from the catalogs: what the applied
    /// release offered, stored with the draft when it was applied.
    pub remote_covers: Vec<RemoteCover>,
    /// What this candidate's identification asks about, as the person left it.
    pub lookup_choices: bae_identify::LookupChoices,
    /// The last import of this candidate that failed.
    pub failure: Option<ImportFailure>,
    /// Where the pane was when the person last left this candidate. `None`
    /// before the pane has been touched.
    pub session: Option<CandidateSession>,
}

/// Where the list places one release, as its row in the list's placement
/// (`import_list_placement`) holds it: what its commands are decided from,
/// and where it stands among Found's states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedRelease {
    pub basis: crate::CandidateActionBasis,
    /// Where it stands among Found's states, what is running for it
    /// included; `None` off Found.
    pub standing: Option<crate::PendingStanding>,
}

impl DraftProjection {
    /// The pane's session: the stored one, or the one the pane opens on for
    /// a candidate nobody has touched, `verdict_stored` saying whether a
    /// lookup answered for it.
    pub fn session_or_initial(&self, verdict_stored: bool) -> CandidateSession {
        self.session.clone().unwrap_or_else(|| {
            CandidateSession::initial(
                self.metadata_provenance.is_some() || self.release_link.is_some(),
                verdict_stored,
            )
        })
    }
}

impl ImportCandidateDetailProjection {
    /// The candidate's draft and what goes with it; `None` once this device
    /// imported its files, when it holds none.
    pub fn draft(&self) -> Option<&DraftProjection> {
        match &self.stage {
            ProjectedStage::Drafting(draft) => Some(draft),
            ProjectedStage::Imported { .. } => None,
        }
    }

    /// The pane's value: the candidate with the commands and state its
    /// standing gives it.
    pub fn resolve(self) -> ImportCandidateDetail {
        let Self {
            candidate,
            source_error,
            actionable,
            skipped,
            resumed_identify_state,
            verdict,
            signals,
            standing: facts,
            listed,
            stage,
        } = self;
        let import_step = facts.import;
        let importing = facts.importing();
        let file_evidence = signals.as_ref().map(crate::file_evidence).unwrap_or_default();
        let placement = listed.basis.placement;
        let live = CandidateLiveState::of(&listed.basis, facts, listed.standing);
        let (import_status, pane_placement, stage) = match stage {
            ProjectedStage::Imported { release, images } => (
                import_status_of(Some(&release), source_error.as_ref(), None),
                CandidatePanePlacement::Done,
                CandidateStage::Imported { release, images },
            ),
            ProjectedStage::Drafting(draft) => {
                let session = draft.session_or_initial(verdict.is_some());
                let DraftProjection {
                    metadata_provenance,
                    release_link,
                    metadata_author,
                    records,
                    picked_library_status,
                    album,
                    metadata_draft,
                    artist_resolutions,
                    mapping,
                    cover,
                    remote_covers,
                    lookup_choices,
                    failure,
                    session: _,
                } = *draft;
                let import_status = import_status_of(
                    None,
                    source_error.as_ref(),
                    failure.as_ref().map(|failure| &failure.reason),
                );
                // The attempt running now is what the pane shows; an earlier
                // failure is behind it.
                let failure = if importing { None } else { failure };
                let verdict = verdict.as_ref().filter(|_| actionable);
                let draft_valid = metadata_draft.clone().shape().is_ok();
                // The catalogs the linked release is described in, as the row
                // names them: only a linked candidate names any.
                let draft_records = || {
                    let linked = release_link.as_ref().filter(|_| actionable);
                    let answered = linked.is_some() || metadata_provenance.is_some();
                    super::triage::TriageReading::of(
                        TriageMetadataSummary::of(&metadata_draft, answered).as_ref(),
                        linked,
                        records,
                    )
                    .records()
                };
                let pane_placement = match placement.tab() {
                    TriageTab::Pending => CandidatePanePlacement::Pending {
                        folder_check: (placement == TriagePlacement::Pending)
                            .then(|| stated_folder_check(metadata_author, draft_valid, verdict))
                            .flatten(),
                        records: draft_records(),
                    },
                    TriageTab::Skipped => {
                        CandidatePanePlacement::Skipped { records: draft_records() }
                    }
                    TriageTab::Done => CandidatePanePlacement::Done,
                };
                let metadata_draft_is_blank = metadata_draft.is_blank();
                (
                    import_status,
                    pane_placement,
                    CandidateStage::Drafting(Box::new(CandidateDraftPane {
                        picked_library_status,
                        album,
                        metadata_draft,
                        artist_resolutions,
                        metadata_draft_is_blank,
                        metadata_provenance,
                        release_link,
                        metadata_author,
                        mapping,
                        cover,
                        remote_covers,
                        lookup_choices,
                        failure,
                        session,
                    })),
                )
            }
        };
        let import_status = match import_step {
            Some(step) => Some(CandidateImportStatus::Importing { step }),
            None => import_status.map(CandidateImportStatus::of),
        };
        ImportCandidateDetail {
            candidate,
            actionable,
            skipped,
            resumed_identify_state,
            placement: pane_placement,
            live,
            import_status,
            file_evidence,
            signals,
            stage,
        }
    }
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
        /// not pass, stated beside its Import: the release's
        /// [`bae_identify::VerdictSummary::folder_check`].
        folder_check: Option<bae_identify::FolderCheck>,
        /// Every catalog the linked release is described in, in the order
        /// surfaces list catalogs. Empty for a candidate linked to no release.
        records: Vec<bae_catalogs::records::ReleaseRecord>,
    },
    /// Skipped, with every catalog the linked release is described in, as
    /// for Pending.
    Skipped { records: Vec<bae_catalogs::records::ReleaseRecord> },
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
    fn of(stored: TriageImportStatus) -> Self {
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
    pub actionable: bool,
    pub skipped: bool,
    pub resumed_identify_state: IdentifyState,
    /// Where the queue places the candidate, with what the pane states beside
    /// it.
    pub placement: CandidatePanePlacement,
    /// What is running for the candidate right now, and the commands its row
    /// offers with it.
    pub live: CandidateLiveState,
    /// Where the candidate's import stands for the pane: the one running now,
    /// or what the last one left in the tables.
    pub import_status: Option<CandidateImportStatus>,
    /// Extracted identifying signals pinned to their source files. Independent
    /// of the selected pressing; which lookups found a result is the run's
    /// ledger's to say.
    pub file_evidence: Vec<FileEvidence>,
    pub signals: Option<Signals>,
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
    Imported {
        release: ImportedRelease,
        /// The folder's images, read from its scan: the gallery beside the
        /// library release, whose editor lists the release's files.
        images: Vec<super::mapping::MappingImage>,
    },
}

/// A candidate's draft and what goes with it, as its pane draws them.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateDraftPane {
    pub picked_library_status: Option<LibraryStatus>,
    /// The library album an import of it files under.
    pub album: crate::DraftAlbum,
    pub metadata_draft: RawReleaseEdit,
    /// What the library holds for every artist credit of the draft and the
    /// mapping rows, as the pane's live read found it.
    pub artist_resolutions: Vec<bae_records::ResolvedCredit>,
    pub metadata_draft_is_blank: bool,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// What the candidate is linked to in the catalogs, which Unlink clears.
    pub release_link: Option<ReleaseLink>,
    /// Who wrote the draft: nobody, the tag prefill, identification's own
    /// pick, or the person.
    pub metadata_author: crate::MetadataAuthor,
    pub mapping: MappingTable,
    pub cover: Option<CoverChoice>,
    pub remote_covers: Vec<RemoteCover>,
    /// What this candidate's identification asks about: the signals its runs
    /// leave out and the catalog numbers they look up. A control that changes
    /// one sends the whole value back, computed from this.
    pub lookup_choices: bae_identify::LookupChoices,
    pub failure: Option<ImportFailure>,
    /// Where the pane was when the person last left this candidate, or where
    /// it opens for one nobody has touched.
    pub session: CandidateSession,
}
