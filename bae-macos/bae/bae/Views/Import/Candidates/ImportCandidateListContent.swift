import BaeKit
import Combine
import SwiftUI

func releaseGroupDisclosureID(
    _ key: BridgeFolderReleaseDecisionKey
) -> ReleaseGroupDisclosureID {
    ReleaseGroupDisclosureID(key: key)
}

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
    /// Where the rail runs, from the list edge: centred under the chevron.
    static var railInset: CGFloat {
        rowEdgePadding + (headerChevron.size - railWidth) / 2
    }
    /// Where a member row's content starts, from the list edge: under the
    /// header's name.
    static var memberContentInset: CGFloat {
        rowEdgePadding + headerChevron.size + headerSpacing
    }
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

// periphery:ignore
/// What restarts the list's take of a pending reveal: a new request, or the
/// list it scrolls coming into existence under one already waiting.
private struct PendingRevealID: Hashable {
    let seq: Int?
    let listLoaded: Bool
}

@MainActor
private final class ImportCandidateRevealOperation {
    var task: Task<Void, Never>?

    func cancel() {
        task?.cancel()
    }
}

// MARK: - ImportCandidateListContent

/// The import sidebar: one paged list over the tab the slot is showing, with
/// items, order and grouping decided by core.
struct ImportCandidateListContent: View {
    /// The loaded entries and the summary the chrome around them reads.
    let importStore: ImportStore
    /// The paged list, the view it is showing and the person's place in it.
    let listSlot: ImportListSlot
    let onAddFolder: () -> Void
    /// Stop watching `path`.
    let onRemoveFolder: (_ path: String) -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    /// Read every release below the header's folder as one.
    let onCombineFolder: (_ key: BridgeFolderReleaseDecisionKey) -> Void
    /// Read the release at `key` as the folders it is made of.
    let onSeparate: (_ key: String) -> Void
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
    @Environment(ImportSelection.self)
    private var importSelection
    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var revealOperation: ImportCandidateRevealOperation?
    /// The positions reveals land on, which the table scrolls to.
    @State
    private var reveals = PassthroughSubject<Int, Never>()
    @FocusState
    private var filterFocused: Bool

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

    private var summary: BridgeImportQueueSummary {
        importStore.summary
    }

    /// What the list is narrowed by as the person has it now: the tab, the
    /// typed text and the filter entry.
    private var shownNarrowing: BridgeImportListNarrowing {
        BridgeImportListNarrowing(
            tab: uiStore.importCandidateTab,
            filterText: uiStore.importCandidateFilterText,
            pendingFilter: uiStore.importCandidatePendingFilter
        )
    }

    private var pendingCovers: [ImageContent] {
        summary.pendingCovers.map { .remote($0) }
    }

    /// Whether the active tab has nothing to show.
    private var activeTabIsEmpty: Bool {
        (listSlot.list?.totalCount ?? 0) == 0
    }

    /// Each watched root's scan state, by path.
    private var scanStatuses: [String: BridgeFolderScanStatus] {
        Dictionary(
            summary.folderScanStatuses.map {
                ($0.watchedFolderPath, $0.status)
            },
            uniquingKeysWith: { first, _ in first }
        )
    }

    /// The watched roots on a network volume.
    private var networkFolders: Set<String> {
        Set(
            summary.folderScanStatuses
                .filter(\.onNetworkVolume)
                .map(\.watchedFolderPath)
        )
    }

    var body: some View {
        ImportSidebarList {
            VStack(spacing: 0) {
                TriageTabBar(
                    activeTab: activeTabBinding,
                    counts: summary.counts
                )
                .padding(ThemeSpace.related)

                // Separates choosing the tab from filtering it.
                Divider()

                HStack(spacing: ThemeSpace.related) {
                    ImportListFilterField(
                        text: filterTextBinding,
                        focused: $filterFocused,
                        narrowed: summary.narrowedCount(
                            under: shownNarrowing
                        )
                    )
                    if let progress = importStore.identificationProgress,
                        progress.total > 0
                    {
                        IdentificationProgressIndicator(
                            identified: progress.identified,
                            total: progress.total,
                            onGoToUnidentified: goToFirstUnidentified,
                            onCancelAll: onCancelAllIdentification
                        )
                    }
                    if importStore.importsInFlight > 0 {
                        ImportActivityIndicator(
                            count: importStore.importsInFlight,
                            onCancelAll: onCancelAllImports
                        )
                    }
                    if let activity = summary.folderScanActivity {
                        FolderScanProgressIndicator(activity: activity)
                    }
                    CandidateListMenu(
                        watchedFolders: importStore.watchedFolders,
                        refreshingFolders: uiStore.refreshingWatchedFolders,
                        scanStatuses: scanStatuses,
                        networkFolders: networkFolders,
                        hasGroups: !summary.groupKeys.isEmpty,
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
                                listSlot.setGroupsExpanded(
                                    summary.groupKeys,
                                    expanded
                                )
                            },
                            onRefreshFolder: onRefreshFolder,
                            onRemoveFolder: onRemoveFolder
                        )
                    )
                }
                .padding(.horizontal, ThemeSpace.group)
                .frame(height: ImportFilterBarLayout.rowHeight)
                // A click anywhere on the row puts the caret in the
                // field; the controls at the end keep their own clicks.
                .contentShape(Rectangle())
                .onTapGesture { filterFocused = true }
            }
        } content: {
            switch summary.doneUploadStatus {
            case .waiting:
                UploadQueueWaiting()
            case .read, .unreadable, nil:
                if activeTabIsEmpty {
                    emptyState
                }
                else {
                    VStack(spacing: 0) {
                        if summary.doneUploadStatus == .unreadable {
                            UploadStatusUnreadableNotice()
                        }
                        tabList
                    }
                }
            }
        }
        .task(id: summary.groupKeys) {
            listSlot.retainGroups(summary.groupKeys)
        }
        // Decode Pending's covers as the queue lands, so its first frame
        // is not a grid of spinners.
        .task(id: pendingCovers) {
            await imageStore.warm(
                pendingCovers,
                pointSize: TriageRowView.coverPointSize,
                displayScale: displayScale
            )
        }
        .onDisappear {
            cancelReveal()
        }
        .task(
            id: PendingRevealID(
                seq: uiStore.pendingImportCandidateReveal?.seq,
                listLoaded: listSlot.list != nil
            )
        ) {
            // A reveal taken before the list exists would be lost.
            guard listSlot.list != nil,
                let request = uiStore.pendingImportCandidateReveal
            else { return }
            // Once taken, the person's next action cancels it.
            uiStore.consumeImportCandidateReveal(seq: request.seq)
            startReveal {
                try await listSlot.revealCandidate(request.candidateKey)
            }
        }
    }

    /// The empty tab, or a narrowed list with no matches.
    private var emptyState: some View {
        ContentUnavailableView {
            Text(summary.narrowed == nil ? "Nothing here yet" : "No matches")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.surface)
    }

    @ViewBuilder
    private var tabList: some View {
        if let list = listSlot.list {
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
extension ImportCandidateListContent {
    private var entryActions: ImportListEntryActions {
        ImportListEntryActions(
            onSetGroupExpanded: { group, expanded in
                cancelReveal()
                listSlot.setGroupExpanded(
                    releaseGroupDisclosureID(group.key),
                    expanded
                )
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
        if row.selected, importSelection.summary.count > 1 {
            return CandidateActionMenu(
                offers: ImportCandidateActionOffer.selection(
                    importSelection.summary
                ),
                isSelection: true
            )
        }
        return CandidateActionMenu(
            offers: ImportCandidateActionOffer.row(
                row.candidateKey,
                actions: row.live.actions
            ),
            isSelection: false
        )
    }

    private func cancelReveal() {
        revealOperation?.cancel()
        revealOperation = nil
    }
}

extension ImportCandidateListContent {
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

#if DEBUG

    // MARK: - Previews

    #Preview("Candidate List — Smoke Test") {
        let scene = PreviewData.importSmokeTestScene()
        let uiStore = UiStore()
        uiStore.setImportCandidateTab(.pending)
        return ImportCandidateListContent(
            importStore: scene.store,
            listSlot: scene.slot(uiStore: uiStore),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onShowInLibrary: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(ImportSelection())
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
        .mainWindowPreviewEnvironment()
        .frame(width: 500, height: 900)
        .windowBackground()
    }

    #Preview("Candidate List Narrow") {
        let scene = PreviewData.importTabScene()
        let uiStore = UiStore()
        uiStore.setWatchedFolderRefreshing(
            PreviewData.importWatchedFolder.path,
            true
        )
        return ImportCandidateListContent(
            importStore: scene.store,
            listSlot: scene.slot(uiStore: uiStore),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onShowInLibrary: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(ImportSelection())
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
        .mainWindowPreviewEnvironment()
        .frame(width: 280, height: 560)
        .windowBackground()
    }

#endif

/// Done before the upload queue first answers: its rows are ordered by where
/// their uploads stand, so it shows none until then.
private struct UploadQueueWaiting: View {
    var body: some View {
        ProgressView("Waiting for the upload queue")
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Theme.surface)
    }
}

/// Above Done's rows when the upload queue couldn't be read: they are in import
/// order instead of upload order.
private struct UploadStatusUnreadableNotice: View {
    var body: some View {
        Label(
            "Upload status couldn't be read. Releases are in the order they were imported.",
            systemImage: "exclamationmark.triangle"
        )
        .font(.callout)
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, ThemeSpace.group)
        .padding(.vertical, ThemeSpace.compact)
    }
}
