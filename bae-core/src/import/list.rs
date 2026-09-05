//! The import tab's list: one ordered sequence of items, served a window at a
//! time.
//!
//! The tab used to be one whole-queue value — every candidate with its files,
//! its cue sheets, its boundary trees and every archived document — rebuilt on
//! every commit and carried across the bridge twice. What the list actually
//! shows is a placement per row and a handful of columns, so that is what the
//! read gathers: [`crate::db::ImportQueueRows`] is placement columns and
//! nothing else, `flatten` turns them plus the requested view into an
//! ordered vector of item references, and only the references inside the
//! requested windows are turned into items.
//!
//! Everything the chrome around the list shows — the tab counts, the Ready
//! rows a bulk import acts on, the group keys disclosure state is retained
//! against, the row the identify count is still waiting on — is computed in
//! that same pass, so none of it can disagree with the rows.

use super::cover_art::{CoverChoice, RemoteCover};
use super::folder_registry::WatchedFolder;
use super::folder_scanner::{FolderCandidate, FolderReleaseDecisionKey, InvalidCandidate};
use super::mapping::MappingTable;
use super::search::ImportSearchReleaseDetail;
use super::triage::{
    import_status_of, place, CandidateAnswer, MatchedRelease, TriageGroup, TriageImportStatus,
    TriageMetadataSummary, TriagePlacement, TriageRow, TriageRuntimeFacts, TriageTabCounts,
};
use super::types::{MetadataProvenance, RawReleaseEdit};
use super::{FileEvidence, ImportFailure, ImportedRelease, WatchedFolderScanStatus};
use crate::db::LibraryStatus;
use crate::identify::{IdentifyState, QueueClassification};
use crate::library::{LibraryPageWindow, LibraryPageWindows};
use crate::signals::Signals;
use std::collections::{BTreeMap, BTreeSet};

mod flatten;
mod subscription;

#[cfg(test)]
mod tests;

pub(crate) use flatten::{flatten, locate_candidate, Flattened, ItemRef};
pub(crate) use subscription::facts_of;
pub use subscription::{ImportListSubscription, ImportListSubscriptionError};

pub use super::triage::TriageTab;

/// What the list is currently showing: which tab, which filter, which groups
/// are folded shut, and in which direction.
///
/// The collapsed set is part of the request rather than a rendering decision:
/// a folded group's rows are not in the list at all, so the offsets a window
/// asks for depend on it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportListView {
    pub tab: TriageTab,
    pub filter_text: String,
    pub collapsed_groups: BTreeSet<FolderReleaseDecisionKey>,
    pub order: ImportListOrder,
}

/// The two orders the list offers, over the folder's path below its watched
/// root.
///
/// Not over the row's title: for a candidate the user picked a release for,
/// the title lives in an archived document, and ordering by it would mean
/// decoding every pick on every rerun. The path is on the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImportListOrder {
    #[default]
    PathAscending,
    PathDescending,
}

/// Where an imported release's cloud upload stands, which is the Done tab's
/// outer order: what is moving now, then what is waiting behind it, then what
/// is settled.
///
/// A release with nothing outstanding is absent from the map rather than
/// present and settled — the same shape `runtime_facts` uses, and for the same
/// reason: the common case is empty.
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
/// `runtime_facts` and `upload_standing` are filled in by
/// [`ImportListSubscription`], never by a caller: a claimed import and a
/// running identification move a row between tabs, an outstanding upload moves
/// a Done row within its tab. Neither is in a table this query reads.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportListRequest {
    pub view: ImportListView,
    pub windows: LibraryPageWindows,
    /// Only the keys whose facts differ from the default, so an idle queue
    /// makes an empty map.
    pub runtime_facts: BTreeMap<String, TriageRuntimeFacts>,
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
    Candidate {
        row: TriageRow,
        is_group_member: bool,
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
    /// The row with `resolved_boundaries` empty and `matched` read off the
    /// verdict's lead. The window fills both in.
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
/// decode before Pending opens.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadyRowRef {
    pub candidate_key: String,
    pub cover_thumbnail_url: Option<String>,
}

/// The first candidate identification has not settled yet, and where that row
/// is in the requested view when the view includes it.
#[derive(Debug, Clone, PartialEq)]
pub struct FirstUnidentifiedRowRef {
    pub candidate_key: String,
    /// Stable identity of the candidate row in the paginated list.
    pub stable_key: String,
    /// The Pending group that must be open for the candidate to be visible.
    pub group_key: Option<FolderReleaseDecisionKey>,
    /// Its position in this projection's active tab/filter/disclosure view.
    /// Absent when that view does not contain the candidate.
    pub visible_position: Option<u64>,
}

/// Everything the chrome around the list shows, computed in the same pass as
/// the items so none of it can drift from them.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    pub counts: TriageTabCounts,
    pub watched_folders: Vec<WatchedFolder>,
    pub folder_scan_statuses: Vec<WatchedFolderScanStatus>,
    /// The current walks, already filtered and totalled for the filter-bar
    /// activity control. Absent as soon as no root is scanning.
    pub folder_scan_activity: Option<FolderScanActivity>,
    /// Every group header the whole queue has, across all tabs — what
    /// disclosure state is retained against.
    pub group_keys: Vec<FolderReleaseDecisionKey>,
    /// The Ready rows matching the view's filter, in queue order.
    pub ready: Vec<ReadyRowRef>,
    /// The first row the identify count is still waiting on, unfiltered, plus
    /// its position when the current view contains it.
    pub first_unidentified: Option<FirstUnidentifiedRowRef>,
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
/// answers and what woke it.
#[derive(Debug, Clone)]
pub struct ImportListSnapshot {
    pub windows: Vec<ImportListWindow>,
    pub total_count: u64,
    pub summary: ImportQueueSummary,
    pub request_revision: u64,
    pub cause: coven::ReconfigurableLiveQueryCause,
}

/// One candidate as the pane reads it, before its runtime is folded in.
///
/// The row is built here with the placement the tables alone imply; a claimed
/// import or a run in flight is applied by [`Self::resolve`], which is where
/// the runtime this process holds joins what the tables say.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateDetailProjection {
    pub candidate: FolderCandidate,
    pub actionable: bool,
    pub skipped: bool,
    pub is_added: bool,
    /// The identify state the stored verdict stands back up as — the answer a
    /// row shows when no run is in flight.
    pub resumed_identify_state: IdentifyState,
    /// What the stored verdict classified to. `None` with no stored verdict
    /// for the candidate's current file shape.
    pub answer: Option<QueueClassification>,
    /// The identity the row leads with: the pick's archived documents where
    /// there is a pick, the verdict's lead otherwise.
    pub matched: Option<MatchedRelease>,
    pub metadata_provenance: Option<MetadataProvenance>,
    pub metadata_revision: u64,
    /// Source policy captured when this candidate was first discovered.
    pub initial_metadata_source: crate::config::DefaultImportMetadataSource,
    /// The library release this candidate's bytes were imported as.
    pub imported_release: Option<ImportedRelease>,
    /// The picked release as its archived documents describe it. `None` with
    /// no pick, and for a folder read as its own tags.
    pub release: Option<ImportSearchReleaseDetail>,
    /// Whether the picked release is already in the library.
    pub picked_library_status: Option<LibraryStatus>,
    /// The candidate's one editable metadata draft.
    pub metadata_draft: RawReleaseEdit,
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
    /// The last import of this candidate that failed.
    pub failure: Option<ImportFailure>,
}

impl ImportCandidateDetailProjection {
    /// The pane's value, with this key's runtime applied.
    pub fn resolve(self, facts: &TriageRuntimeFacts) -> ImportCandidateDetail {
        let Self {
            candidate,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            answer,
            matched,
            metadata_provenance,
            metadata_revision,
            initial_metadata_source,
            imported_release,
            release,
            picked_library_status,
            metadata_draft,
            mapping,
            cover,
            remote_covers,
            signals,
            failure,
        } = self;
        let file_evidence = signals
            .as_ref()
            .map(crate::import::file_evidence)
            .unwrap_or_default();
        let import_status = import_status_of(
            facts.importing,
            imported_release.as_ref(),
            failure.as_ref().map(|failure| failure.error.as_str()),
        );
        let failure = if matches!(
            import_status.as_ref(),
            Some(TriageImportStatus::Importing | TriageImportStatus::Complete { .. })
        ) {
            None
        } else {
            failure
        };
        let known = match (answer.filter(|_| actionable), facts.identify_phase) {
            (Some(classification), _) => CandidateAnswer::Classified(classification),
            (None, Some(phase)) => CandidateAnswer::Unanswered(phase),
            (None, None) => CandidateAnswer::Idle,
        };
        let metadata_draft_valid = metadata_draft.clone().shape().is_ok();
        let placement = place(
            skipped,
            is_added,
            import_status.as_ref(),
            metadata_provenance.as_ref().filter(|_| actionable),
            metadata_draft_valid,
            &known,
        );
        let row = TriageRow {
            candidate_key: candidate.path.to_string_lossy().into_owned(),
            folder_name: candidate.name.clone(),
            watched_folder_path: candidate.watched_folder_path.clone(),
            display_path: candidate.display_path.clone(),
            resolved_boundaries: candidate.resolved_boundaries.clone(),
            combine_ancestor_key: candidate.combine_ancestor_key.clone(),
            actionable,
            skip_action: actionable.then(|| placement.skip_action()).flatten(),
            selectable: actionable && matches!(placement, TriagePlacement::Ready),
            matched: matched.filter(|_| actionable),
            metadata_summary: TriageMetadataSummary::of(
                &metadata_draft,
                metadata_provenance.clone().filter(|_| actionable),
            ),
            cover_thumbnail: None,
            placement,
            import_status,
            metadata_provenance: metadata_provenance.clone().filter(|_| actionable),
        };
        let metadata_draft_is_blank = metadata_draft.is_blank();
        ImportCandidateDetail {
            candidate,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            row,
            release,
            picked_library_status,
            file_evidence,
            metadata_draft,
            metadata_draft_is_blank,
            metadata_provenance,
            metadata_revision,
            initial_metadata_source,
            mapping,
            cover,
            remote_covers,
            signals,
            failure,
        }
    }
}

/// One candidate, whole: the folder with its files, what the queue makes of
/// it, and the identify state it resumes.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateDetail {
    pub candidate: FolderCandidate,
    pub actionable: bool,
    pub skipped: bool,
    pub is_added: bool,
    pub resumed_identify_state: IdentifyState,
    pub row: TriageRow,
    pub release: Option<ImportSearchReleaseDetail>,
    pub picked_library_status: Option<LibraryStatus>,
    /// Extracted identifying signals pinned to their source files. Independent
    /// of the selected pressing; result support lives in result provenance.
    pub file_evidence: Vec<FileEvidence>,
    pub metadata_draft: RawReleaseEdit,
    pub metadata_draft_is_blank: bool,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// Revision of the exact metadata draft and selected cover in this value.
    pub metadata_revision: u64,
    pub initial_metadata_source: crate::config::DefaultImportMetadataSource,
    pub mapping: MappingTable,
    pub cover: Option<CoverChoice>,
    pub remote_covers: Vec<RemoteCover>,
    pub signals: Option<Signals>,
    pub failure: Option<ImportFailure>,
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
