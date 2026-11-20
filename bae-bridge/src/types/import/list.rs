//! The import tab's list as it crosses: one window of items at a time, plus
//! the chrome around them.
//!
//! Mirrors `bae_import::list` field for field and decides nothing. Which
//! items exist, in what order, under which header and in which tab is core's;
//! a UI iterates a window and formats it for its locale.

use super::super::*;

/// Folder dates or natural folder-path order. Done dates are import dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportListOrder {
    NewestFirst,
    OldestFirst,
    PathAscending,
    PathDescending,
}

/// What the list is showing. A change to any of it changes which items sit at
/// which offsets, so it travels as one value.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeImportListView {
    pub tab: BridgeTriageTab,
    pub filter_text: String,
    /// Which of Found's rows the list shows.
    pub pending_filter: BridgePendingFilter,
    pub order: BridgeImportListOrder,
}

/// What every read of the import list takes: the view the app shows and the
/// groups it folds shut. The app keeps both, and a change to either is a new
/// read under a new request.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeImportListRequest {
    pub view: BridgeImportListView,
    /// Which groups the list folds shut: their members are not in it.
    pub folds: BridgeImportListFolds,
}

/// Which of Found's groups a list folds shut. Mirrors
/// `bae_import::ImportListFolds`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportListFolds {
    /// The list folds these groups shut and leaves every other group open.
    Only { groups: Vec<BridgeFolderReleaseDecisionKey> },
    /// The list folds every group shut but these, the groups Found gains
    /// later included: what folding every group leaves.
    AllBut { groups: Vec<BridgeFolderReleaseDecisionKey> },
}

/// One entry of Found's filter menu: which of Found's rows the list shows.
/// Mirrors `bae_import::PendingFilter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgePendingFilter {
    /// Every row.
    All,
    /// Every row waiting on the person: the lookup left the answer to them,
    /// or a catalog, bae itself or the import failed.
    NeedsYou,
    /// A run or an import is queued or going.
    InProgress,
    Identified,
    Unmatched,
    NotLookedUp,
}

/// One entry of Found's filter menu and how many of Found's rows it holds, as
/// the snapshot carrying it was joined. An entry holding none cannot be chosen; one
/// already chosen stays chosen. Mirrors `bae_import::PendingFilterEntry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgePendingFilterEntry {
    pub filter: BridgePendingFilter,
    pub count: u32,
    pub selectable: bool,
}

/// A group's header: the group, how the list shows it, and how many of its
/// entries the list holds. `stable_key` is its item's.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportGroupHeader {
    pub stable_key: String,
    pub group: BridgeTriageGroup,
    pub watched_folder_path: Vec<u8>,
    pub expanded: bool,
    /// How many entries the group holds in this tab, after the filter.
    pub entry_count: u32,
    /// Whether the list shows no member under the header, so the header's
    /// row is the last row the list shows of its group: the list folds the
    /// group shut, or keeps none of its releases.
    pub shows_no_member: bool,
}

/// A release's place in the group it is listed under: the group's header,
/// and whether the release is the last member the list shows of the group.
/// Mirrors `bae_import::ImportGroupMember`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportGroupMember {
    pub header: BridgeImportGroupHeader,
    pub last: bool,
}

/// One item at one offset. `stable_key` identifies it across reruns — the id a
/// paged list holds positions by.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImportListItem {
    GroupHeader {
        header: BridgeImportGroupHeader,
    },
    /// A candidate's row, presented as what the candidate reads as on
    /// whichever tab places it. A Done row is the row it was in Found, its
    /// import status naming the one release this device imported from its
    /// files; it is never a group member, since only Pending rows join a group.
    Candidate {
        stable_key: String,
        row: BridgeTriageRow,
        /// The group it is listed under, for a member of a group, whether or
        /// not the window holds the header's own row.
        group: Option<BridgeImportGroupMember>,
    },
    /// A folder that failed validation, presented as what the scan read of
    /// it.
    Invalid {
        stable_key: String,
        invalid_candidate: BridgeInvalidCandidate,
    },
}

/// What revealing a candidate shows: the request on the tab its placement
/// puts it on, with the filters cleared, its group opened and the order and
/// the other folds of the list it was revealed from kept, and where the
/// candidate sits in that list. Mirrors `bae_import::ImportListReveal`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportListReveal {
    pub request: BridgeImportListRequest,
    pub position: u64,
}

/// Where a list opens under a new request. Mirrors
/// `bae_import::ImportListOpening`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImportListOpening {
    /// The list's top: another tab, filter or search shows none of the
    /// selected rows.
    Top,
    /// The first selected row another tab, filter or search shows: the
    /// position the row sits at, where the list opens.
    At { position: u64 },
    /// Where the person is in the list: another order or another fold moves
    /// rows within the same view, so the list keeps the row on top on top.
    InPlace,
}

/// What showing a list under a new request comes to. Mirrors
/// `bae_import::ImportListViewChange`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportListViewChange {
    pub opening: BridgeImportListOpening,
    /// The keys of the selected rows that leave the selection: on another
    /// tab, filter or search, each selected row the new view doesn't show;
    /// on another fold, each selected row the fold hid; and none on another
    /// order.
    pub left: Vec<Vec<u8>>,
}

/// What the chrome around the list shows beside its rows and counts. Where
/// the folder scans stand is [`BridgeFolderScans`], sent on its own: a scan's
/// progress moves no row. How many entries each tab holds is a stream of its
/// own, the tab counts.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportQueueSummary {
    /// Whether bae watches any folder: with none, the screen shows no list.
    pub has_watched_folders: bool,
    /// Whether Found has any group, which folding every group acts on.
    pub has_groups: bool,
}

/// How many of a tab's entries a narrowed list shows, of how many it has.
/// Mirrors `bae_import::NarrowedCount`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeNarrowedCount {
    pub shown: u32,
    pub total: u32,
}

/// How many entries each import tab holds. Mirrors
/// `bae_import::TriageTabCounts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTriageTabCounts {
    pub pending: u32,
    pub done: u32,
    pub skipped: u32,
}

/// How far the identifications and imports in flight have got, whoever
/// started them. Mirrors `bae_import::ImportCounts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeImportCounts {
    /// How many identifications of the batch in flight have ended, of
    /// `admitted`; both zero when no batch is in flight. A view must not
    /// count its filtered rows instead.
    pub identified: u32,
    pub admitted: u32,
    /// How many imports are waiting or running.
    pub imports_in_flight: u32,
}

/// Where each watched folder's scan stands. Mirrors
/// `bae_import::FolderScanProgress`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderScans {
    /// Every watched root, in watched-folder order.
    pub statuses: Vec<BridgeWatchedFolderScanStatus>,
    /// Whether any root's last scan failed or left it not watched.
    pub has_problem: bool,
    /// Whether bae is scanning any root, for the filter bar's scan
    /// indicator.
    pub scanning: bool,
    /// Each root's problem that is not among the alerts the app raised, in
    /// watched-folder order. The app raises each one and adds it to the
    /// alerts it raised (`FolderScansSubscription::set_raised`).
    pub alerts: Vec<BridgeFolderScanAlert>,
}

/// A watched folder's problem, which the app raises an alert for once.
/// Mirrors `bae_import::FolderScanAlert`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeFolderScanAlert {
    pub folder: BridgeWatchedFolder,
    pub problem: BridgeFolderScanProblem,
}

/// What is wrong with a watched folder. Mirrors
/// `bae_import::FolderScanProblem`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeFolderScanProblem {
    /// The folder's scan failed, for this error, untranslated.
    Failed { error: String },
    /// The scan read the whole folder, and bae could not watch a directory
    /// in it.
    NotWatched { reason: BridgeUnwatchedReason },
}

/// A root bae is scanning, with how many folders its scan has stored so far,
/// as the scan indicator lists it. Mirrors `bae_import::ScanningFolder`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeScanningFolder {
    pub folder: BridgeWatchedFolder,
    /// How many folders the scan has stored so far.
    pub found: u64,
}

/// One candidate as the pane reads it, whole: the folder with its files, what
/// the queue makes of it, the identify state it resumes, and its draft — the
/// picked release, the metadata form, the mapping table, the cover, and the
/// failure its last import left — or the release this device imported from
/// it.
///
/// The pane holds no copy of any of it. Every control writes, and the next
/// value of this record is what redraws.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportCandidateDetail {
    pub candidate: BridgeFolderCandidate,
    /// How many folders the candidate's files come from, as SQL counts them,
    /// when they come from several; 0 when they come from one folder, and
    /// then `candidate.parts` is empty.
    pub part_count: u64,
    pub actionable: bool,
    /// Where the queue places the candidate, with what the pane states beside
    /// it.
    pub placement: BridgeCandidatePanePlacement,
    /// What is running for the candidate right now, and the commands its row
    /// offers with it.
    pub live: BridgeCandidateLiveState,
    /// Where the candidate's import stands for the pane: the one running now,
    /// or what the last one left in the tables.
    pub import_status: Option<BridgeCandidateImportStatus>,
    /// The signals identification settled on, or `None` before it has.
    pub signals: Option<BridgeSignals>,
    pub stage: BridgeCandidateStage,
}

/// What a candidate's pane draws besides its folder: the draft an import
/// would write, or, once this device imported the candidate's files, the
/// library release it became, the candidate holding no draft.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCandidateStage {
    Drafting {
        draft: BridgeCandidateDraftPane,
    },
    Imported {
        release: BridgeImportedRelease,
        /// The folder's images, read from its scan: the gallery beside the
        /// library release, whose editor lists the release's files; `None`
        /// when the folder holds no image.
        images: Option<Vec<crate::types::BridgeMappingImage>>,
    },
}

/// A candidate's draft and what goes with it, as its pane draws them.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateDraftPane {
    /// Whether the picked release is already in the library.
    pub picked_library_status: Option<BridgeLibraryStatus>,
    /// The library album an import of it files under.
    pub album: BridgeDraftAlbum,
    /// The candidate's one editable metadata draft.
    pub metadata_draft: BridgeRawReleaseEdit,
    /// The media and Discogs details the draft's pressing Add menus offer.
    pub pressing_offers: crate::types::BridgePressingOffers,
    /// The catalog number the draft's one-line summary shows, or `None` when
    /// no label row has one.
    pub summary_catalog_number: Option<String>,
    /// The draft's album artists, each with how it stands to the library, as
    /// the pane's live read found them; each mapping row carries its track's
    /// own. `None` when the draft names no album artist.
    pub album_artists: Option<crate::types::BridgeArtistField>,
    /// Whether the draft contains no authored or sourced metadata.
    pub metadata_draft_is_blank: bool,
    /// Where the current draft began, absent for direct entry and after clear.
    pub metadata_provenance: Option<BridgeMetadataProvenance>,
    /// What the candidate is linked to in the catalogs — a pressing, or an
    /// album whose pressing is unknown — which Unlink clears.
    pub release_link: Option<crate::types::BridgeReleaseLink>,
    /// Every source unit the folder offers, with the track committing makes of
    /// it. Every audio row awaits a pick until there is one.
    pub mapping: BridgeMappingTable,
    /// The cover this candidate commits with.
    pub cover: Option<BridgeCoverChoice>,
    /// Why the last import of this candidate failed.
    pub failure: Option<BridgeError>,
    /// The failure the pane states for its last command, which the pane
    /// commands store; the pane's next command clears it.
    pub pane_failure: Option<BridgePaneFailure>,
    /// Where the pane opens on the candidate: Find online while
    /// identification runs for it or its answer waits on the person, the
    /// draft otherwise. The app keeps where the person moves it from there.
    pub opens_on: BridgeMetadataPresentation,
}

/// Where the queue places the candidate a pane shows, with what the pane
/// states beside it. A Done candidate's pane is the library release it became,
/// so it carries nothing the candidate's draft says.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCandidatePanePlacement {
    /// In Pending.
    Pending {
        /// The check against the folder the candidate's found release did
        /// not pass, stated beside its Import.
        folder_check: Option<crate::types::BridgeFolderCheck>,
        /// Every catalog the linked release is described in, in the order
        /// surfaces list catalogs; `None` for a candidate linked to no
        /// release.
        records: Option<Vec<crate::types::BridgeReleaseRecord>>,
    },
    /// Skipped, with every catalog the draft was read from.
    Skipped { records: Option<Vec<crate::types::BridgeReleaseRecord>> },
    /// In the library.
    Done,
}

/// Which surface the pane's metadata slot shows. Mirrors
/// `bae_import::MetadataPresentation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMetadataPresentation {
    Draft,
    FindOnline,
}

/// A command the pane runs for its candidate whose failure the pane states.
/// Mirrors `bae_import::PaneCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePaneCommand {
    Import,
    ReadFileTags,
    ChangeLookups,
    ChangeSearchWords,
    ChangeAgreements,
    /// Keep the folder's own draft over what its lookup offered.
    KeepOwnDraft,
    /// Link the folder to the album its lookup's offered pressings are of,
    /// its pressing unknown.
    LinkSharedAlbum,
    /// Unlink the candidate from its release.
    Unlink,
}

/// The pane's last command, which failed, and why. Mirrors
/// `bae_import::PaneFailure`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePaneFailure {
    pub command: BridgePaneCommand,
    pub error: BridgeError,
}

/// What a pane command came to: it ran, or it failed and the pane states why.
/// Mirrors `bae_import::PaneOutcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePaneOutcome {
    Done,
    Failed,
}

mirror_enum! {
    #[cfg(desktop)]
    BridgePaneCommand = bae_import::PaneCommand,
    from_core: pub(crate) fn,
    variants: {
        Import,
        ReadFileTags,
        ChangeLookups,
        ChangeSearchWords,
        ChangeAgreements,
        KeepOwnDraft,
        LinkSharedAlbum,
        Unlink,
    },
}

mirror_enum! {
    #[cfg(desktop)]
    BridgePaneOutcome = bae_import::PaneOutcome,
    from_core: pub(crate) fn,
    variants: { Done, Failed },
}

mirror_struct! {
    #[cfg(desktop)]
    BridgePaneFailure = bae_import::PaneFailure,
    from_core: pub(crate) fn,
    fields: {
        command: (BridgePaneCommand),
        error: (BridgeError),
    },
}

mirror_enum! {
    #[cfg(desktop)]
    BridgeMetadataPresentation = bae_import::MetadataPresentation,
    from_core: pub(crate) fn,
    variants: { Draft, FindOnline },
}
