//! The import tab's list: one ordered sequence of items, served a window at a
//! time.
//!
//! Each surface's list is a row of its own (`import_list`), holding its view,
//! the groups it folds shut and the rows it has selected, and what the list
//! shows is a query over that row and the placement the import tables keep
//! (`import_list_placement`): the requested windows, the tab counts, Found's
//! filter entries and what the selection offers all come from one read, so
//! none of it can disagree with the rows. Changing the view, folding a group
//! or selecting rows is a write the list's live query answers; the windows
//! are the query's request.
//!
//! What is running for a candidate right now — a run queued or in flight, an
//! import that owns it — is in the standings table the candidate runtime
//! writes, and the placement's Found state already counts it: each row carries
//! it as its [`CandidateLiveState`], and the selection's offers read it.

use super::cover_art::CoverChoice;
use super::folder_scanner::FolderCandidate;
use super::folder_scanner::{FolderReleaseDecisionKey, InvalidCandidate};
use super::mapping::MappingTable;
use super::triage::{
    import_status_of, stated_folder_check, CandidateLiveState, TriageGroup, TriageImportStatus,
    TriageMetadataSummary, TriagePlacement, TriageRow, TriageRuntimeFacts, TriageTabCounts,
};
use super::types::{MetadataProvenance, RawReleaseEdit, ReleaseLink};
use super::watched_folder::WatchedFolder;
use super::{FileEvidence, ImportFailure, ImportedRelease, WatchedFolderScanStatus};
use crate::db::LibraryStatus;
use crate::identify::{IdentifyState, VerdictSummary};
use crate::import::CandidateSession;
use crate::library::{LibraryPageWindow, LibraryPageWindows};
use crate::signals::Signals;
use bae_catalogs::remote_image::RemoteCover;

mod path_order;
mod pending_filter;
mod subscription;

pub(crate) use path_order::release_keys_in_path_order;
pub(crate) use pending_filter::pending_filter_entries;
pub use pending_filter::{PendingFilter, PendingFilterEntry};
pub use subscription::{ImportListSubscription, ImportListSubscriptionError};

pub use super::triage::TriageTab;

/// What the list is currently showing: which tab, which filters, and in which
/// direction. Which groups it folds shut is the list's too, written on their
/// own ([`ImportListSubscription::set_group_folded`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListView {
    pub tab: TriageTab,
    pub filter_text: String,
    /// Which of Found's rows the list shows. Done and Skipped rows are past
    /// identification, so it leaves them alone.
    pub pending_filter: PendingFilter,
    pub order: ImportListOrder,
}

impl Default for ImportListView {
    fn default() -> Self {
        Self {
            tab: TriageTab::Pending,
            filter_text: String::new(),
            pending_filter: PendingFilter::All,
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

/// One list a surface has open: the id of its `import_list` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImportListId(pub(crate) i64);

/// What one read of a list answers: the list, and the windows of it to load.
/// Everything else the read is a function of is in the tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListRequest {
    pub list: ImportListId,
    pub windows: LibraryPageWindows,
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
    /// A candidate, presented as what the candidate reads as on whichever tab
    /// places it. A Done row is the row it was in Found, its import status
    /// naming the one release this device imported from its files; it is
    /// never a group member, since only Pending rows join a group.
    Candidate {
        row: TriageRow,
        /// What is running for the candidate, as the read found it, and the
        /// commands and state the row has with it.
        live: CandidateLiveState,
        is_group_member: bool,
    },
    /// A folder that failed validation, presented as what the scan read of
    /// it. It offers nothing to do, and nothing runs for it.
    Invalid {
        candidate: InvalidCandidate,
        /// Whether the person has selected the row.
        selected: bool,
        is_group_member: bool,
    },
}

/// The stable key of a candidate's item, whichever tab places it.
pub(crate) fn candidate_stable_key(candidate_key: &str) -> String {
    format!("candidate:{candidate_key}")
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
            Self::Candidate { row, .. } => candidate_stable_key(&row.candidate_key),
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

/// The items one window of the list holds.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListWindow {
    pub window: LibraryPageWindow,
    pub items: Vec<ImportListItem>,
}

/// Everything the chrome around the list shows, read with the items so none
/// of it can drift from them.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportQueueSummary {
    pub counts: TriageTabCounts,
    pub watched_folders: Vec<WatchedFolder>,
    /// Whether Found has any group, which folding every group acts on.
    pub has_groups: bool,
    /// How many of the tab's entries the list shows, of how many it has,
    /// while the text filter or the filter entry narrows it; `None` while
    /// nothing does.
    pub narrowed: Option<NarrowedCount>,
    /// What the view asked the rows to be narrowed by when this read was
    /// made: what `narrowed` counts under, so a surface shows the count only
    /// beside the filter it is for.
    pub narrowing: ImportListNarrowing,
    /// Where the list's view opened: the position its first selected row had
    /// in it when the view was set, or its top (0) while it showed no
    /// selected row. What a list narrowed anew shows on top.
    pub opened_at: u64,
}

/// What a view narrows its rows by: the tab, the text filter and the filter
/// entry, as the view states them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportListNarrowing {
    pub tab: TriageTab,
    pub filter_text: String,
    pub pending_filter: PendingFilter,
}

/// How many of a tab's entries a narrowed list shows, of how many it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NarrowedCount {
    pub shown: u32,
    pub total: u32,
}

/// Where each watched folder's scan stands, for the chrome around the list.
///
/// Read apart from the list, by its own live query, and sent on its own
/// subscription: a scan re-confirms every folder it walks by moving its row
/// to the scan's generation, which moves the found count and nothing the list
/// shows. Counted inside the list's read, each of those writes cost a
/// whole-queue read, and a rescan of a large folder kept the list reading back
/// to back until it finished; sent with the list, each resent its page.
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

/// One read of the list: the requested windows, the total, and the chrome, as
/// the tables hold them.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportListProjection {
    pub windows: Vec<ImportListWindow>,
    pub total_count: u64,
    pub summary: ImportQueueSummary,
    /// Every entry of Found's filter, in the menu's order, with how many of
    /// Found's rows it holds, whatever the view shows.
    pub pending_filter_entries: Vec<PendingFilterEntry>,
    /// What the selection holds and can be told to do, with what is running
    /// for each selected row.
    pub selection: crate::import::selection::SelectionSummary,
    /// The changes to the list's view, its folds included, this read
    /// reflects.
    pub view_revision: u64,
    /// The changes to the list's selection this read reflects: the person's,
    /// and those that took out a row the list stopped showing.
    pub selection_revision: u64,
}

/// A projection, and what woke the live query.
#[derive(Debug, Clone)]
pub struct ImportListSnapshot {
    pub windows: Vec<ImportListWindow>,
    pub total_count: u64,
    pub summary: ImportQueueSummary,
    /// Every entry of Found's filter, in the menu's order, with how many of
    /// Found's rows it holds.
    pub pending_filter_entries: Vec<PendingFilterEntry>,
    /// The changes to the selection the rows' `selected` reflects.
    pub selection_revision: u64,
    /// What the selection holds and can be told to do: the selected rows
    /// this snapshot shows, with what is running for them.
    pub selection: crate::import::selection::SelectionSummary,
    /// The changes to the view, its folds included, the rows reflect.
    pub view_revision: u64,
    pub cause: coven::LiveQueryCause,
}

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
    pub is_added: bool,
    /// The identify state the stored verdict stands back up as — the answer a
    /// row shows when no run is in flight.
    pub resumed_identify_state: IdentifyState,
    /// The stored verdict's columns, as the list reads them. `None` with no
    /// stored verdict for the candidate's current file shape.
    pub verdict: Option<VerdictSummary>,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// What the candidate is linked to in the catalogs.
    pub release_link: Option<ReleaseLink>,
    /// Who wrote the draft, which decides whether a valid one is the answer.
    pub metadata_author: crate::import::MetadataAuthor,
    pub metadata_revision: u64,
    /// The release this device imported from this candidate's files.
    pub imported: Option<ImportedRelease>,
    /// Every catalog the linked releases describe the release in, in the
    /// order surfaces list catalogs. Empty with no link.
    pub records: Vec<crate::import::ReleaseRecord>,
    /// Whether the linked release is already in the library.
    pub picked_library_status: Option<LibraryStatus>,
    /// The library album an import of it files under.
    pub album: crate::import::DraftAlbum,
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
    /// The covers the picker offers from the catalogs: what the applied
    /// release offered, stored with the draft when it was applied.
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
    /// What is running for the candidate, from its standing.
    pub standing: TriageRuntimeFacts,
    /// Where the list places the candidate, as its row in the list's
    /// placement says.
    pub listed: ListedRelease,
}

/// Where the list places one release, as its row in the list's placement
/// (`import_list_placement`) holds it: what its commands are decided from,
/// and where it stands among Found's states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedRelease {
    pub basis: crate::import::CandidateActionBasis,
    /// Where it stands among Found's states, what is running for it
    /// included; `None` off Found.
    pub standing: Option<crate::import::PendingStanding>,
}

impl ImportCandidateDetailProjection {
    /// The pane's session: the stored one, or the one the pane opens on for
    /// a candidate nobody has touched.
    pub fn session_or_initial(&self) -> CandidateSession {
        self.session.clone().unwrap_or_else(|| {
            CandidateSession::initial(
                self.metadata_provenance.is_some() || self.release_link.is_some(),
                self.verdict.is_some(),
            )
        })
    }

    /// The pane's value: the candidate with the commands and state its
    /// standing gives it.
    pub fn resolve(self) -> ImportCandidateDetail {
        let session = self.session_or_initial();
        let Self {
            candidate,
            source_error,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            verdict,
            metadata_provenance,
            release_link,
            metadata_author,
            metadata_revision,
            imported,
            records,
            picked_library_status,
            album,
            metadata_draft,
            artist_resolutions,
            mapping,
            cover,
            remote_covers,
            signals,
            lookup_choices,
            failure,
            session: _,
            standing: facts,
            listed,
        } = self;
        let import_standing = facts.import;
        let importing = facts.importing();
        let file_evidence = signals
            .as_ref()
            .map(crate::import::file_evidence)
            .unwrap_or_default();
        let import_status = import_status_of(
            imported.as_ref(),
            source_error.as_ref(),
            failure.as_ref().map(|failure| &failure.reason),
        );
        // The attempt running now, or the one that completed, is what the pane
        // shows; an earlier failure is behind either.
        let failure = if importing
            || matches!(
                import_status.as_ref(),
                Some(TriageImportStatus::Complete { .. })
            ) {
            None
        } else {
            failure
        };
        let verdict = verdict.as_ref().filter(|_| actionable);
        let draft_valid = metadata_draft.clone().shape().is_ok();
        let placement = listed.basis.placement;
        let live = CandidateLiveState::of(&listed.basis, facts, listed.standing);
        // The catalogs the linked release is described in, as the row names
        // them: only a linked candidate names any.
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
            TriageTab::Skipped => CandidatePanePlacement::Skipped {
                records: draft_records(),
            },
            TriageTab::Done => CandidatePanePlacement::Done,
        };
        let metadata_draft_is_blank = metadata_draft.is_blank();
        let import_status = match import_standing {
            Some(standing) => Some(CandidateImportStatus::Importing { standing }),
            None => import_status.map(CandidateImportStatus::of),
        };
        ImportCandidateDetail {
            candidate,
            actionable,
            skipped,
            is_added,
            resumed_identify_state,
            placement: pane_placement,
            live,
            import_status,
            picked_library_status,
            album,
            file_evidence,
            metadata_draft,
            artist_resolutions,
            metadata_draft_is_blank,
            metadata_provenance,
            release_link,
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
/// so it carries nothing the candidate's draft says: the folder check and the
/// catalogs of the linked release are a queued candidate's alone.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidatePanePlacement {
    /// In Pending.
    Pending {
        /// The check against the folder the candidate's found release did
        /// not pass, stated beside its Import: the release's
        /// [`crate::identify::VerdictSummary::folder_check`].
        folder_check: Option<crate::identify::FolderCheck>,
        /// Every catalog the linked release is described in, in the order
        /// surfaces list catalogs. Empty for a candidate linked to no release.
        records: Vec<crate::import::ReleaseRecord>,
    },
    /// Skipped, with every catalog the linked release is described in, as
    /// for Pending.
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
    Importing {
        standing: super::ImportStanding,
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
/// it, and the identify state it resumes.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportCandidateDetail {
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
    pub picked_library_status: Option<LibraryStatus>,
    /// The library album an import of it files under.
    pub album: crate::import::DraftAlbum,
    /// Extracted identifying signals pinned to their source files. Independent
    /// of the selected pressing; result support lives in result provenance.
    pub file_evidence: Vec<FileEvidence>,
    pub metadata_draft: RawReleaseEdit,
    /// What the library holds for every artist credit of the draft and the
    /// mapping rows, as the pane's live read found it.
    pub artist_resolutions: Vec<crate::import::ResolvedCredit>,
    pub metadata_draft_is_blank: bool,
    pub metadata_provenance: Option<MetadataProvenance>,
    /// What the candidate is linked to in the catalogs, which Unlink clears.
    pub release_link: Option<ReleaseLink>,
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
