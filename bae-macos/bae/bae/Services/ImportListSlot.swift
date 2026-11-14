import BaeKit
import Foundation
import Observation

/// Where a view change opened the list: what the list was narrowed by, and
/// the row it shows on top, its first selected row or its top.
struct ImportListOpened: Equatable {
    let narrowing: BridgeImportListNarrowing
    let position: Int
}

/// The import sidebar's list machinery: the warm `PaginatedList`, the reads
/// around it, and the view they are showing.
///
/// The view — which tab, which filter text, which order — and the groups
/// folded shut decide which items sit in which windows, so they are core's
/// list's rather than applied to a window after it arrives. A view change is
/// a write that answers where the new view opens; once it has, the list's
/// windows and count are dropped and read again from that place, so every
/// value they deliver was read after the write. Until the first of them lands
/// the rows the list holds are the previous view's, which the sidebar does
/// not show as this one's. Which groups are folded is core's alone; each
/// header says whether it is open.
///
/// `UiStore` keeps the tab and the filter text as session state, so every
/// setter of them here writes both: the store the sidebar renders from, and
/// the view core answers.
@MainActor
@Observable
final class ImportListSlot {
    private(set) var list: PaginatedList<BridgeImportListItem>?
    /// Whether the rows the list holds answer the view it shows: false from
    /// the moment another tab, filter or order is asked for until the windows
    /// that read it have landed. Before then the list's rows are the previous
    /// view's.
    private(set) var answersView = true
    private(set) var sortOrder: BridgeImportListOrder
    /// Where the last view change opened the list, which the table scrolls
    /// to when it is narrowed anew.
    private(set) var opened: ImportListOpened
    /// Where the person is in the list, as the table last recorded it. Held
    /// here rather than by the table so the import tab finds it again after
    /// another tab was shown. Nothing draws from it.
    @ObservationIgnored
    var place: ImportListPlace?
    @ObservationIgnored
    private let defaults: UserDefaults
    private static let sortPreferenceKey = "importCandidateSortOrder"

    /// The view core answers the list for.
    @ObservationIgnored
    private(set) var view: BridgeImportListView
    @ObservationIgnored
    private let makeHandle:
        (BridgeImportListView) async throws -> ImportListHandle
    @ObservationIgnored
    private var handle: ImportListHandle?
    @ObservationIgnored
    private let importStore: ImportStore
    @ObservationIgnored
    private let uiStore: UiStore
    /// What a person does to the rows goes here; core's list keeps it to the
    /// rows the view shows.
    @ObservationIgnored
    private let selection: ImportSelection
    /// Where the selection's values go beside the selection the panes read.
    @ObservationIgnored
    private let onSelection:
        @MainActor @Sendable (BridgeSelectionSummary) -> Void
    /// The reads around the list: the summary, Found's filter entries and
    /// the selection.
    @ObservationIgnored
    private var surroundings: [any Sendable] = []
    /// The last selection write asked for, which the next one waits on.
    @ObservationIgnored
    private var selectionTask: Task<Void, Never>?
    /// The last fold asked for, which the next one waits on.
    @ObservationIgnored
    private var foldTask: Task<Void, Never>?
    @ObservationIgnored
    private var reloadTask: Task<Void, Never>?
    /// The last view change asked for, which the next one waits on so the
    /// writes reach core in the order they were made.
    @ObservationIgnored
    private var viewChange: Task<Void, Never>?
    /// Set when a read failed. The reads behind a failed one are finished,
    /// so the next view change has to build a new list rather than reopen
    /// the dead one.
    @ObservationIgnored
    private var sourceFailed = false

    /// Why the list could not be read, for as long as that stands.
    ///
    /// A read that fails delivers no rows, no watched folders and no summary,
    /// which is indistinguishable from a library that has none — so the import
    /// tab must render this rather than its "add a folder" prompt. Nil is the
    /// only state in which the absence of folders means the person has not
    /// added any.
    private(set) var loadFailure: DisplayError?

    init(
        importStore: ImportStore,
        uiStore: UiStore,
        selection: ImportSelection,
        defaults: UserDefaults = .standard,
        onSelection:
            @escaping @MainActor @Sendable (BridgeSelectionSummary) -> Void = {
                _ in
            },
        makeHandle:
            @escaping (BridgeImportListView) async throws -> ImportListHandle
    ) {
        self.importStore = importStore
        self.uiStore = uiStore
        self.selection = selection
        self.onSelection = onSelection
        self.makeHandle = makeHandle
        self.defaults = defaults
        // A saved order that doesn't decode is logged and sorts newest first.
        let initialOrder =
            defaults.decoded(
                BridgeImportListOrder.self,
                forKey: Self.sortPreferenceKey
            ) ?? .newestFirst
        sortOrder = initialOrder
        let initialView = BridgeImportListView(
            tab: uiStore.importCandidateTab,
            filterText: uiStore.importCandidateFilterText,
            pendingFilter: uiStore.importCandidatePendingFilter,
            order: initialOrder
        )
        view = initialView
        opened = ImportListOpened(
            narrowing: Self.narrowing(of: initialView),
            position: 0
        )
    }

    /// What `view` narrows the list by.
    private static func narrowing(
        of view: BridgeImportListView
    ) -> BridgeImportListNarrowing {
        BridgeImportListNarrowing(
            tab: view.tab,
            filterText: view.filterText,
            pendingFilter: view.pendingFilter
        )
    }

    /// Build the list and read its first windows. Called once the app's
    /// subscriptions start, and again when a view change follows a failed
    /// read.
    func startLoad() {
        reloadTask?.cancel()
        reloadTask = Task { [weak self] in
            await self?.reload()
        }
    }

    func setTab(_ tab: BridgeTriageTab) {
        uiStore.setImportCandidateTab(tab)
        updateView { $0.tab = tab }
    }

    func setFilterText(_ text: String) {
        uiStore.setImportCandidateFilterText(text)
        updateView { $0.filterText = text }
    }

    /// Narrow Found's rows to one filter entry.
    func setPendingFilter(_ filter: BridgePendingFilter) {
        uiStore.setImportCandidatePendingFilter(filter)
        updateView { $0.pendingFilter = filter }
    }

    func setSortOrder(_ order: BridgeImportListOrder) {
        sortOrder = order
        defaults.setEncoded(order, forKey: Self.sortPreferenceKey)
        updateView { $0.order = order }
    }

    /// How many of the tab's rows the list shows, when the list's count was
    /// read for `narrowing` — the filter the field holds now — and nothing
    /// while it was read for another: a count stands only beside the filter
    /// it is for.
    func narrowedCount(
        under narrowing: BridgeImportListNarrowing
    ) -> BridgeNarrowedCount? {
        guard narrowing == opened.narrowing, let extra = list?.countExtra,
            case .import(let narrowed) = extra
        else { return nil }
        return narrowed
    }

}

// MARK: - Revealing, view changes and loading

extension ImportListSlot {
    /// Navigate to the first candidate the identification count is still
    /// waiting on, as core finds it when asked. `nil` when nothing is.
    func revealFirstIdentifying() async throws
        -> (candidateKey: Data, position: Int)?
    {
        guard let handle,
            let candidateKey = try await handle.firstIdentifyingCandidate(),
            let position = try await revealCandidate(candidateKey)
        else { return nil }
        return (candidateKey, position)
    }

    /// Navigate to the candidate's current authoritative placement, even when
    /// that placement has moved it out of the list presently on screen, and
    /// select it alone.
    func revealCandidate(_ candidateKey: Data) async throws -> Int? {
        guard let handle,
            let location = try await handle.locateCandidate(candidateKey)
        else {
            return nil
        }
        uiStore.setImportCandidateTab(location.tab)
        uiStore.setImportCandidateFilterText("")
        uiStore.setImportCandidatePendingFilter(.all)
        if let groupKey = location.groupKey {
            try await handle.setGroupFolded(groupKey, false)
        }
        var next = view
        next.tab = location.tab
        next.filterText = ""
        next.pendingFilter = .all
        view = next
        answersView = false
        // The list keeps only the rows its view shows selected, so it is
        // asked for the view that shows the candidate before it selects it.
        // The reveal's own place replaces the one the view would open at.
        _ = try await handle.setView(next)
        try await selectOnly(candidateKey)
        guard let list else { return nil }
        let position = Int(location.visiblePosition)
        await list.reopen(at: location.key, position: position)
        opened = ImportListOpened(
            narrowing: Self.narrowing(of: next),
            position: position
        )
        if view == next { answersView = true }
        let stableKey = location.stableKey
        let placed = await list.holds(stableKey, at: position)
        guard !Task.isCancelled, placed else { return nil }
        return position
    }

    private func updateView(_ change: (inout BridgeImportListView) -> Void) {
        var next = view
        change(&next)
        guard next != view else { return }
        view = next
        // A failed read is not retried on its own: it is reported, the list
        // shows what it could not load, and the next thing the person does
        // with the list is what asks core again.
        if sourceFailed {
            answersView = false
            startLoad()
            return
        }
        guard handle != nil else { return }
        show(next)
    }

    /// Ask core's list for `next`, and open the windows again from where it
    /// opens, unless another view was asked for since.
    private func show(_ next: BridgeImportListView) {
        answersView = false
        let previous = viewChange
        viewChange = Task { [weak self] in
            await previous?.value
            guard let self, view == next, let handle, let list else { return }
            let opening: BridgeImportListOpening?
            do {
                opening = try await handle.setView(next)
            }
            catch {
                fail(error)
                return
            }
            guard view == next else { return }
            if let opening {
                switch opening {
                case .top:
                    await list.reopen(at: nil, position: 0)
                    opened = ImportListOpened(
                        narrowing: Self.narrowing(of: next),
                        position: 0
                    )
                case .at(let key, let position):
                    await list.reopen(at: key, position: Int(position))
                    opened = ImportListOpened(
                        narrowing: Self.narrowing(of: next),
                        position: Int(position)
                    )
                }
            }
            if view == next { answersView = true }
        }
    }

    /// A read failed: report it, and have the next change rebuild the list.
    private func fail(_ error: any Error) {
        sourceFailed = true
        // A cancellation has no line and is not a failed read.
        guard let displayed = DisplayError(error) else { return }
        loadFailure = displayed
        uiStore.showError(displayed)
    }

    private func reload() async {
        sourceFailed = false
        loadFailure = nil
        let requested = view
        let made: ImportListHandle
        do {
            made = try await makeHandle(requested)
        }
        catch {
            fail(error)
            return
        }
        guard !Task.isCancelled else { return }
        list?.cancel()
        if let replaced = handle {
            Task { [uiStore] in
                do { try await replaced.close() }
                catch { uiStore.showError(error) }
            }
        }
        let newList = PaginatedList<BridgeImportListItem>(
            queries: made.queries,
            onError: { [weak self] error in self?.fail(error) }
        )
        openSurroundings(of: made)
        await newList.loadInitial()
        // The first window's failure never reaches `onError`: `PaginatedList`
        // keeps it as `initialLoadError` for a list view to render inline, the
        // way every other list surface reads it. The import tab is not a bare
        // list — it decides between three panes before one is drawn — so it has
        // to read that outcome here, or a library nobody could look at renders
        // as a library with no folders.
        if let initial = newList.initialLoadError {
            sourceFailed = true
            loadFailure = initial
            uiStore.showError(initial)
        }
        guard !Task.isCancelled else { return }
        handle = made
        list = newList
        opened = ImportListOpened(
            narrowing: Self.narrowing(of: requested),
            position: 0
        )
        // The list was read under the view asked for when it was built; one
        // asked for since is asked for now.
        if view == requested {
            answersView = true
        }
        else {
            show(view)
        }
    }

    /// Open the reads that follow what surrounds the list; they end with the
    /// list.
    private func openSurroundings(of handle: ImportListHandle) {
        let importStore = importStore
        let onSelection = onSelection
        let failed: @MainActor @Sendable (any Error) -> Void = {
            [weak self] error in
            self?.fail(error)
        }
        surroundings = [
            ScreenSubscription(
                handle.summary(),
                onValue: { importStore.applySummary($0) },
                onError: failed
            ),
            ScreenSubscription(
                handle.filterEntries(),
                onValue: { importStore.applyPendingFilterEntries($0) },
                onError: failed
            ),
            ScreenSubscription(
                handle.selection(),
                onValue: { onSelection($0) },
                onError: failed
            ),
        ]
    }
}

#if DEBUG
    extension ImportListSlot {
        // periphery:ignore - called by baeTests, through the fixtures' `preview`
        /// A slot over a fixed set of items. The list is seeded synchronously
        /// so a canvas draws without a live query; `pendingFilterEntries` are
        /// what the import store holds as the filter's pending entries.
        static func seededWithItems(
            importStore: ImportStore,
            uiStore: UiStore,
            items: [BridgeImportListItem],
            selection: ImportSelection,
            pendingFilterEntries: [BridgePendingFilterEntry]
        ) -> ImportListSlot {
            let tab = uiStore.importCandidateTab
            let handle = ImportListHandle.fixed(items: items, tab: tab)
            let slot = ImportListSlot(
                importStore: importStore,
                uiStore: uiStore,
                selection: selection,
                makeHandle: { _ in handle }
            )
            importStore.applyPendingFilterEntries(pendingFilterEntries)
            let list = PaginatedList<BridgeImportListItem>(
                queries: handle.queries,
                onError: { _ in }
            )
            list.preloadRows(rows: items)
            slot.list = list
            slot.handle = handle
            return slot
        }
    }
#endif

// MARK: - Folds

extension ImportListSlot {
    /// Fold one group open or shut. Its entries leave the list when it folds,
    /// so this changes what every later position holds.
    func setGroupExpanded(
        _ group: BridgeFolderReleaseDecisionKey,
        _ expanded: Bool
    ) {
        foldGroups { try await $0.setGroupFolded(group, !expanded) }
    }

    /// Fold every group open or shut as one change.
    func setAllGroupsExpanded(_ expanded: Bool) {
        foldGroups { try await $0.setAllGroupsFolded(!expanded) }
    }

    /// Send a fold to core's list once the folds asked for before it have
    /// landed, so they land in the order the person made them; a failure is
    /// reported. A list whose read failed is rebuilt instead, and the rebuilt
    /// list starts with no group folded.
    private func foldGroups(
        _ fold: @escaping @MainActor (ImportListHandle) async throws -> Void
    ) {
        if sourceFailed {
            startLoad()
            return
        }
        guard let handle else { return }
        let previous = foldTask
        foldTask = Task { [uiStore] in
            await previous?.value
            do {
                try await fold(handle)
            }
            catch {
                uiStore.showError(error)
            }
        }
    }
}

// MARK: - Place

extension ImportListSlot {
    /// Where the candidate on top of `place` sits now in the list the view
    /// shows, as core places it: `nil` when the place's row is not a
    /// candidate's, or core places it where this view does not show it — on
    /// another tab, or where the view's filters would put it elsewhere. A
    /// failed read is reported, and answers `nil`.
    func position(ofPlace place: ImportListPlace) async -> Int? {
        guard case .candidate(let stableKey, let candidateKey) = place.row,
            let handle
        else { return nil }
        do {
            guard let location = try await handle.locateCandidate(candidateKey),
                let list
            else { return nil }
            let position = Int(location.visiblePosition)
            let placed = await list.holds(stableKey, at: position)
            guard !Task.isCancelled, placed else { return nil }
            return position
        }
        catch {
            uiStore.showError(error)
            return nil
        }
    }
}

// MARK: - Selection

extension ImportListSlot {
    /// Apply what a person did to the list's rows: `change` goes to core,
    /// and the windows holding the rows it changed show it.
    func changeSelection(_ change: BridgeSelectionChange) {
        writeSelection { try await $0.change(change) }
    }

    /// Make `candidateKey` the whole selection, once the writes asked for
    /// before it have landed.
    func selectOnly(_ candidateKey: Data) async throws {
        try await queueSelectionWrite {
            try await $0.change(.replace(keys: [candidateKey]))
        }
        .value
    }

    /// Select every row the list shows under its view, loaded or not.
    func selectAllShown() {
        writeSelection { try await $0.selectAll() }
    }

    /// Run `write` once every selection write asked for before it has ended,
    /// so the writes land in the order the person made them; a failure is
    /// reported.
    private func writeSelection(
        _ write: @escaping @MainActor (ImportSelection) async throws -> Void
    ) {
        let task = queueSelectionWrite(write)
        Task { [uiStore] in
            if case .failure(let error) = await task.result {
                uiStore.showError(error)
            }
        }
    }

    private func queueSelectionWrite(
        _ write: @escaping @MainActor (ImportSelection) async throws -> Void
    ) -> Task<Void, any Error> {
        let previous = selectionTask
        let task = Task { [selection] in
            await previous?.value
            try await write(selection)
        }
        selectionTask = Task { _ = await task.result }
        return task
    }
}

extension PaginatedList<BridgeImportListItem> {
    /// Whether the list holds `stableKey` at `position`, once the window that
    /// covers it has answered.
    fileprivate func holds(_ stableKey: String, at position: Int) async -> Bool
    {
        await withPage(containing: position) {
            idAt(position) == stableKey
        }
    }

    /// The keys of the loaded rows the person has selected, as core's rows
    /// say: what the table highlights. Rows past the loaded windows may be
    /// selected too; core holds the whole selection.
    var selectedKeys: Set<Data> {
        Set(
            loadedRows.compactMap { item -> Data? in
                switch item {
                case .candidate(_, let row, _):
                    row.selected ? row.candidateKey : nil
                case .invalid(_, let invalid, let selected):
                    selected ? invalid.candidateKey : nil
                case .groupHeader:
                    nil
                }
            }
        )
    }

    /// Whether the text filter or the filter entry narrows the list, as the
    /// list's count says.
    var isNarrowed: Bool {
        if case .import(let narrowed)? = countExtra { return narrowed != nil }
        return false
    }

    /// The covers the loaded rows draw, to decode before they are on screen.
    var covers: Set<ImageContent> {
        Set(
            loadedRows.compactMap { item -> ImageContent? in
                guard case .candidate(_, let row, _) = item else { return nil }
                return row.cover.map(ImageContent.init(bridge:))
            }
        )
    }
}
