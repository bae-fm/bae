import BaeKit
import Combine
import SwiftUI

/// A group member's place under its folder header: the child inset and the
/// rail that runs down the members.
enum ImportListHierarchyLayout {
    /// The horizontal padding every list row carries, member or not.
    static let rowEdgePadding = ThemeSpace.group
    /// The folder header's disclosure chevron.
    static let headerChevron = ThemeIcon.small
    /// The gap between the header's chevron and its name.
    static let headerSpacing = ThemeSpace.compact
    static let railWidth: CGFloat = 1
    /// Where the rail runs, from the list edge: under the chevron.
    static let railInset: CGFloat = 16
    /// Where a member row's content starts, from the list edge: under the
    /// header's name.
    static let memberContentInset: CGFloat = 30
    /// The leading padding a member row adds on top of its own edge padding
    /// so its content lands at `memberContentInset`.
    static var memberInset: CGFloat { memberContentInset - rowEdgePadding }
}

/// The filter row's geometry: one hit box and one glyph for every control at
/// its end.
enum ImportFilterBarLayout {
    /// The clickable square each trailing control occupies.
    static let controlHitSize = ThemeSize.hitTarget
    /// The glyph drawn inside that square.
    static let glyph = ThemeIcon.medium
    static let rowHeight: CGFloat = 36
}

extension View {
    /// A trailing filter-row control: a square hit box around its glyph.
    func filterBarControl() -> some View {
        frame(
            width: ImportFilterBarLayout.controlHitSize,
            height: ImportFilterBarLayout.controlHitSize
        )
        .contentShape(Rectangle())
    }

    func groupMemberRail(_ isGroupMember: Bool) -> some View {
        padding(
            .leading,
            isGroupMember ? ImportListHierarchyLayout.memberInset : 0
        )
        .overlay(alignment: .leading) {
            if isGroupMember {
                Rectangle()
                    .fill(Theme.hairline)
                    .frame(width: ImportListHierarchyLayout.railWidth)
                    .padding(.leading, ImportListHierarchyLayout.railInset)
            }
        }
    }
}

@MainActor
private final class ImportCandidateRevealOperation {
    var task: Task<Void, Never>?

    func cancel() {
        task?.cancel()
    }
}

// MARK: - MacImportSidebarContainer

/// Reads the stores for `MacImportSidebar`: one paged list over the tab the
/// slot is showing, with items, order and grouping decided by core.
struct MacImportSidebarContainer: View {
    /// The loaded entries and the summary the chrome around them reads.
    let importStore: ImportStore
    /// The paged list, the view it is showing and the person's place in it.
    let listSlot: ImportListSlot
    let onAddFolder: () -> Void
    /// Stop watching `path`.
    let onRemoveFolder: (_ path: Data) -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolderScanStatus) -> Void
    /// Read every release below the header's folder as one.
    let onCombineFolder: (_ key: BridgeFolderReleaseDecisionKey) -> Void
    /// Read the release at `key` as the folders it is made of.
    let onSeparate: (_ key: Data) -> Void
    /// Open the release this device imported from a Done row's files.
    let onShowInLibrary: (_ release: BridgeImportedRelease) -> Void
    /// Run one action a row's menu offers, for the row or the selection.
    let onPerform: (ImportCandidateActionOffer) -> Void
    /// Take every candidate off the identification queue.
    let onCancelAllIdentification: () -> Void
    /// Cancel every import that has not begun writing its release.
    let onCancelAllImports: () -> Void

    @Environment(UiStore.self)
    private var uiStore
    /// The candidate a navigation asked the list to show before the list
    /// existed, which the list shows once it does.
    @State
    private var requestedReveal: Data?
    /// The reveal on its way: from a request, once the list exists, or from
    /// the identification progress line.
    @State
    private var revealOperation: ImportCandidateRevealOperation?
    /// The positions reveals land on, which the table scrolls to.
    @State
    private var reveals = PassthroughSubject<Int, Never>()

    private var filterTextBinding: Binding<String> {
        Binding(
            get: { uiStore.importCandidateFilterText },
            set: {
                cancelReveal()
                listSlot.setFilterText($0)
            }
        )
    }

    private var activeTabBinding: Binding<BridgeTriageTab> {
        Binding(
            get: { uiStore.importCandidateTab },
            set: {
                cancelReveal()
                listSlot.setTab($0)
            }
        )
    }

    private var summary: BridgeImportQueueSummary? {
        importStore.summary
    }

    /// What the list is narrowed by as the person has it now: the tab, the
    /// typed text and the filter entry.
    private var shownNarrowing: ImportListNarrowing {
        ImportListNarrowing(
            tab: uiStore.importCandidateTab,
            filterText: uiStore.importCandidateFilterText,
            pendingFilter: uiStore.importCandidatePendingFilter
        )
    }

    var body: some View {
        MacImportSidebar(
            activeTab: activeTabBinding,
            listSlot: listSlot,
            filterText: filterTextBinding,
            narrowed: { listSlot.narrowedCount(under: shownNarrowing) }
        ) {
            IdentificationProgressSlot(
                importStore: importStore,
                onGoToUnidentified: goToFirstUnidentified,
                onCancelAll: onCancelAllIdentification
            )
            ImportActivitySlot(
                importStore: importStore,
                onCancelAll: onCancelAllImports
            )
            FolderScanActivitySlot(importStore: importStore)
            if importStore.hasWatchedFolders != nil {
                CandidateListMenu(
                    refreshingFolders: uiStore.refreshingWatchedFolders,
                    hasGroups: summary?.hasGroups ?? false,
                    sortOrder: listSlot.sortOrder,
                    pendingFilter: uiStore
                        .importCandidatePendingFilter,
                    importStore: importStore,
                    pendingFilterApplies: uiStore
                        .importCandidateTab == .pending,
                    actions: CandidateListMenuActions(
                        onSetSortOrder: { order in
                            cancelReveal()
                            listSlot.setSortOrder(order)
                        },
                        onSetPendingFilter: { filter in
                            cancelReveal()
                            listSlot.setPendingFilter(filter)
                        },
                        onAddFolder: onAddFolder,
                        onSetAllGroupsExpanded: { expanded in
                            cancelReveal()
                            listSlot.setAllGroupsExpanded(expanded)
                        },
                        onRefreshFolder: onRefreshFolder,
                        onRemoveFolder: onRemoveFolder
                    )
                )
            }
        } content: {
            ImportCandidateListSwitch(
                listSlot: listSlot,
                importStore: importStore,
                table: { tabList }
            )
        }
        .onDisappear {
            cancelReveal()
        }
        .onReceive(uiStore.importCandidateReveals.requests) { candidateKey in
            // A reveal started before the list exists would be lost.
            if listSlot.list != nil {
                reveal(candidateKey)
            }
            else {
                requestedReveal = candidateKey
            }
        }
        .onChange(of: listSlot.list != nil) { _, listExists in
            guard listExists, let candidateKey = requestedReveal else {
                return
            }
            requestedReveal = nil
            reveal(candidateKey)
        }
    }

    @ViewBuilder
    private var tabList: some View {
        if let list = listSlot.list, importStore.summary != nil {
            ImportCandidateTable(
                list: list,
                importStore: importStore,
                listSlot: listSlot,
                actions: entryActions,
                reveals: reveals.eraseToAnyPublisher(),
                revealInProgress: revealOperation != nil,
                onPersonSelected: cancelReveal
            )
            .background(Theme.surface)
        }
    }
}

/// The rows' actions.
extension MacImportSidebarContainer {
    private var entryActions: ImportListEntryActions {
        ImportListEntryActions(
            onSetGroupExpanded: { group, expanded in
                cancelReveal()
                listSlot.setGroupExpanded(group.key, expanded)
            },
            onCombineFolder: onCombineFolder,
            onSeparate: onSeparate,
            onShowInLibrary: onShowInLibrary,
            onPerform: onPerform,
            menuOffers: menuOffers(for:)
        )
    }

    /// What a row's menu offers: the selection's actions when the row is
    /// one of several selected, and the row's own otherwise.
    private func menuOffers(for row: BridgeTriageRow) -> CandidateActionMenu {
        let selection = listSlot.selection
        if selection.keys.contains(row.candidateKey),
            selection.summary.count > 1
        {
            return CandidateActionMenu(
                offers: ImportCandidateActionOffer.selection(
                    selection.summary
                ),
                isSelection: true
            )
        }
        return CandidateActionMenu(
            offers: row.live.offers.map {
                ImportCandidateActionOffer($0, rowKey: row.candidateKey)
            },
            isSelection: false
        )
    }

    private func cancelReveal() {
        revealOperation?.cancel()
        revealOperation = nil
    }
}

extension MacImportSidebarContainer {
    /// Show the candidate a navigation asked for; the person's next action
    /// cancels it on its way.
    private func reveal(_ candidateKey: Data) {
        startReveal {
            try await listSlot.revealCandidate(candidateKey)
        }
    }

    /// Go to the first row still being identified.
    private func goToFirstUnidentified() {
        startReveal {
            try await listSlot.revealFirstIdentifying()?.position
        }
    }

    private func startReveal(
        locate: @escaping @MainActor () async throws -> Int?
    ) {
        revealOperation?.cancel()
        let operation = ImportCandidateRevealOperation()
        revealOperation = operation
        operation.task = Task {
            defer {
                if revealOperation === operation {
                    revealOperation = nil
                }
            }
            do {
                guard let position = try await locate(), !Task.isCancelled
                else {
                    return
                }
                reveals.send(position)
            }
            catch is CancellationError {}
            catch {
                uiStore.showError(error)
            }
        }
    }
}

/// The list's table, or the empty tab, or a narrowed list with no matches.
/// The view draws a plain surface until the list query and the summary have
/// returned their first values, and while core has not returned the count: a
/// list core has not counted yet is neither empty nor filled, and only the
/// count tells them apart. The slot shows a list only once it has its first
/// rows and its count, so another tab, filter, search or order keeps drawing
/// the rows it had until the new one's arrive.
/// A view of its own so that the list's count and the summary are read here
/// and not by the list's chrome.
private struct ImportCandidateListSwitch<Table: View>: View {
    let listSlot: ImportListSlot
    let importStore: ImportStore
    @ViewBuilder
    let table: () -> Table

    var body: some View {
        if let list = listSlot.list, importStore.summary != nil {
            if let count = list.count, count.rows > 0 {
                table()
            }
            else if list.count != nil {
                ContentUnavailableView {
                    Text(
                        list.isNarrowed ? "No matches" : "Nothing here yet"
                    )
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Theme.surface)
            }
            else {
                Rectangle().fill(Theme.surface)
            }
        }
        else {
            Rectangle().fill(Theme.surface)
        }
    }
}
