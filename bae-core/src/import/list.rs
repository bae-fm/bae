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
use super::folder_scanner::{
    FolderCandidate, FolderReleaseBoundary, FolderReleaseDecisionKey, InvalidCandidate,
};
use super::mapping::MappingTable;
use super::search::ImportSearchReleaseDetail;
use super::triage::{
    import_status_of, place, CandidateAnswer, MatchedRelease, TriageGroup, TriagePlacement,
    TriageRow, TriageRuntimeFacts, TriageTabCounts,
};
use super::types::{AudioFile, IdentityChoice, IdentityPick, RawReleaseEdit};
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

pub(crate) use flatten::{flatten, Flattened, ItemRef};
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

/// Everything the list query is a function of.
///
/// `runtime_facts` is filled in by [`ImportListSubscription`] from the
/// candidate runtime, never by a caller: a claimed import and a running
/// identification move a row between tabs, and neither is in a table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImportListRequest {
    pub view: ImportListView,
    pub windows: LibraryPageWindows,
    /// Only the keys whose facts differ from the default, so an idle queue
    /// makes an empty map.
    pub runtime_facts: BTreeMap<String, TriageRuntimeFacts>,
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
    Candidate(TriageRow),
    Boundary(FolderReleaseBoundary),
    Invalid(InvalidCandidate),
}

impl ImportListItem {
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
            Self::Candidate(row) => format!("candidate:{}", row.candidate_key),
            Self::Boundary(boundary) => format!(
                "boundary:{}:{}{}",
                boundary.key.watched_folder_path.len(),
                boundary.key.watched_folder_path,
                boundary.key.relative_folder_path
            ),
            Self::Invalid(candidate) => format!("invalid:{}", candidate.path.display()),
        }
    }
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
    /// The stored decision in the shape commit takes — a bulk import has no
    /// pane to read a claim line off.
    pub claim: IdentityChoice,
    pub cover_thumbnail_url: Option<String>,
}

/// Everything the chrome around the list shows, computed in the same pass as
/// the items so none of it can drift from them.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    pub counts: TriageTabCounts,
    pub watched_folders: Vec<WatchedFolder>,
    pub folder_scan_statuses: Vec<WatchedFolderScanStatus>,
    /// Every group header the whole queue has, across all tabs — what
    /// disclosure state is retained against.
    pub group_keys: Vec<FolderReleaseDecisionKey>,
    /// The Ready rows matching the view's filter, in queue order.
    pub ready: Vec<ReadyRowRef>,
    /// The first row the identify count is still waiting on, unfiltered.
    pub first_unidentified_key: Option<String>,
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
    pub picked: Option<IdentityPick>,
    /// The library release this candidate's bytes were imported as.
    pub imported_release: Option<ImportedRelease>,
    /// The picked release as its archived documents describe it. `None` with
    /// no pick, and for a folder read as its own tags.
    pub release: Option<ImportSearchReleaseDetail>,
    /// Whether the picked release is already in the library.
    pub picked_library_status: Option<LibraryStatus>,
    /// The metadata form: the pick's seed with the stored per-field overlay
    /// applied. `None` with no pick — there is nothing to edit yet.
    pub edit: Option<RawReleaseEdit>,
    /// Every source unit the folder offers, with the track committing makes of
    /// it. Every audio row awaits a pick until there is one.
    pub mapping: MappingTable,
    /// Audio units nothing has measured. Non-empty means the pane owes a probe
    /// — the duration cells for these rows have no number to show yet.
    pub unprobed: Vec<AudioFile>,
    /// The cover this candidate commits with: the one chosen, else the picked
    /// release's default.
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
    ///
    /// `live_identify` is the run in flight for this key, `Idle` when none is.
    /// It decides the evidence badge: a run that just found the release by
    /// disc ID says so before its verdict is stored, and after a restart the
    /// resumed verdict says the same thing.
    pub fn resolve(
        self,
        facts: &TriageRuntimeFacts,
        live_identify: &IdentifyState,
    ) -> ImportCandidateDetail {
        let Self {
            candidate,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            answer,
            matched,
            picked,
            imported_release,
            release,
            picked_library_status,
            edit,
            mapping,
            unprobed,
            cover,
            remote_covers,
            signals,
            failure,
        } = self;
        let file_evidence = match (picked.as_ref(), signals.as_ref()) {
            (
                Some(IdentityPick::Release {
                    source, release_id, ..
                }),
                Some(signals),
            ) => {
                let state = match live_identify {
                    IdentifyState::Idle => &resumed_identify_state,
                    live => live,
                };
                crate::import::claim::file_evidence(
                    state,
                    &crate::import::MetadataRef::new(release_id.clone(), *source),
                    signals,
                )
            }
            _ => Vec::new(),
        };
        let import_status = import_status_of(
            facts.importing,
            imported_release.as_ref(),
            failure.as_ref().map(|failure| failure.error.as_str()),
        );
        let known = match answer.filter(|_| actionable) {
            Some(classification) => CandidateAnswer::Classified(classification),
            None => CandidateAnswer::Unanswered(facts.phase),
        };
        let placement = place(
            skipped,
            is_added,
            import_status.as_ref(),
            picked.as_ref().filter(|_| actionable),
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
            placement,
            import_status,
            claim: picked
                .as_ref()
                .filter(|_| actionable)
                .map(IdentityPick::choice),
            picked: picked.filter(|_| actionable),
        };
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
            edit,
            mapping,
            unprobed,
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
    /// What identified the picked release, pinned to the candidate file each
    /// piece of evidence was read off — the chip that file's gallery tile or
    /// table row carries. Empty with no pick, for a folder read as its own
    /// tags, and where nothing identification matched on came from a file.
    pub file_evidence: Vec<FileEvidence>,
    pub edit: Option<RawReleaseEdit>,
    pub mapping: MappingTable,
    pub unprobed: Vec<AudioFile>,
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
