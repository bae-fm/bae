import BaeKit
import Combine
import Foundation
import Observation

/// Holds the import sidebar's `PaginatedList`, the reads around it and the
/// view they show. The list stays loaded while the view changes.
///
/// The view — which tab, which filter text, which order — and the groups
/// folded shut decide which items sit in which windows, so they are core's
/// list's rather than applied to a window after it arrives. A view change is
/// a write. When the new view has another tab, filter or search, the write
/// returns where the new view opens; the slot then drops the list's windows
/// and count and reads them again from that place, so core reads every value
/// they deliver after the write. Until the first window's rows arrive, the
/// rows the list holds are the previous view's, which the sidebar does not
/// show as this one's. Which groups are folded is core's alone; each
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
    /// read under it have delivered their rows. Before then the list's rows
    /// are the previous view's.
    private(set) var answersView = true
    private(set) var sortOrder: BridgeImportListOrder
    /// The row each opening of the list puts on top: its first selected row,
    /// or its top. Core answers an opening only when another tab, filter or
    /// search narrows the list anew, and the table scrolls to each position
    /// `openings` sends. A table not shown then finds the opening in
    /// `place`.
    @ObservationIgnored
    let openings = PassthroughSubject<Int, Never>()
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
    /// The keys of the rows the person selected, which the table draws its
    /// highlight from and each view change and fold passes to core.
    let selection: ImportSelection
    /// Where the selection's values go beside the selection the panes read.
    @ObservationIgnored
    private let onSelection:
        @MainActor @Sendable (BridgeSelectionSummary) -> Void
    /// The reads around the list: the summary, Found's filter entries and
    /// the selection.
    @ObservationIgnored
    private var surroundings: [any Sendable] = []
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
    /// the list whose reads have ended.
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
        selection.onError = { [uiStore] in uiStore.showError($0) }
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

    /// How many of the tab's rows the list shows, when core counted the list
    /// under `narrowing`, the filter the field holds now; nil when core
    /// counted the rows for another filter or has not counted them yet, so
    /// the field shows a count only next to the filter core counted it for.
    func narrowedCount(
        under narrowing: BridgeImportListNarrowing
    ) -> BridgeNarrowedCount? {
        guard answersView, narrowing == Self.narrowing(of: view) else {
            return nil
        }
        return list?.narrowed
    }

    /// Open the list at `position`: the person's place is its row, at its
    /// top, and the table shown scrolls there.
    private func open(at position: Int) {
        place = ImportListPlace(row: .atPosition, position: position, offset: 0)
        openings.send(position)
    }

    /// How many rows each tab holds, which core sends with the list's count.
    /// The value is `nil` until core sends the count.
    var tabCounts: BridgeTriageTabCounts? {
        guard let count = list?.count else { return nil }
        guard case .import(_, let tabs) = count.extra else {
            preconditionFailure(
                "the import list's count carried \(count.extra)"
            )
        }
        return tabs
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
            _ = try await handle.setGroupFolded(
                groupKey,
                false,
                selected: selection.keys.inByteOrder
            )
        }
        var next = view
        next.tab = location.tab
        next.filterText = ""
        next.pendingFilter = .all
        view = next
        answersView = false
        // The reveal's own place replaces the one the view would open at,
        // and the candidate alone replaces the selection.
        _ = try await handle.setView(
            next,
            selected: selection.keys.inByteOrder
        )
        selectOnly(candidateKey)
        guard let list else { return nil }
        let position = Int(location.visiblePosition)
        await list.reopen(at: location.key, position: position)
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
            let generation = selection.generation
            let change: BridgeImportListViewChange?
            do {
                change = try await handle.setView(
                    next,
                    selected: selection.keys.inByteOrder
                )
            }
            catch {
                fail(error)
                return
            }
            // The rows the new tab, filter or search doesn't show leave the
            // selection, even when the person has asked for another view
            // since this change started.
            if let change, selection.generation == generation {
                selection.remove(change.left)
            }
            guard view == next else { return }
            switch change?.opening {
            case .top:
                await list.reopen(at: nil, position: 0)
                open(at: 0)
            case .at(let key, let position):
                await list.reopen(at: key, position: Int(position))
                open(at: Int(position))
            case nil:
                // A change of order alone answers no opening: the table keeps
                // its scroll position, and the rows redraw in the new order
                // under it.
                await list.reorder()
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
        // A list built anew opens at its top.
        open(at: 0)
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
    /// list. The selection starts empty on the list.
    private func openSurroundings(of handle: ImportListHandle) {
        let importStore = importStore
        let selectionQuery = handle.selection()
        selection.attach(selectionQuery)
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
                selectionQuery.values,
                onValue: { onSelection($0) },
                onError: failed
            ),
        ]
    }
}

#if DEBUG
    extension ImportListSlot {
        // periphery:ignore - called by baeTests, through the fixtures' `preview`
        /// A slot over a fixed set of items, with `tabCounts` as the list's
        /// count carries them. The list is seeded synchronously so a canvas
        /// draws without a live query.
        static func seededWithItems(
            importStore: ImportStore,
            uiStore: UiStore,
            items: [BridgeImportListItem],
            tabCounts: BridgeTriageTabCounts,
            selection: ImportSelection
        ) -> ImportListSlot {
            let tab = uiStore.importCandidateTab
            let extra = BridgeListCountExtra.import(
                narrowed: nil,
                tabs: tabCounts
            )
            let handle = ImportListHandle.fixed(
                items: items,
                tab: tab,
                extra: extra
            )
            let slot = ImportListSlot(
                importStore: importStore,
                uiStore: uiStore,
                selection: selection,
                makeHandle: { _ in handle }
            )
            let list = PaginatedList<BridgeImportListItem>(
                queries: handle.queries,
                onError: { _ in }
            )
            list.preloadRows(rows: items, extra: extra)
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
        foldGroups {
            try await $0.setGroupFolded(group, !expanded, selected: $1)
        }
    }

    /// Fold every group open or shut as one change.
    func setAllGroupsExpanded(_ expanded: Bool) {
        foldGroups { try await $0.setAllGroupsFolded(!expanded, selected: $1) }
    }

    /// Send a fold to core's list, with the keys the selection holds then,
    /// once the folds asked for before it have finished, so core makes them
    /// in the order the person made them. Then take the selected rows the
    /// fold hid out of the selection. The slot reports a failure. The slot
    /// rebuilds a list whose read failed instead, and the rebuilt list
    /// starts with no group folded.
    private func foldGroups(
        _ fold:
            @escaping @MainActor (ImportListHandle, [Data]) async throws ->
            [Data]
    ) {
        if sourceFailed {
            startLoad()
            return
        }
        guard let handle else { return }
        let previous = foldTask
        foldTask = Task { [uiStore, selection] in
            await previous?.value
            do {
                let generation = selection.generation
                let hidden = try await fold(handle, selection.keys.inByteOrder)
                guard selection.generation == generation else { return }
                selection.remove(hidden)
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
    /// shows, as core places it under the view's filters and folds: `nil`
    /// when the place's row is not a candidate's, or the view does not show
    /// the candidate. The slot reports a failed read and answers `nil`.
    func position(ofPlace place: ImportListPlace) async -> Int? {
        guard case .candidate(let candidateKey) = place.row, let handle
        else { return nil }
        do {
            return try await handle.candidatePosition(candidateKey)
                .map(Int.init)
        }
        catch {
            uiStore.showError(error)
            return nil
        }
    }
}

// MARK: - Selection

extension ImportListSlot {
    /// Make `candidateKey` the whole selection.
    func selectOnly(_ candidateKey: Data) {
        selection.select([candidateKey])
    }

    /// Make `edit`, the change a click or key made, to the selection. Core
    /// reads the rows a Shift-click adds on the tab and in the order the slot
    /// shows.
    func editSelection(_ edit: ImportListSelectionRequest.Edit) {
        selection.edit(edit, on: view.tab, in: view.order)
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

    /// Whether the text filter or the filter entry narrows the list, as the
    /// list's count says; `false` until core delivers the count.
    var isNarrowed: Bool {
        narrowed != nil
    }

    /// How many of the tab's rows the list shows while its filters narrow
    /// it, as the list's count says; `nil` while they narrow nothing, or
    /// until core delivers the count.
    fileprivate var narrowed: BridgeNarrowedCount? {
        guard let count else { return nil }
        guard case .import(let narrowed, _) = count.extra else {
            preconditionFailure(
                "the import list's count carried \(count.extra)"
            )
        }
        return narrowed
    }
}
