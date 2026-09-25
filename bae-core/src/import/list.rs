//! The import tab's list: one ordered sequence of items, served a window at a
//! time.
//!
//! The tab used to be one whole-queue value — every candidate with its files,
//! its cue sheets, its boundary trees and every fetched release — rebuilt on
//! every commit and carried across the bridge twice. What the list actually
//! shows is a placement per row and a handful of columns, so that is what the
//! read gathers: [`crate::db::ImportQueueRows`] is placement columns and
//! nothing else, `flatten` turns them plus the requested view into an
//! ordered vector of item references, and only the references inside the
//! requested windows are turned into items.
//!
//! Everything the chrome around the list shows — the tab counts, the Ready
//! rows a bulk import acts on, the group keys disclosure state is retained
//! against — is computed in that same pass, so none of it can disagree with
//! the rows.
//!
//! The read is of the tables and nothing else. What is running for a candidate
//! right now — a run queued or in flight, an import that owns it — moves no row
//! between tabs and reorders nothing, so it is not an input to the list: each
//! row reads it from its own subscription, as a
//! [`CandidateLiveState`].

use super::cover_art::{CoverChoice, RemoteCover};
use super::folder_scanner::{FolderReleaseDecisionKey, InvalidCandidate};
use super::mapping::MappingTable;
use super::folder_scanner::FolderCandidate;
use super::search::ImportSearchReleaseDetail;
use super::triage::{
    import_status_of, place, CandidateActionBasis, CandidateLiveState, ImportedRow,
    TriageGroup, TriageImportStatus, TriageMetadataSummary, TriageRow,
    TriageRuntimeFacts, TriageTabCounts,
};
use super::types::{MetadataProvenance, RawReleaseEdit};
use super::watched_folder::WatchedFolder;
use super::{FileEvidence, ImportFailure, ImportedRelease, WatchedFolderScanStatus};
use crate::db::LibraryStatus;
use crate::identify::{IdentifyState, QueueClassification};
use crate::import::CandidateSession;
use crate::library::{LibraryPageWindow, LibraryPageWindows};
use crate::signals::Signals;
use std::collections::{BTreeMap, BTreeSet};

mod flatten;
mod subscription;

#[cfg(test)]
mod tests;

pub(crate) use flatten::{first_candidate_among, flatten, locate_candidate, Flattened, ItemRef};
pub use subscription::{ImportListSubscription, ImportListSubscriptionError};

pub use super::triage::TriageTab;

/// What the list is currently showing: which tab, which filter, which groups
/// are folded shut, and in which direction.
///
/// The collapsed set is part of the request rather than a rendering decision:
/// a folded group's rows are not in the list at all, so the offsets a window
/// asks for depend on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListView {
    pub tab: TriageTab,
    pub filter_text: String,
    pub collapsed_groups: BTreeSet<FolderReleaseDecisionKey>,
    pub order: ImportListOrder,
}

impl ImportListView {
    /// Whether the filter hides anything — and so whether the queue read
    /// reads the text a Done row shows, which only the filter tests.
    pub(crate) fn filters(&self) -> bool {
        !self.filter_text.is_empty()
    }
}

impl Default for ImportListView {
    fn default() -> Self {
        Self {
            tab: TriageTab::Pending,
            filter_text: String::new(),
            collapsed_groups: BTreeSet::new(),
            order: ImportListOrder::NewestFirst,
        }
    }
}

/// Folder dates or natural folder-path order, never the editable album title.
/// Done uses the library import date and keeps outstanding uploads first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportListOrder {
    NewestFirst,
    OldestFirst,
    PathAscending,
    PathDescending,
}

/// Where an imported release's cloud upload stands, which is the Done tab's
/// outer order: what is moving now, then what is waiting behind it, then what
/// is settled.
///
/// A release with nothing outstanding is absent from the map rather than
/// present and settled: the common case is empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadStanding {
    /// Something is happening to this release's files right now — preparing,
    /// transferring, publishing, unwinding a cancel, or retrying after a
    /// failure. One bucket, because the row draws one arrow for all of them.
    Working,
    /// Admitted to the cloud queue with nothing happening yet.
    Queued,
}

impl UploadStanding {
    /// The Done tab's outer sort key. Settled — no entry at all — sorts last.
    pub(crate) fn rank(standing: Option<Self>) -> u8 {
        match standing {
            Some(Self::Working) => 0,
            Some(Self::Queued) => 1,
            None => 2,
        }
    }

    /// Where each release the cloud outbox still holds work for stands.
    pub fn of_outbox(snapshot: &crate::library::OutboxSnapshot) -> BTreeMap<String, Self> {
        use crate::library::UploadActivity;
        snapshot
            .upload_groups
            .iter()
            .filter_map(|group| {
                let standing = match group.progress.activity()? {
                    UploadActivity::Queued => Self::Queued,
                    UploadActivity::Cancelling
                    | UploadActivity::Publishing
                    | UploadActivity::Uploading
                    | UploadActivity::Preparing
                    | UploadActivity::Retrying
                    | UploadActivity::Prepared
                    | UploadActivity::Uploaded => Self::Working,
                };
                Some((group.release_id.clone(), standing))
            })
            .collect()
    }
}

/// Everything the list query is a function of.
///
/// `upload_standing` is filled in by [`ImportListSubscription`], never by a
/// caller: an outstanding upload moves a Done row within its tab, and whether
/// one is moving or waiting is the upload pipeline's, held in memory rather
/// than in a table this query reads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportListRequest {
    pub view: ImportListView,
    pub windows: LibraryPageWindows,
    /// Only the releases the cloud outbox still holds work for, by release id.
    pub upload_standing: BTreeMap<String, UploadStanding>,
}

/// One item in the list, at one offset.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportListItem {
    GroupHeader {
        group: TriageGroup,
        watched_folder_path: String,
        expanded: bool,
        /// How many entries the group holds in this tab, after the filter.
        entry_count: u32,
    },
    /// A candidate the list places in Pending or Skipped, presented as what
    /// the candidate reads as.
    Candidate {
        row: TriageRow,
        is_group_member: bool,
    },
    /// A candidate the list places in Done, presented as the library release
    /// it became. Never a group member: only Pending rows join a group.
    Imported {
        row: ImportedRow,
    },
    Invalid {
        candidate: InvalidCandidate,
        is_group_member: bool,
    },
}

impl ImportListItem {
    fn candidate_stable_key(candidate_key: &str) -> String {
        format!("candidate:{candidate_key}")
    }

    /// Stable identity for one item. Variant prefixes keep a candidate, a
    /// boundary and a group header at the same folder from sharing view state;
    /// the length prefix makes a two-component key unambiguous.
    pub fn stable_key(&self) -> String {
        match self {
            Self::GroupHeader { group, .. } => format!(
                "group:{}{}{}",
                group.key.watched_folder_path.len(),
                group.key.watched_folder_path,
                group.key.relative_folder_path
            ),
            Self::Candidate { row, .. } => Self::candidate_stable_key(&row.candidate_key),
            Self::Imported { row } => Self::candidate_stable_key(&row.candidate_key),
            Self::Invalid { candidate, .. } => {
                format!("invalid:{}", candidate.path.display())
            }
        }
    }
}

/// The list view and position that reveal one candidate at its current
/// placement.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateListLocation {
    pub stable_key: String,
    pub tab: TriageTab,
    /// The Pending group that has to be open for this candidate to be visible.
    pub group_key: Option<FolderReleaseDecisionKey>,
    pub visible_position: u64,
}

/// One group header the flatten emitted, before it becomes an item.
pub(crate) struct GroupHeaderRow {
    pub(crate) group: TriageGroup,
    pub(crate) watched_folder_path: String,
    pub(crate) expanded: bool,
    pub(crate) entry_count: u32,
}

/// One placed candidate row, and which scanned row it came from.
pub(crate) struct PlacedRow {
    /// The row with `matched` read off the verdict's lead and a reading
    /// naming no records. The window fills both in from what it reads for the rows it materialises — or, for a
    /// row placed Done, reads the library release it became and presents
    /// that instead.
    pub(crate) row: TriageRow,
    /// Index into [`crate::db::ImportQueueRows::candidates`].
    pub(crate) index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportListWindow {
    pub window: LibraryPageWindow,
    pub items: Vec<ImportListItem>,
}

/// One Ready row, as the surfaces that act on the whole Ready set need it: the
/// foot bar's count, select-all, the bulk import's claims, and the covers to
/// decode before Pending opens. Ready as the tables place it: a bulk import
/// checks what is running for each row when it gets to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadyRowRef {
    pub candidate_key: String,
    pub cover_thumbnail_url: Option<String>,
}

/// Everything the chrome around the list shows, computed in the same pass as
/// the items so none of it can drift from them.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    pub counts: TriageTabCounts,
    pub watched_folders: Vec<WatchedFolder>,
    /// Every group header the whole queue has, across all tabs — what
    /// disclosure state is retained against.
    pub group_keys: Vec<FolderReleaseDecisionKey>,
    /// The Ready rows matching the view's filter, in queue order.
    pub ready: Vec<ReadyRowRef>,
}

/// Where each watched folder's scan stands, for the chrome around the list.
///
/// Read apart from the list, and by its own live query: a scan re-confirms
/// every folder it walks by moving its row to the scan's generation, which
/// moves the found count and nothing the list shows. Counted inside the list's
/// read, each of those writes cost a whole-queue read, and a rescan of a large
/// folder kept the list reading back to back until it finished.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderScanProgress {
    /// Every scanned root, in watched-folder order.
    pub statuses: Vec<WatchedFolderScanStatus>,
    /// The current walks, already filtered and totalled for the filter-bar
    /// activity control. Absent as soon as no root is scanning.
    pub activity: Option<FolderScanActivity>,
}

impl FolderScanProgress {
    pub(crate) fn of(statuses: Vec<WatchedFolderScanStatus>) -> Self {
        let folders: Vec<ActiveFolderScan> = statuses
            .iter()
            .filter_map(|folder| match folder.status {
                super::FolderScanStatus::Scanning { found_count } => Some(ActiveFolderScan {
                    watched_folder_path: folder.watched_folder_path.clone(),
                    watched_folder_name: folder.watched_folder_name.clone(),
                    found_count,
                }),
                super::FolderScanStatus::Complete | super::FolderScanStatus::Failed { .. } => None,
            })
            .collect();
        let activity = (!folders.is_empty()).then(|| FolderScanActivity {
            found_count: folders.iter().map(|folder| folder.found_count).sum(),
            folders,
        });
        Self { statuses, activity }
    }
}

/// Live folder-scan activity for the list chrome. Counts come from each
/// root's current generation, never from the list windows a UI has loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderScanActivity {
    pub found_count: u64,
    pub folders: Vec<ActiveFolderScan>,
}

/// One root in [`FolderScanActivity`], in watched-folder order.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveFolderScan {
    pub watched_folder_path: String,
    pub watched_folder_name: String,
    pub found_count: u64,
}

/// One read of the list: the requested windows, the total, and the chrome.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListProjection {
    pub windows: Vec<ImportListWindow>,
    pub total_count: u64,
    pub summary: ImportQueueSummary,
}

/// A projection with the live query's own bookkeeping — which request it
/// answers and what woke it — and where the folder scans stand.
///
/// A change to the scans alone delivers the last projection again beside
/// them, with its revision and a [`coven::ReconfigurableLiveQueryCause::DatabaseChanged`]
/// cause.
#[derive(Debug, Clone)]
pub struct ImportListSnapshot {
    pub windows: Vec<ImportListWindow>,
    pub total_count: u64,
    pub summary: ImportQueueSummary,
    pub folder_scans: FolderScanProgress,
    pub request_revision: u64,
    pub cause: coven::ReconfigurableLiveQueryCause,
}

/// One candidate as the pane reads it, before its runtime is joined.
///
/// The row is built here from what the tables say; a claimed import or a run
/// in flight is joined by [`Self::resolve`] as the detail's live state, beside
/// the row rather than inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateDetailProjection {
    pub candidate: FolderCandidate,
    pub source_error: Option<String>,
    pub actionable: bool,
    pub skipped: bool,
    pub is_added: bool,
    /// The identify state the stored verdict stands back up as — the answer a
    /// row shows when no run is in flight.
    pub resumed_identify_state: IdentifyState,
    /// What the stored verdict classified to. `None` with no stored verdict
    /// for the candidate's current file shape.
    pub answer: Option<QueueClassification>,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// Who wrote the draft, which decides whether a valid one is the answer.
    pub metadata_author: crate::import::MetadataAuthor,
    pub metadata_revision: u64,
    /// The library release this candidate's bytes were imported as.
    pub imported_release: Option<ImportedRelease>,
    /// The picked release as its stored release describes it. `None` with no
    /// pick, and for a folder read as its own tags.
    pub release: Option<ImportSearchReleaseDetail>,
    /// Every catalog the pick's stored releases describe the release in,
    /// in the order surfaces list catalogs. Empty with no pick.
    pub records: Vec<crate::import::ReleaseRecord>,
    /// Whether the picked release is already in the library.
    pub picked_library_status: Option<LibraryStatus>,
    /// The candidate's one editable metadata draft.
    pub metadata_draft: RawReleaseEdit,
    /// What the library holds, as this read found it, for every artist credit
    /// the draft carries.
    pub artist_resolutions: Vec<crate::import::ResolvedCredit>,
    /// Every source unit the folder offers, with the track committing makes of
    /// it. Every audio row awaits a pick until there is one.
    pub mapping: MappingTable,
    /// The cover this candidate commits with: its selection, the picked
    /// release's default, or the folder's default image.
    pub cover: Option<CoverChoice>,
    /// Every cover the picker offers: the picked release's remote art.
    pub remote_covers: Vec<RemoteCover>,
    /// The signals identification settled on, or `None` before it has.
    pub signals: Option<Signals>,
    /// What this candidate's identification asks about, as the person left it.
    pub lookup_choices: crate::import::LookupChoices,
    /// The last import of this candidate that failed.
    pub failure: Option<ImportFailure>,
    /// Where the pane was when the person last left this candidate. `None`
    /// before the pane has been touched.
    pub session: Option<CandidateSession>,
}

impl ImportCandidateDetailProjection {
    /// The pane's session: the stored one, or the one the pane opens on for
    /// a candidate nobody has touched.
    pub fn session_or_initial(&self) -> CandidateSession {
        self.session.clone().unwrap_or_else(|| {
            CandidateSession::initial(self.metadata_provenance.as_ref(), self.answer.is_some())
        })
    }

    /// The pane's value, with this key's runtime joined.
    pub fn resolve(self, facts: &TriageRuntimeFacts) -> ImportCandidateDetail {
        let session = self.session_or_initial();
        let Self {
            candidate,
            source_error,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            answer,
            metadata_provenance,
            metadata_author,
            metadata_revision,
            imported_release,
            release,
            records,
            picked_library_status,
            metadata_draft,
            artist_resolutions,
            mapping,
            cover,
            remote_covers,
            signals,
            lookup_choices,
            failure,
            session: _,
        } = self;
        let file_evidence = signals
            .as_ref()
            .map(crate::import::file_evidence)
            .unwrap_or_default();
        let import_status = import_status_of(
            imported_release.as_ref(),
            source_error
                .as_deref()
                .or_else(|| failure.as_ref().map(|failure| failure.error.as_str())),
        );
        // The attempt running now, or the one that completed, is what the pane
        // shows; an earlier failure is behind either.
        let failure = if facts.importing
            || matches!(
                import_status.as_ref(),
                Some(TriageImportStatus::Complete { .. })
            ) {
            None
        } else {
            failure
        };
        let classification = answer.as_ref().filter(|_| actionable);
        let placement = place(
            skipped,
            is_added,
            import_status.as_ref(),
            metadata_author,
            metadata_draft.clone().shape().is_ok(),
            classification,
        );
        let action_basis = CandidateActionBasis::of(actionable, &placement, classification);
        let live = CandidateLiveState::of(&action_basis, facts.clone());
        // The catalogs the draft was read from, as the row names them: only a
        // draft read from a catalog's release names any.
        let draft_records = || {
            let picked = metadata_provenance.clone().filter(|_| actionable);
            match super::triage::TriageReading::of(
                TriageMetadataSummary::of(&metadata_draft, picked.clone()).as_ref(),
                picked.as_ref(),
                records,
            ) {
                super::triage::TriageReading::Identified { records } => records,
                super::triage::TriageReading::Unidentified
                | super::triage::TriageReading::Prefilled => Vec::new(),
            }
        };
        let pane_placement = match placement.tab() {
            TriageTab::Pending => CandidatePanePlacement::Pending {
                ready_check: super::triage::ready_check(&placement),
                records: draft_records(),
            },
            TriageTab::Skipped => CandidatePanePlacement::Skipped {
                records: draft_records(),
            },
            TriageTab::Done => CandidatePanePlacement::Done,
        };
        let metadata_draft_is_blank = metadata_draft.is_blank();
        let grouping_action = if is_added || facts.importing || facts.identifying() {
            None
        } else if candidate.grouping.is_some() {
            Some(super::grouping::GroupingAction::Separate)
        } else if actionable {
            Some(super::grouping::GroupingAction::Combine)
        } else {
            None
        };
        let import_status = if facts.importing {
            Some(CandidateImportStatus::Importing)
        } else {
            import_status.map(CandidateImportStatus::of)
        };
        ImportCandidateDetail {
            grouping_action,
            candidate,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            placement: pane_placement,
            live,
            import_status,
            release,
            picked_library_status,
            file_evidence,
            metadata_draft,
            artist_resolutions,
            metadata_draft_is_blank,
            metadata_provenance,
            metadata_author,
            metadata_revision,
            mapping,
            cover,
            remote_covers,
            signals,
            lookup_choices,
            failure,
            session,
        }
    }
}

/// Where the queue places the candidate a pane shows, with what the pane
/// states beside it. A Done candidate's pane is the library release it became,
/// so it carries nothing the candidate's draft says: the Ready check and the
/// catalogs the draft was read from are a queued candidate's alone.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidatePanePlacement {
    /// In Pending.
    Pending {
        /// The Ready check the candidate did not pass, stated beside its
        /// Import: [`crate::import::triage::ready_check`] of its placement.
        ready_check: Option<crate::identify::NeedsYou>,
        /// Every catalog the draft was read from, in the order surfaces list
        /// catalogs. Empty for a draft read from the files' tags, typed in,
        /// or not there yet.
        records: Vec<crate::import::ReleaseRecord>,
    },
    /// Skipped, with every catalog the draft was read from, as for Pending.
    Skipped {
        records: Vec<crate::import::ReleaseRecord>,
    },
    /// In the library.
    Done,
}

/// Where a candidate's import stands for the pane that shows the candidate:
/// running now, or the outcome the last one left in the tables. The pane
/// draws a different surface for each, so the three are one value rather
/// than a row's stored outcome beside a live flag.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateImportStatus {
    Importing,
    Complete { release: super::ImportedRelease },
    Error { error: String },
}

impl CandidateImportStatus {
    fn of(stored: TriageImportStatus) -> Self {
        match stored {
            TriageImportStatus::Complete { release } => Self::Complete { release },
            TriageImportStatus::Error { error } => Self::Error { error },
        }
    }
}

/// One candidate, whole: the folder with its files, what the queue makes of
/// it, and the identify state it resumes.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateDetail {
    /// Whether this release can be read together with others as one, or
    /// read as the folders it is made of.
    pub grouping_action: Option<super::grouping::GroupingAction>,
    pub candidate: FolderCandidate,
    pub actionable: bool,
    pub skipped: bool,
    pub is_added: bool,
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
    pub release: Option<ImportSearchReleaseDetail>,
    pub picked_library_status: Option<LibraryStatus>,
    /// Extracted identifying signals pinned to their source files. Independent
    /// of the selected pressing; result support lives in result provenance.
    pub file_evidence: Vec<FileEvidence>,
    pub metadata_draft: RawReleaseEdit,
    /// What the library holds for every artist credit of the draft and the
    /// mapping rows, as the pane's live read found it.
    pub artist_resolutions: Vec<crate::import::ResolvedCredit>,
    pub metadata_draft_is_blank: bool,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// Who wrote the draft: nobody, the tag prefill, identification's own
    /// pick, or the person.
    pub metadata_author: crate::import::MetadataAuthor,
    /// Revision of the exact metadata draft and selected cover in this value.
    pub metadata_revision: u64,
    pub mapping: MappingTable,
    pub cover: Option<CoverChoice>,
    pub remote_covers: Vec<RemoteCover>,
    pub signals: Option<Signals>,
    /// What this candidate's identification asks about: the signals its runs
    /// leave out and the catalog numbers they look up. A control that changes
    /// one sends the whole value back, computed from this.
    pub lookup_choices: crate::import::LookupChoices,
    pub failure: Option<ImportFailure>,
    /// Where the pane was when the person last left this candidate, or where
    /// it opens for one nobody has touched.
    pub session: CandidateSession,
}

/// The item references one window asks for, clamped to what the list holds.
pub(crate) fn window_refs<'a>(items: &'a [ItemRef], window: &LibraryPageWindow) -> &'a [ItemRef] {
    let start = usize::try_from(window.offset)
        .unwrap_or(usize::MAX)
        .min(items.len());
    let end = usize::try_from(window.limit)
        .unwrap_or(usize::MAX)
        .saturating_add(start)
        .min(items.len());
    &items[start..end]
}
