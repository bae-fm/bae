import BaeKit
import SwiftUI

func releaseGroupDisclosureID(
    _ key: BridgeFolderReleaseDecisionKey
) -> ReleaseGroupDisclosureID {
    ReleaseGroupDisclosureID(key: key)
}

/// A group member's place under its folder header: the child inset, plus the
/// thin rail that runs down the members — whitespace and the rail say the
/// membership, not dividers.
enum ImportListHierarchyLayout {
    /// The horizontal padding every list row carries, member or not.
    static let rowEdgePadding: CGFloat = 12
    /// Where the rail runs, from the list edge — under the header's chevron.
    static let railInset: CGFloat = 17
    /// Where a member row's content starts, from the list edge — 10pt past
    /// the rail.
    static let memberContentInset: CGFloat = 28
    /// The leading padding a member row adds on top of its own edge padding
    /// so its content lands at `memberContentInset`.
    static var memberInset: CGFloat { memberContentInset - rowEdgePadding }
    /// Air over a group boundary — a header, or the first top-level row
    /// after a group's members. Rendered as its own spacer row so every real
    /// row keeps a symmetric box for selection and highlight chrome to trace.
    static let groupBoundaryAir: CGFloat = 7
}

/// The filter row's geometry: every control at its end is one hit box, and
/// the glyph inside it is one size, whichever indicator it is.
enum ImportFilterBarLayout {
    /// The clickable square each trailing control occupies.
    static let controlHitSize: CGFloat = 24
    /// The glyph drawn inside that square.
    static let glyphSize: CGFloat = 14
    /// The row's height — the field is the row, so a click anywhere on it
    /// lands in the field.
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
                    .frame(width: 1)
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
            return restore
        }
        observe(rows, viewport: viewport, contentRevision: contentRevision)
        return nil
    }
}

@MainActor
private final class ImportCandidateRevealOperation {
    var task: Task<Void, Never>?

    func cancel() {
        task?.cancel()
    }
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
    let onReveal: (_ key: String) -> Void
    /// Import the highlighted Ready candidates.
    let onImportSelected: () -> Void

    @Environment(UiStore.self)
    private var uiStore
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

    var body: some View {
        ScrollViewReader { proxy in
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
                            .font(.system(size: 13))
                            .focused($filterFocused)
                        if !uiStore.importCandidateFilterText.isEmpty {
                            Button {
                                cancelReveal()
                                listSlot.setFilterText("")
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .font(
                                        .system(
                                            size: ImportFilterBarLayout
                                                .glyphSize
                                        )
                                    )
                                    .foregroundStyle(.tertiary)
                                    .filterBarControl()
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
                                onGoToUnidentified: goToFirstUnidentified(
                                    using: proxy
                                )
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
                    .padding(.horizontal, 14)
                    .frame(height: ImportFilterBarLayout.rowHeight)
                    // The row is the field: a plain text field's own hit
                    // area is its one line of text, so the row takes the
                    // click and puts the caret in the field. The controls at
                    // the end keep their own clicks.
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
            .onDisappear {
                cancelReveal()
            }
            .onReceive(listSlot.candidateRevealRequests) { candidateKey in
                startReveal(using: proxy) {
                    guard
                        let position = try await listSlot.revealCandidate(
                            candidateKey
                        )
                    else { return nil }
                    return (candidateKey, position)
                }
            }
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
    private func tabList(_ proxy: ScrollViewProxy) -> some View {
        if let list = listSlot.list {
            switch uiStore.importCandidateTab {
            case .pending:
                VStack(spacing: 0) {
                    entryList(list, proxy: proxy)
                    if !selectedReadyKeys.isEmpty {
                        Divider()
                        TriageFootBar(
                            selectedCount: selectedReadyKeys.count,
                            readyCount: summary.ready.count,
                            onSelectAll: {
                                selectedKeys = Set(
                                    summary.ready.map(\.candidateKey)
                                )
                            },
                            onSelectNone: { selectedKeys = [] },
                            onImport: {
                                onImportSelected()
                            }
                        )
                    }
                }
            case .done, .skipped:
                entryList(list, proxy: proxy)
            }
        }
    }
}

/// The list itself. In an extension so the view's body and the chrome it
/// builds — tabs, filter, folders — read as one piece above them.
extension ImportCandidateListContent {
    private var selectedReadyKeys: Set<String> {
        let currentReady = Set(summary.ready.map(\.candidateKey))
        return selectedKeys.intersection(currentReady)
    }

    /// Virtualized rows over the paged list: each visible position loads the
    /// page it sits in and renders whatever core put at that offset.
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
        // The list's default minimum row height (~24pt) would inflate the
        // 6pt boundary spacer rows into wide bands of air.
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
                proxy.scrollTo(target, anchor: .top)
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
            case .invalid(_, let invalid, let isGroupMember):
                invalidRow(invalid, isGroupMember: isGroupMember)
            }
        }
        else {
            // The position is inside the list but its page has not landed:
            // an empty row keeps the scroll geometry until it does.
            Color.clear.frame(height: TriageRowView.coverPointSize)
        }
    }

    /// The air over the row at `index`: every header past the top, and the
    /// first top-level row after a group's members. Zero while a page it
    /// reads has not landed, like the placeholder row that holds its spot.
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
            case .candidate, .invalid:
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
        case .invalid(_, _, let isMember):
            return isMember
        }
    }

    /// A folder group renders as a header row followed by sibling list items,
    /// rather than a `DisclosureGroup`. Core marks the actual members so only
    /// those siblings receive the child inset.
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
            // The chevron and the name alone mark a folder — a glyph would
            // repeat what the indent and rail below already say.
            HStack(spacing: 7) {
                Image(systemName: expanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .frame(width: 9)
                Text(group.name)
                    .font(.system(size: 12.5, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, ImportListHierarchyLayout.rowEdgePadding)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            // The rows below are this folder read as several releases, and
            // this is where it is read as one instead — once, for the folder,
            // rather than on each of the rows it produced. A header that only
            // names a shared path component has no such folder behind it and
            // offers nothing.
            if group.combinable {
                Button("Combine as One Release") {
                    onReleaseDecision(group.key, .combineAsOneRelease)
                }
            }
        }
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
            uploadObservation: uploadObservation(for: row),
            isGroupMember: isGroupMember,
            onReveal: { onReveal(row.candidateKey) },
            onSkip: { onSkip(row.candidateKey, $0) },
            onReleaseDecision: onReleaseDecision
        )
        .tag(row.candidateKey)
    }

    private func uploadObservation(
        for row: BridgeTriageRow
    ) -> UploadObservation? {
        guard case .complete(let releaseId, _) = row.importStatus else {
            return nil
        }
        return outboxStore.persistedUploadObservation(
            forRelease: releaseId
        )
    }

    /// An invalid folder isn't selectable — selecting its key is a no-op
    /// upstream (it isn't a real candidate), so it carries no `.tag()`.
    private func invalidRow(
        _ invalid: BridgeInvalidCandidate,
        isGroupMember: Bool
    ) -> some View {
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
        .groupMemberRail(isGroupMember)
    }
}

extension ImportCandidateListContent {
    /// Go to the first row the identify count is still waiting on. `nil` when
    /// there is none to go to.
    private func goToFirstUnidentified(
        using proxy: ScrollViewProxy
    ) -> (() -> Void)? {
        summary.firstUnidentified.map { target in
            {
                startReveal(using: proxy) {
                    guard let position = try await listSlot.reveal(target)
                    else {
                        return nil
                    }
                    return (target.candidateKey, position)
                }
            }
        }
    }

    private func startReveal(
        using proxy: ScrollViewProxy,
        locate:
            @escaping @MainActor () async throws
            -> (candidateKey: String, position: Int)?
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
                guard let target = try await locate(), !Task.isCancelled else {
                    return
                }
                selectedKeys = [target.candidateKey]
                proxy.scrollTo(target.position, anchor: .center)
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
            onReleaseDecision: { _, _ in },
            onSkip: { _, _ in },
            onReveal: { _ in },
            onImportSelected: {}
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
            onReveal: { _ in },
            onImportSelected: {}
        )
        .environment(OutboxStore(snapshot: OutboxStore.emptySnapshot))
        .environment(uiStore)
        .environment(PreviewData.artImageStore())
        .frame(width: 280, height: 560)
        .windowBackground()
    }

#endif
