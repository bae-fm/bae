import BaeKit
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
    /// Air over a group boundary, drawn as its own spacer row so every real
    /// row keeps a symmetric box for the selection highlight.
    static let groupBoundaryAir = ThemeSpace.related
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

struct ImportCandidateListRowBounds: Equatable {
    let stableKey: String
    let bounds: CGRect
}

struct ImportCandidateListGeometry: Equatable {
    var rows: [ImportCandidateListRowBounds] = []
    var viewport: CGRect?
}

struct ImportCandidateListGeometryKey: PreferenceKey {
    static let defaultValue = ImportCandidateListGeometry()

    static func reduce(
        value: inout ImportCandidateListGeometry,
        nextValue: () -> ImportCandidateListGeometry
    ) {
        let next = nextValue()
        value.rows.append(contentsOf: next.rows)
        if let viewport = next.viewport { value.viewport = viewport }
    }
}

struct ImportCandidateListViewport {
    private var anchorKey: String?
    private var appliedContentRevision: UInt64?
    /// Set while a restore's scroll has not run yet, when layouts still
    /// measure the list where it stood before and cannot pick the anchor.
    private var awaitingRestoreScroll = false

    /// The scroll the last restore asked for has run.
    mutating func restoreScrolled() {
        awaitingRestoreScroll = false
    }

    private mutating func accept(contentRevision: UInt64) {
        appliedContentRevision = contentRevision
    }

    private mutating func observe(
        _ rows: [ImportCandidateListRowBounds],
        viewport: CGRect,
        contentRevision: UInt64
    ) {
        if appliedContentRevision == nil {
            appliedContentRevision = contentRevision
        }
        guard appliedContentRevision == contentRevision else { return }
        anchorKey =
            rows
            .filter {
                $0.bounds.maxY > viewport.minY
                    && $0.bounds.minY < viewport.maxY
            }
            .min { $0.bounds.minY < $1.bounds.minY }?
            .stableKey
    }

    private mutating func contentChanged(
        to revision: UInt64,
        positionOf: (String) -> Int?
    ) -> Int? {
        guard revision != appliedContentRevision else { return nil }
        appliedContentRevision = revision
        return anchorKey.flatMap(positionOf)
    }

    mutating func update(
        rows: [ImportCandidateListRowBounds],
        viewport: CGRect,
        contentRevision: UInt64,
        revealInProgress: Bool,
        positionOf: (String) -> Int?
    ) -> Int? {
        if revealInProgress {
            accept(contentRevision: contentRevision)
            observe(rows, viewport: viewport, contentRevision: contentRevision)
            return nil
        }
        if let restore = contentChanged(
            to: contentRevision,
            positionOf: positionOf
        ) {
            awaitingRestoreScroll = true
            return restore
        }
        guard !awaitingRestoreScroll else { return nil }
        observe(rows, viewport: viewport, contentRevision: contentRevision)
        return nil
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
    /// The paged list and the view it is showing.
    let listSlot: ImportListSlot
    @Binding
    var selectedKeys: Set<String>
    let onAddFolder: () -> Void
    /// Stop watching `path`.
    let onRemoveFolder: (_ path: String) -> Void
    let onRefreshFolder: (_ folder: BridgeWatchedFolder) -> Void
    /// Read every release below the header's folder as one.
    let onCombineFolder: (_ key: BridgeFolderReleaseDecisionKey) -> Void
    /// Read the release at `key` as the folders it is made of.
    let onSeparate: (_ key: String) -> Void
    /// Show an imported row's folders.
    let onReveal: (_ key: String) -> Void
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
    @Environment(OutboxStore.self)
    private var outboxStore
    @Environment(\.displayScale)
    private var displayScale
    @State
    private var viewport = ImportCandidateListViewport()
    @State
    private var revealOperation: ImportCandidateRevealOperation?
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

    private var candidateSelectionBinding: Binding<Set<String>> {
        Binding(
            get: { selectedKeys },
            set: {
                cancelReveal()
                selectedKeys = $0
            }
        )
    }

    private var summary: BridgeImportQueueSummary {
        importStore.summary
    }

    /// What the list is narrowed by as the person has it now: the tab, the
    /// typed text and the checked states.
    private var shownNarrowing: BridgeImportListNarrowing {
        BridgeImportListNarrowing(
            tab: uiStore.importCandidateTab,
            filterText: uiStore.importCandidateFilterText,
            pendingFilters: uiStore.importCandidatePendingFilters
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
        ScrollViewReader { proxy in
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
                            pendingFilters: summary.pendingFilters,
                            narrowed: summary.narrowedCount(
                                under: shownNarrowing
                            )
                        ) { filter in
                            cancelReveal()
                            listSlot.setPendingFilter(filter, checked: false)
                        }
                        if let progress = importStore.identificationProgress,
                            progress.total > 0
                        {
                            IdentificationProgressIndicator(
                                identified: progress.identified,
                                total: progress.total,
                                onGoToUnidentified: goToFirstUnidentified(
                                    using: proxy
                                ),
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
                            onSetSortOrder: { order in
                                cancelReveal()
                                listSlot.setSortOrder(order)
                            },
                            pendingFilters: uiStore
                                .importCandidatePendingFilters,
                            pendingFilterApplies: uiStore
                                .importCandidateTab == .pending,
                            onSetPendingFilter: { filter, checked in
                                cancelReveal()
                                listSlot.setPendingFilter(
                                    filter,
                                    checked: checked
                                )
                            },
                            onShowAllPending: {
                                cancelReveal()
                                listSlot.showAllPending()
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
                        .equatable()
                    }
                    .padding(.horizontal, ThemeSpace.group)
                    .frame(height: ImportFilterBarLayout.rowHeight)
                    // A click anywhere on the row puts the caret in the
                    // field; the controls at the end keep their own clicks.
                    .contentShape(Rectangle())
                    .onTapGesture { filterFocused = true }
                }
            } content: {
                if activeTabIsEmpty {
                    emptyState
                }
                else {
                    tabList(proxy)
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
                startReveal(using: proxy) {
                    try await listSlot.revealCandidate(request.candidateKey)
                }
            }
        }
    }

    /// The empty tab, or a narrowed list with no matches.
    private var emptyState: some View {
        ContentUnavailableView(
            summary.narrowed == nil ? "Nothing here yet" : "No matches",
            systemImage: summary.narrowed == nil
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
    private func tabList(_ proxy: ScrollViewProxy) -> some View {
        if let list = listSlot.list {
            entryList(list, proxy: proxy)
                // Select All covers every row the list shows, loaded or not.
                .focusedValue(\.selectAllShownRows) {
                    cancelReveal()
                    listSlot.selectAllShown()
                }
        }
    }
}

/// The list itself.
extension ImportCandidateListContent {
    /// Rows over the paged list; each visible position loads its page.
    private func entryList(
        _ list: PaginatedList<BridgeImportListItem>,
        proxy: ScrollViewProxy
    ) -> some View {
        List(selection: candidateSelectionBinding) {
            ForEach(0..<list.totalCount, id: \.self) { index in
                let stableKey = list.idAt(index)
                let air = airAbove(index, in: list)
                if air > 0 {
                    Color.clear
                        .frame(height: air)
                        .listRowInsets(EdgeInsets())
                        .listRowSeparator(.hidden)
                }
                entry(at: index, in: list)
                    .listRowInsets(EdgeInsets())
                    .listRowSeparator(.hidden)
                    .id(index)
                    .background {
                        if let stableKey {
                            rowGeometry(stableKey: stableKey)
                        }
                    }
                    .task(
                        id: RowLoadID(epoch: list.loadEpoch, index: index)
                    ) {
                        await list.loadPage(containing: index)
                    }
            }
        }
        .listStyle(.plain)
        // The default minimum row height would inflate the boundary spacers.
        .environment(
            \.defaultMinListRowHeight,
            ImportListHierarchyLayout.groupBoundaryAir
        )
        .background {
            GeometryReader { geometry in
                Color.clear.preference(
                    key: ImportCandidateListGeometryKey.self,
                    value: ImportCandidateListGeometry(
                        viewport: geometry.frame(in: .global)
                    )
                )
            }
        }
        .onPreferenceChange(ImportCandidateListGeometryKey.self) { geometry in
            guard let bounds = geometry.viewport else { return }
            if let target = viewport.update(
                rows: geometry.rows,
                viewport: bounds,
                contentRevision: list.contentRevision,
                revealInProgress: revealOperation != nil,
                positionOf: { list.position(of: $0) }
            ) {
                // Run on its own turn so the layouts before it can be told
                // from the ones after.
                Task { @MainActor in
                    proxy.scrollTo(target, anchor: .top)
                    viewport.restoreScrolled()
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.surface)
    }

    private func rowGeometry(stableKey: String) -> some View {
        GeometryReader { geometry in
            Color.clear.preference(
                key: ImportCandidateListGeometryKey.self,
                value: ImportCandidateListGeometry(rows: [
                    ImportCandidateListRowBounds(
                        stableKey: stableKey,
                        bounds: geometry.frame(in: .global)
                    )
                ])
            )
        }
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
            case .candidate(_, let row, let isGroupMember):
                candidateRow(row, isGroupMember: isGroupMember)
            case .imported(_, let row):
                importedRow(row)
            case .invalid(_, let invalid, let isGroupMember):
                invalidRow(invalid, isGroupMember: isGroupMember)
            }
        }
        else {
            // Holds the scroll geometry until the row's page lands.
            Color.clear.frame(height: TriageRowView.coverPointSize)
        }
    }

    /// The air over the row at `index`: every header past the top, and the
    /// first top-level row after a group's members; zero until its page lands.
    private func airAbove(
        _ index: Int,
        in list: PaginatedList<BridgeImportListItem>
    ) -> CGFloat {
        guard index > 0, let key = list.idAt(index),
            let item = importStore.items[key]
        else { return 0 }
        let boundary =
            switch item {
            case .groupHeader:
                true
            case .candidate, .imported, .invalid:
                isGroupMember(at: index, in: list) == false
                    && isGroupMember(at: index - 1, in: list) == true
            }
        return boundary ? ImportListHierarchyLayout.groupBoundaryAir : 0
    }

    /// Whether the item at `index` is a group member; `nil` while its page
    /// has not landed.
    private func isGroupMember(
        at index: Int,
        in list: PaginatedList<BridgeImportListItem>
    ) -> Bool? {
        guard index >= 0, let key = list.idAt(index),
            let item = importStore.items[key]
        else { return nil }
        switch item {
        case .groupHeader:
            return false
        case .candidate(_, _, let isMember):
            return isMember
        case .imported:
            return false
        case .invalid(_, _, let isMember):
            return isMember
        }
    }

    /// A folder group's header row; its members follow as sibling rows.
    private func releaseGroupHeader(
        _ group: BridgeTriageGroup,
        expanded: Bool
    ) -> some View {
        Button {
            cancelReveal()
            listSlot.setGroupExpanded(
                releaseGroupDisclosureID(group.key),
                !expanded
            )
        } label: {
            HStack(spacing: ImportListHierarchyLayout.headerSpacing) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .themeIcon(ImportListHierarchyLayout.headerChevron)
                    .foregroundStyle(.tertiary)
                    .frame(width: ImportListHierarchyLayout.headerChevron.size)
                Text(group.name)
                    .themeText(.strong)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
            .padding(.vertical, ThemeSpace.inline)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            // A header that only names a shared path component offers
            // nothing.
            if group.combinable {
                Button("Combine as One Release") {
                    onCombineFolder(group.key)
                }
            }
        }
    }

    /// What a row's menu offers: the selection's actions when the row is
    /// one of several selected, and the row's own otherwise.
    private func menuOffers(
        for row: BridgeTriageRow,
        live: BridgeCandidateLiveState?
    ) -> CandidateActionMenu {
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
                actions: live?.actions ?? []
            ),
            isSelection: false
        )
    }

    private func cancelReveal() {
        revealOperation?.cancel()
        revealOperation = nil
    }

    private func candidateRow(
        _ row: BridgeTriageRow,
        isGroupMember: Bool
    ) -> some View {
        TriageRowView(
            row: row,
            coverContent: importStore.sidebarCover(for: row),
            isGroupMember: isGroupMember,
            menuOffers: { live in menuOffers(for: row, live: live) },
            onPerform: onPerform
        )
        .tag(row.candidateKey)
    }

    private func importedRow(_ row: BridgeImportedRow) -> some View {
        ImportedRowView(
            row: row,
            uploadObservation: outboxStore.persistedUploadObservation(
                forRelease: row.release.releaseId
            ),
            onReveal: { onReveal(row.candidateKey) }
        )
        .tag(row.candidateKey)
    }

    /// An invalid folder is not a candidate, so it carries no selection tag.
    private func invalidRow(
        _ invalid: BridgeInvalidCandidate,
        isGroupMember: Bool
    ) -> some View {
        InvalidCandidateRow(
            displayName: invalid.sourceFolderName,
            reason: invalid.reason,
            revealPath: invalid.folderPath
        )
        // An unreadable combined folder can only be separated from here.
        .contextMenu {
            if invalid.separable {
                Button("Keep as Separate Releases") {
                    onSeparate(invalid.candidateKey)
                }
            }
        }
        .groupMemberRail(isGroupMember)
    }
}

extension ImportCandidateListContent {
    /// Go to the first row still being identified.
    private func goToFirstUnidentified(
        using proxy: ScrollViewProxy
    ) -> () -> Void {
        {
            startReveal(using: proxy) {
                try await listSlot.revealFirstIdentifying()?.position
            }
        }
    }

    private func startReveal(
        using proxy: ScrollViewProxy,
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
                proxy.scrollTo(position, anchor: .center)
                await Task.yield()
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
            selectedKeys: .constant([PreviewData.importTabCandidate.key]),
            onAddFolder: {},
            onRemoveFolder: { _ in },
            onRefreshFolder: { _ in },
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onReveal: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(ImportSelection())
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
            onCombineFolder: { _ in },
            onSeparate: { _ in },
            onReveal: { _ in },
            onPerform: { _ in },
            onCancelAllIdentification: {},
            onCancelAllImports: {}
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(ImportSelection())
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
        .frame(width: 280, height: 560)
        .windowBackground()
    }

#endif
