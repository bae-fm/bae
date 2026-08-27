import BaeKit
import SwiftUI

func releaseGroupDisclosureID(
    _ key: BridgeFolderReleaseDecisionKey
) -> ReleaseGroupDisclosureID {
    ReleaseGroupDisclosureID(key: key)
}

// MARK: - ImportCandidateListContent

/// The import sidebar: one paged list over the tab the slot is showing. Which
/// items exist, in what order, under which header and in which tab is core's
/// answer — the list asks for a page of offsets and renders what comes back.
/// Everything around it comes from the same value's summary.
struct ImportCandidateListContent: View {
    /// Read at the leaf: the loaded entries and the chrome around them come
    /// from the store.
    let importStore: ImportStore
    /// The paged list and the view it is showing.
    let listSlot: ImportListSlot
    @Binding
    var selectedKeys: Set<String>
    let onAddFolder: () -> Void
    /// Stop watching `path`. Reached from the watched-folders menu; release
    /// groups inside each root are rendered by the list below.
    let onRemoveFolder: (_ path: String) -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    let onReleaseDecision:
        (
            _ key: BridgeFolderReleaseDecisionKey,
            _ decision: BridgeFolderReleaseDecision
        ) -> Void
    /// Skip (or unskip) the candidate at `key`. Wired to the row context menu.
    let onSkip: (_ key: String, _ skipped: Bool) -> Void
    /// Import every candidate in `keys` — Pending's bulk action.
    let onImportSelected: (_ keys: [String]) -> Void

    @Environment(UiStore.self)
    private var uiStore
    @Environment(ImageStore.self)
    private var imageStore
    @Environment(\.displayScale)
    private var displayScale

    private var filterTextBinding: Binding<String> {
        Binding(
            get: { uiStore.importCandidateFilterText },
            set: { listSlot.setFilterText($0) }
        )
    }

    private var activeTabBinding: Binding<BridgeTriageTab> {
        Binding(
            get: { uiStore.importCandidateTab },
            set: { listSlot.setTab($0) }
        )
    }

    private var summary: BridgeImportQueueSummary {
        importStore.summary
    }

    private var readyCovers: [ImageContent] {
        summary.ready.compactMap { row in
            row.coverThumbnailUrl.map { .remote(url: $0) }
        }
    }

    /// True when the active tab has nothing to show — drives the empty state.
    private var activeTabIsEmpty: Bool {
        (listSlot.list?.totalCount ?? 0) == 0
    }

    /// Where each watched root's scan stands, by path — what the folder menu
    /// marks its entries with.
    private var scanStatuses: [String: BridgeFolderScanStatus] {
        Dictionary(
            summary.folderScanStatuses.map {
                ($0.watchedFolderPath, $0.status)
            },
            uniquingKeysWith: { first, _ in first }
        )
    }

    /// The watched roots on a volume served over the network — what the folder
    /// menu explains the checking schedule on.
    private var networkFolders: Set<String> {
        Set(
            summary.folderScanStatuses
                .filter(\.onNetworkVolume)
                .map(\.watchedFolderPath)
        )
    }

    /// Go to the first row the identify count is still waiting on. `nil` when
    /// there is none to go to.
    private var goToFirstUnidentified: (() -> Void)? {
        summary.firstUnidentifiedKey.map { key in
            {
                listSlot.setTab(.pending)
                selectedKeys = [key]
            }
        }
    }

    var body: some View {
        ImportSidebarList {
            VStack(spacing: 0) {
                TriageTabBar(
                    activeTab: activeTabBinding,
                    counts: summary.counts
                )
                .padding(.horizontal, 10)
                .padding(.top, 10)
                .padding(.bottom, 10)

                // The tabs choose what the list holds; the filter narrows what
                // it shows. Two jobs, so the header says where one ends.
                Divider()

                HStack(spacing: 8) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 13))
                        .foregroundStyle(.tertiary)
                    TextField("Filter...", text: filterTextBinding)
                        .textFieldStyle(.plain)
                        .font(.system(size: 12.5))
                    if !uiStore.importCandidateFilterText.isEmpty {
                        Button {
                            listSlot.setFilterText("")
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.tertiary)
                        }
                        .buttonStyle(.plain)
                    }
                    if let progress = importStore.queueIdentifyProgress,
                        progress.total > 0,
                        progress.identified < progress.total
                    {
                        QueueProgressIndicator(
                            identified: progress.identified,
                            total: progress.total,
                            onGoToUnidentified: goToFirstUnidentified
                        )
                    }
                    CandidateListMenu(
                        watchedFolders: importStore.watchedFolders,
                        refreshingFolders: uiStore.refreshingWatchedFolders,
                        scanStatuses: scanStatuses,
                        networkFolders: networkFolders,
                        onAddFolder: onAddFolder,
                        onRefreshFolder: onRefreshFolder,
                        onRemoveFolder: onRemoveFolder
                    )
                    .equatable()
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
            }
        } content: {
            if activeTabIsEmpty {
                emptyState
            }
            else {
                tabList
            }
        }
        .task(id: summary.groupKeys) {
            listSlot.retainGroups(summary.groupKeys)
        }
        // Importable rows are covers this app has already downloaded once.
        // Decoding them as the queue lands keeps Pending's first frame from
        // being a grid of spinners.
        .task(id: readyCovers) {
            await imageStore.warm(
                readyCovers,
                pointSize: TriageRowView.coverPointSize,
                displayScale: displayScale
            )
        }
    }

    /// Per-tab empty state: distinguishes "no matches" while filtering from
    /// "nothing in this tab yet".
    private var emptyState: some View {
        ContentUnavailableView(
            uiStore.importCandidateFilterText.isEmpty
                ? "Nothing here yet" : "No matches",
            systemImage: uiStore.importCandidateFilterText.isEmpty
                ? emptyTabSymbol : "magnifyingglass"
        )
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.surface)
    }

    private var emptyTabSymbol: String {
        switch uiStore.importCandidateTab {
        case .pending: "questionmark.circle"
        case .done: "tray.full"
        case .skipped: "minus.circle"
        }
    }

    @ViewBuilder
    private var tabList: some View {
        if let list = listSlot.list {
            switch uiStore.importCandidateTab {
            case .pending:
                VStack(spacing: 0) {
                    entryList(list)
                    Divider()
                    TriageFootBar(
                        selectedCount: selectedReadyKeys.count,
                        readyCount: summary.ready.count,
                        onSelectAll: {
                            uiStore.selectAllReady(
                                summary.ready.map(\.candidateKey)
                            )
                        },
                        onSelectNone: { uiStore.clearReadySelection() },
                        onImport: {
                            onImportSelected(Array(selectedReadyKeys))
                        }
                    )
                }
            case .done, .skipped:
                entryList(list)
            }
        }
    }
}

/// The list itself. In an extension so the view's body and the chrome it
/// builds — tabs, filter, folders — read as one piece above them.
extension ImportCandidateListContent {
    private var selectedReadyKeys: Set<String> {
        let currentReady = Set(summary.ready.map(\.candidateKey))
        return uiStore.selectedReadyCandidates.intersection(currentReady)
    }

    /// Virtualized rows over the paged list: each visible position loads the
    /// page it sits in and renders whatever core put at that offset.
    private func entryList(
        _ list: PaginatedList<BridgeImportListItem>
    ) -> some View {
        List(selection: $selectedKeys) {
            ForEach(0..<list.totalCount, id: \.self) { index in
                entry(at: index, in: list)
                    .task(
                        id: RowLoadID(epoch: list.loadEpoch, index: index)
                    ) {
                        await list.loadPage(containing: index)
                    }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    @ViewBuilder
    private func entry(
        at index: Int,
        in list: PaginatedList<BridgeImportListItem>
    ) -> some View {
        if let stableKey = list.idAt(index),
            let item = importStore.items[stableKey]
        {
            switch item {
            case .groupHeader(_, let group, _, let expanded, _):
                releaseGroupHeader(group, expanded: expanded)
            case .candidate(_, let row):
                candidateRow(row)
            case .invalid(_, let invalid):
                invalidRow(invalid)
            }
        }
        else {
            // The position is inside the list but its page has not landed:
            // an empty row keeps the scroll geometry until it does.
            Color.clear.frame(height: TriageRowView.coverPointSize)
        }
    }

    /// A folder group renders as a header row followed by its rows as
    /// siblings — not a `DisclosureGroup`, which indents whatever it contains.
    /// Grouped and ungrouped rows share one leading edge; the header row is
    /// the only thing that says a group is there.
    private func releaseGroupHeader(
        _ group: BridgeTriageGroup,
        expanded: Bool
    ) -> some View {
        Button {
            listSlot.setGroupExpanded(
                releaseGroupDisclosureID(group.key),
                !expanded
            )
        } label: {
            HStack(spacing: 6) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .frame(width: 9)
                Label(group.name, systemImage: "folder")
                    .font(.system(size: 12.5, weight: .semibold))
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .overlay(alignment: .trailing) {
            // The rows below are this folder read as several releases, and
            // this is where it is read as one instead — once, for the folder,
            // rather than on each of the rows it produced. A header that only
            // names a shared path component has no such folder behind it and
            // offers nothing.
            if group.combinable {
                Button("Combine as One Release") {
                    onReleaseDecision(group.key, .combineAsOneRelease)
                }
                .buttonStyle(.link)
                .font(.system(size: 11))
            }
        }
    }

    /// The Ready row's bulk-import checkbox, over the selection `UiStore`
    /// holds.
    private func readySelection(for row: BridgeTriageRow) -> Binding<Bool> {
        Binding(
            get: {
                uiStore.selectedReadyCandidates.contains(row.candidateKey)
            },
            set: {
                uiStore.setReadySelection(row.candidateKey, selected: $0)
            }
        )
    }

    private func candidateRow(_ row: BridgeTriageRow) -> some View {
        TriageRowView(
            row: row,
            coverContent: importStore.sidebarCover(for: row),
            selection: row.selectable ? readySelection(for: row) : nil,
            onSkip: { onSkip(row.candidateKey, $0) },
            onReleaseDecision: onReleaseDecision
        )
        .tag(row.candidateKey)
        .disabled(!row.actionable)
    }

    /// An invalid folder isn't selectable — selecting its key is a no-op
    /// upstream (it isn't a real candidate), so it carries no `.tag()`.
    private func invalidRow(_ invalid: BridgeInvalidCandidate) -> some View {
        InvalidCandidateRow(
            displayName: invalid.sourceFolderName,
            reason: invalid.reason,
            revealPath: invalid.folderPath
        )
        // A folder read as one release that turned out to be unreadable is
        // still that folder, and its row is the only place left to say it
        // should be read as several.
        .contextMenu {
            ForEach(
                invalid.resolvedBoundaries.filter(isCombined),
                id: \.key
            ) { boundary in
                Button("Keep as Separate Releases") {
                    onReleaseDecision(
                        boundary.key,
                        .keepAsSeparateReleases
                    )
                }
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
        uiStore.selectAllReady([PreviewData.importTabCandidate.key])
        return ImportCandidateListContent(
            importStore: scene.store,
            listSlot: scene.slot(uiStore: uiStore),
            selectedKeys: .constant([PreviewData.importTabCandidate.key]),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onReleaseDecision: { _, _ in },
            onSkip: { _, _ in },
            onImportSelected: { _ in }
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
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
            selectedKeys: .constant([]),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onReleaseDecision: { _, _ in },
            onSkip: { _, _ in },
            onImportSelected: { _ in }
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
        .frame(width: 280, height: 560)
        .windowBackground()
    }

#endif
